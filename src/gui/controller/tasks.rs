//! Tareas asíncronas del controlador: lanzar trabajo en segundo plano y reportar progreso.

use super::Controller;
use crate::actions;
use crate::config::Settings;
use crate::content::curseforge::{self};
use crate::content::modrinth::{self};
use crate::game::mojang::{self};
use crate::gui::model::{Model, Session};
use crate::paths::Paths;
use crate::progress::{Event, Reporter};
use crate::updater::{self};
use anyhow::Result;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_LOG: usize = 20_000;

impl Controller {
    pub fn reporter(&self) -> Reporter {
        let sh = self.sh.clone();
        let ctx = self.ctx.clone();
        Reporter::new(move |e| {
            {
                let mut s = sh.lock().unwrap();
                match e {
                    Event::Stage(t) => {
                        s.logs.push(format!("==> {t}"));
                        s.stage = t;
                        s.progress = None;
                    }
                    Event::Progress { done, total } => s.progress = Some((done, total)),
                    Event::Log(l) => s.logs.push(l),
                    Event::Started => {}
                }
                if s.logs.len() > MAX_LOG {
                    let extra = s.logs.len() - MAX_LOG;
                    s.logs.drain(..extra);
                    s.logs_dropped += extra;
                }
            }
            ctx.request_repaint();
        })
    }

    /// Lanza una tarea asíncrona; los errores se muestran en la barra inferior.
    pub fn spawn<F, Fut>(&self, label: &str, f: F)
    where
        F: FnOnce(reqwest::Client, Paths, Reporter, Arc<Mutex<Session>>) -> Fut,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        {
            let mut s = self.sh.lock().unwrap();
            s.tasks += 1;
            s.stage = label.to_string();
            s.error = None;
        }
        let fut = f(self.client.clone(), self.p.clone(), self.reporter(), self.sh.clone());
        let sh = self.sh.clone();
        let ctx = self.ctx.clone();
        self.rt.spawn(async move {
            let r = fut.await;
            {
                let mut s = sh.lock().unwrap();
                s.tasks = s.tasks.saturating_sub(1);
                s.progress = None;
                s.refresh = true;
                if let Err(e) = r {
                    let msg = format!("{e:#}");
                    s.logs.push(t!("[error] {msg}", msg = msg));
                    s.error = Some(msg);
                }
            }
            ctx.request_repaint();
        });
    }

    /// Manifiesto de versiones en segundo plano (con mensaje claro si no hay conexión).
    pub fn load_manifest(&self) {
        self.sh.lock().unwrap().manifest_error = false;
        self.spawn(t!("Cargando versiones"), |c, p, _rep, sh| async move {
            match mojang::manifest(&c, &p).await {
                Ok(m) => {
                    let mut s = sh.lock().unwrap();
                    s.latest_release = m.latest.release.clone();
                    s.manifest = m.versions;
                    anyhow::Ok(())
                }
                Err(e) => {
                    // Sin conexión al abrir no es un error que interrumpa: «Nueva instancia» ya lo
                    // explica con su botón Reintentar, y las instancias instaladas se juegan igual.
                    let mut s = sh.lock().unwrap();
                    s.manifest_error = true;
                    s.logs.push(t!("[brasa] Lista de versiones no disponible: {e:#}", e = e));
                    anyhow::Ok(())
                }
            }
        });
    }

    /// .mrpack (Modrinth) o .zip (CurseForge) → instancia nueva.
    pub fn import_pack_file(&self, settings: &Settings, path: PathBuf) {
        let conc = settings.concurrency;
        let key = settings.curseforge_api_key.clone();
        self.spawn(t!("Importando modpack"), move |c, p, rep, _sh| async move {
            let is_zip = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false);
            let i = if is_zip {
                curseforge::import_zip(&c, &key, &p, &path, conc, &rep).await?
            } else {
                modrinth::import_mrpack(&c, &p, &path, conc, &rep).await?
            };
            actions::install_only(&c, &p, &i.id, false, &rep).await
        });
    }

    /// Arranque: lista de versiones y, si está activado, comprobación de actualizaciones del launcher.
    pub fn start(&self, mdl: &Model) {
        self.presence.configure(crate::discord::Config::from_settings(&mdl.settings));
        self.load_manifest();
        if mdl.settings.check_updates && !mdl.settings.update_repo.is_empty() {
            let repo = mdl.settings.update_repo.clone();
            self.rt.spawn({
                let (c, sh, ctx) = (self.client.clone(), self.sh.clone(), self.ctx.clone());
                async move {
                    if let Ok(Some(rel)) = updater::check(&c, &repo).await {
                        sh.lock().unwrap().update = Some(rel);
                        ctx.request_repaint();
                    }
                }
            });
        }
    }
}

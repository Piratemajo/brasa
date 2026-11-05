//! `Controller::handle`: ejecuta cada [`Action`] cambiando el modelo y lanzando tareas.

use super::Controller;
use crate::account::auth::{self, Account, AccountStore};
use crate::account::skins;
use crate::actions;
use crate::config::{Instance, Settings};
use crate::content::curseforge::{self};
use crate::content::modrinth::{self, ProjectType};
use crate::game::{launch, loaders};
use crate::gui::action::Action;
use crate::gui::model::{cf_class_of, Launch, Model, PackCheck, PingState};
use crate::gui::state::ViewState;
use crate::instance::importers::{self};
use crate::instance::worlds;
use crate::progress::{Event, Reporter};
use crate::updater::{self};
use anyhow::Context;
use std::sync::Arc;

impl Controller {
    pub fn handle(&self, a: Action, mdl: &mut Model, vw: &mut ViewState) {
        match a {
            Action::Select(id) => self.select(mdl, vw, &id),
            Action::ShowCreate => {
                vw.show_create = true;
                mdl.selected = None;
                vw.edit = None;
            }
            Action::Create => {
                let name = if vw.new_name.trim().is_empty() { format!("{} {}", vw.new_loader.label(), vw.new_version) } else { vw.new_name.trim().to_string() };
                let lv = Some(vw.new_loader_version.clone()).filter(|s| !s.is_empty());
                match Instance::create(&self.p, &name, &vw.new_version, vw.new_loader, lv) {
                    Ok(i) => {
                        let id = i.id.clone();
                        mdl.instances = Instance::list(&self.p);
                        self.select(mdl, vw, &id);
                        vw.new_name.clear();
                        self.handle(Action::Install(id, false), mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::LoadLoaderVersions => {
                let (loader, mc) = (vw.new_loader, vw.new_version.clone());
                vw.new_loader_version.clear();
                self.spawn(t!("Buscando versiones del loader"), move |c, p, _r, sh| async move {
                    let (v, stale) = loaders::versions_cached(&c, &p, loader, &mc).await?;
                    let mut s = sh.lock().unwrap();
                    s.loader_versions = v;
                    if stale {
                        s.info = Some(t!("{} no responde: se usa la lista de versiones guardada", loader.label()));
                    }
                    anyhow::Ok(())
                });
            }
            Action::Play(id) => self.launch(mdl, id, None),
            Action::JoinServer(id, ip) => self.launch(mdl, id, Some(ip)),
            Action::PingServers(id) => {
                if mdl.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                let ips: Vec<String> = mdl.servers.iter().map(|s| s.ip().to_string()).filter(|s| !s.is_empty()).collect();
                {
                    let mut s = self.sh.lock().unwrap();
                    for ip in &ips {
                        s.pings.insert(ip.clone(), PingState::Pending);
                    }
                }
                for ip in ips {
                    let (sh, ctx) = (self.sh.clone(), self.ctx.clone());
                    self.rt.spawn(async move {
                        let r = crate::instance::servers::ping(&ip, std::time::Duration::from_secs(5)).await;
                        let st = match r {
                            Ok(st) => PingState::Ok(st),
                            Err(e) => PingState::Failed(format!("{e:#}")),
                        };
                        sh.lock().unwrap().pings.insert(ip, st);
                        ctx.request_repaint();
                    });
                }
            }
            Action::AddServer(id, name, ip) => {
                let r = (|| -> anyhow::Result<()> {
                    crate::instance::servers::parse_address(&ip)?;
                    let g = Instance::load(&self.p, &id)?.game_dir(&self.p);
                    let mut list = crate::instance::servers::load(&g)?;
                    let name = if name.trim().is_empty() { ip.trim().to_string() } else { name };
                    list.push(crate::instance::servers::Server::new(&name, &ip));
                    crate::instance::servers::save(&g, &list)
                })();
                match r {
                    Ok(()) => {
                        vw.new_server_name.clear();
                        vw.new_server_ip.clear();
                        mdl.load_selected_lists();
                        self.handle(Action::PingServers(id), mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(t!("No se pudo añadir el servidor: {e:#}", e = e)),
                }
            }
            Action::RemoveServer(id, idx) => {
                vw.confirm_server_delete = None;
                let r = (|| -> anyhow::Result<()> {
                    let g = Instance::load(&self.p, &id)?.game_dir(&self.p);
                    let mut list = crate::instance::servers::load(&g)?;
                    if idx < list.len() {
                        list.remove(idx);
                        crate::instance::servers::save(&g, &list)?;
                    }
                    Ok(())
                })();
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.load_selected_lists();
            }
            Action::AddContent(id, kind) => {
                use crate::content::resources::Kind;
                let Ok(inst) = Instance::load(&self.p, &id) else { return };
                let (filter, title) = match kind {
                    Kind::ResourcePack => (t!("Texturas"), t!("Añadir texturas (.zip)")),
                    Kind::Shader => (t!("Shaders"), t!("Añadir shaders (.zip)")),
                    Kind::Screenshot => (t!("Imágenes"), t!("Añadir capturas")),
                };
                let Some(files) = rfd::FileDialog::new().add_filter(filter, kind.extensions()).set_title(title).pick_files() else { return };
                match crate::content::resources::add(&inst.game_dir(&self.p), kind, &files) {
                    Ok(n) => self.sh.lock().unwrap().info = Some(t!("{n} añadidos a {}", inst.name, n = n)),
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
                mdl.load_selected_lists();
                if kind == Kind::Screenshot {
                    self.handle(Action::MakeThumbs(id), mdl, vw);
                }
            }
            Action::DeleteContent(id, kind, path) => {
                vw.confirm_content_delete = None;
                let r = Instance::load(&self.p, &id).and_then(|i| crate::content::resources::delete(&i.game_dir(&self.p), kind, &path));
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.load_selected_lists();
            }
            Action::TogglePack(id, name, on) => {
                let r = Instance::load(&self.p, &id).and_then(|i| crate::content::resources::set_pack_enabled_in(&i.game_dir(&self.p), &name, on));
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.load_selected_lists();
            }
            Action::SetShader(id, shader) => {
                let names: Vec<String> = mdl.mods.iter().filter(|m| m.enabled).map(|m| m.file_name.clone()).collect();
                let loader = crate::content::resources::shader_loader(&names);
                let r =
                    Instance::load(&self.p, &id).and_then(|i| crate::content::resources::set_active_shader(&i.game_dir(&self.p), shader.as_deref(), loader));
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.load_selected_lists();
            }
            Action::SetSort(sort) => {
                mdl.ui.sort = sort;
                if let Err(e) = mdl.ui.save(&self.p) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
            }
            Action::ToggleGroup(g) => {
                mdl.ui.toggle(&g);
                let _ = mdl.ui.save(&self.p);
            }
            Action::PickIcon(id) => {
                let Some(file) =
                    rfd::FileDialog::new().set_title(t!("Icono de la instancia")).add_filter(t!("Imagen"), &["png", "jpg", "jpeg", "webp", "gif"]).pick_file()
                else {
                    return;
                };
                let r = Instance::load(&self.p, &id).and_then(|mut i| {
                    let name = crate::instance::organize::set_icon(&i.dir(&self.p), &file)?;
                    i.icon = Some(name);
                    i.save(&self.p)?;
                    Ok(i.icon)
                });
                match r {
                    Ok(icon) => {
                        // el borrador también, para que «Guardar cambios» no lo pise con el icono viejo
                        if let Some(e) = vw.edit.as_mut().filter(|e| e.id == id) {
                            e.icon = icon;
                        }
                        self.reload(mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::RemoveIcon(id) => {
                let r = Instance::load(&self.p, &id).and_then(|mut i| {
                    crate::instance::organize::remove_icons_except(&i.dir(&self.p), None);
                    i.icon = None;
                    i.save(&self.p)
                });
                match r {
                    Ok(()) => {
                        if let Some(e) = vw.edit.as_mut().filter(|e| e.id == id) {
                            e.icon = None;
                        }
                        self.reload(mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::MakeThumbs(id) => {
                if mdl.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                let (items, cache) = (mdl.shots.clone(), self.p.cache());
                let ctx = self.ctx.clone();
                self.rt.spawn_blocking(move || {
                    if crate::content::resources::make_thumbs(&cache, &items) > 0 {
                        ctx.request_repaint();
                    }
                });
            }
            Action::CheckPackUpdate(id) => {
                let Some(info) = mdl.pack.clone().filter(|_| mdl.selected.as_deref() == Some(id.as_str())) else { return };
                self.sh.lock().unwrap().pack_checks.insert(id.clone(), PackCheck::Checking);
                let (c, s, sh, ctx) = (self.client.clone(), mdl.settings.clone(), self.sh.clone(), self.ctx.clone());
                self.rt.spawn(async move {
                    let st = match crate::content::modpacks::check_latest(&c, &s, &info).await {
                        Ok(l) if l.id == info.version_id => PackCheck::UpToDate,
                        Ok(l) => PackCheck::Available(l.name),
                        Err(e) => PackCheck::Failed(format!("{e:#}")),
                    };
                    sh.lock().unwrap().pack_checks.insert(id, st);
                    ctx.request_repaint();
                });
            }
            Action::UpdatePack(id) => {
                self.sh.lock().unwrap().pack_checks.insert(id.clone(), PackCheck::Updating);
                self.spawn(t!("Actualizando modpack"), move |c, p, rep, sh| async move {
                    let s = Settings::load(&p);
                    let r = crate::content::modpacks::update(&c, &p, &s, &id, &rep).await;
                    let mut st = sh.lock().unwrap();
                    match &r {
                        Ok(v) => {
                            st.info = Some(t!("Modpack actualizado a {v}. Tus mundos se copiaron antes.", v = v));
                            st.pack_checks.insert(id, PackCheck::UpToDate);
                        }
                        Err(e) => {
                            st.pack_checks.insert(id, PackCheck::Failed(format!("{e:#}")));
                        }
                    }
                    r.map(|_| ())
                });
            }
            Action::OpenFile(path) => {
                let _ = open::that(path);
            }
            Action::Install(id, verify) => {
                let label = if verify { t!("Reparando") } else { t!("Instalando") };
                self.spawn(label, move |c, p, rep, _sh| async move { actions::install_only(&c, &p, &id, verify, &rep).await });
            }
            Action::OpenDir(d) => {
                let _ = std::fs::create_dir_all(&d);
                let _ = open::that(d);
            }
            Action::Delete(id) => {
                if let Err(e) = Instance::delete(&self.p, &id) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.selected = None;
                vw.edit = None;
                self.reload(mdl, vw);
            }
            Action::Duplicate(id) => {
                let r = Instance::load(&self.p, &id).and_then(|i| {
                    let name = t!("{} (copia)", i.name);
                    i.clone_to(&self.p, &name)
                });
                match r {
                    Ok(n) => {
                        mdl.instances = Instance::list(&self.p);
                        self.select(mdl, vw, &n.id);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::SaveInstance => {
                if let Some(mut e) = vw.edit.clone() {
                    e.jvm_args = vw.edit_jvm.split_whitespace().map(String::from).collect();
                    e.java_path = Some(vw.edit_java.trim().to_string()).filter(|s| !s.is_empty());
                    e.join_server = Some(vw.edit_server.trim().to_string()).filter(|s| !s.is_empty());
                    e.group = crate::instance::organize::clean_group(&vw.edit_group);
                    e.env_vars = vw.edit_env.lines().map(|l| l.trim().to_string()).filter(|l| l.contains('=')).collect();
                    e.wrapper = Some(vw.edit_wrapper.trim().to_string()).filter(|s| !s.is_empty());
                    e.pre_launch = Some(vw.edit_pre.trim().to_string()).filter(|s| !s.is_empty());
                    e.post_exit = Some(vw.edit_post.trim().to_string()).filter(|s| !s.is_empty());
                    if !vw.edit_res {
                        e.width = None;
                        e.height = None;
                    }
                    // Si cambia versión o loader hay que re-resolver.
                    if let Ok(old) = Instance::load(&self.p, &e.id) {
                        if old.mc_version != e.mc_version || old.loader != e.loader || old.loader_version != e.loader_version {
                            e.resolved_version = None;
                        }
                    }
                    let res = e.save(&self.p);
                    let mut s = self.sh.lock().unwrap();
                    match res {
                        Ok(()) => s.info = Some(t!("Instancia guardada").into()),
                        Err(err) => s.error = Some(format!("{err:#}")),
                    }
                    drop(s);
                    vw.edit = Some(e);
                    mdl.instances = Instance::list(&self.p);
                }
            }
            Action::BackupWorld(id, world) => {
                self.spawn(t!("Copia de seguridad"), move |_c, p, rep, _sh| async move {
                    let inst = Instance::load(&p, &id)?;
                    rep.stage(t!("Copiando «{world}»", world = world));
                    let b = tokio::task::spawn_blocking(move || worlds::backup_world(&p, &inst, &world)).await??;
                    rep.log(t!("[brasa] Copia guardada: {} ({})", b.file.display(), worlds::fmt_size(b.size)));
                    Ok(())
                });
            }
            Action::RestoreBackup(id, file) => {
                self.spawn(t!("Restaurando copia"), move |_c, p, rep, _sh| async move {
                    let inst = Instance::load(&p, &id)?;
                    rep.stage(t!("Restaurando copia"));
                    let name = tokio::task::spawn_blocking(move || worlds::restore_backup(&p, &inst, &file)).await??;
                    rep.log(t!("[brasa] Mundo «{name}» restaurado (el anterior se conservó aparte en saves/)", name = name));
                    Ok(())
                });
            }
            Action::DeleteBackup(id, file) => {
                vw.confirm_backup_delete = None;
                let r = Instance::load(&self.p, &id).and_then(|i| worlds::delete_backup(&self.p, &i, &file));
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                self.reload(mdl, vw);
            }
            Action::ToggleMod(m) => {
                if let Err(e) = modrinth::toggle_mod(&m) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                self.reload(mdl, vw);
            }
            Action::DeleteMod(path) => {
                let _ = std::fs::remove_file(path);
                self.reload(mdl, vw);
            }
            Action::CheckUpdates(id) => {
                self.spawn(t!("Buscando actualizaciones"), move |c, p, rep, sh| async move {
                    let inst = Instance::load(&p, &id)?;
                    let ups = modrinth::check_updates(&c, &p, &inst).await?;
                    rep.stage(t!("{} actualizaciones disponibles", ups.len()));
                    sh.lock().unwrap().updates = ups;
                    anyhow::Ok(())
                });
            }
            Action::ApplyUpdates(id) => {
                let ups = std::mem::take(&mut self.sh.lock().unwrap().updates);
                self.spawn(t!("Actualizando mods"), move |c, p, rep, _sh| async move {
                    let inst = Instance::load(&p, &id)?;
                    let n = modrinth::apply_updates(&c, &p, &inst, &ups, &rep).await?;
                    rep.stage(t!("{n} mods actualizados", n = n));
                    anyhow::Ok(())
                });
            }
            Action::Search => {
                let target = vw.target.clone().or(mdl.selected.clone());
                let inst = target.and_then(|t| mdl.instances.iter().find(|i| i.id == t).cloned());
                let (q, kind) = (vw.query.clone(), vw.ptype);
                if vw.browse_cf {
                    let key = mdl.settings.curseforge_api_key.clone();
                    let class = cf_class_of(kind);
                    self.spawn(t!("Buscando en CurseForge"), move |c, _p, _rep, sh| async move {
                        let (mc, loader) = match (&inst, class) {
                            (_, curseforge::Class::Modpack) | (None, _) => (None, None),
                            (Some(i), _) => (Some(i.mc_version.clone()), Some(i.loader)),
                        };
                        let hits = curseforge::search(&c, &key, &q, class, mc.as_deref(), loader).await?;
                        sh.lock().unwrap().cf_hits = hits;
                        anyhow::Ok(())
                    });
                    return;
                }
                self.spawn(t!("Buscando en Modrinth"), move |c, _p, _rep, sh| async move {
                    let (mc, loader) = match (&inst, kind) {
                        (_, ProjectType::Modpack) | (None, _) => (None, None),
                        (Some(i), _) => (Some(i.mc_version.clone()), Some(i.loader)),
                    };
                    let hits = modrinth::search(&c, &q, kind, mc.as_deref(), loader).await?;
                    sh.lock().unwrap().hits = hits;
                    anyhow::Ok(())
                });
            }
            Action::InstallProject(pid, kind) => {
                if kind == ProjectType::Modpack {
                    let conc = mdl.settings.concurrency;
                    self.spawn(t!("Instalando modpack"), move |c, p, rep, _sh| async move {
                        let i = modrinth::install_modpack(&c, &p, &pid, conc, &rep).await?;
                        rep.stage(t!("Modpack instalado como '{}'", i.name));
                        actions::install_only(&c, &p, &i.id, false, &rep).await
                    });
                } else {
                    let Some(id) = vw.target.clone().or(mdl.selected.clone()) else {
                        self.sh.lock().unwrap().error = Some(t!("elige una instancia de destino").into());
                        return;
                    };
                    self.spawn(t!("Instalando desde Modrinth"), move |c, p, rep, _sh| async move {
                        let inst = Instance::load(&p, &id)?;
                        let files = modrinth::install_project(&c, &p, &inst, &pid, kind, &rep).await?;
                        rep.stage(t!("Instalado: {}", files.join(", ")));
                        anyhow::Ok(())
                    });
                }
            }
            Action::ImportPack(path) => self.import_pack_file(&mdl.settings, path),
            Action::AddJar(path) => {
                let Some(inst) = vw.edit.clone() else {
                    self.sh.lock().unwrap().error = Some(t!("selecciona una instancia para soltar mods").into());
                    return;
                };
                let dst = inst.mods_dir(&self.p).join(path.file_name().unwrap_or_default());
                let _ = std::fs::create_dir_all(inst.mods_dir(&self.p));
                match std::fs::copy(&path, &dst) {
                    Ok(_) => self.sh.lock().unwrap().info = Some(t!("Añadido {}", dst.display())),
                    Err(e) => self.sh.lock().unwrap().error = Some(e.to_string()),
                }
                self.reload(mdl, vw);
            }
            Action::MsLogin => {
                let cid = mdl.settings.msa_client_id.clone();
                self.spawn(t!("Iniciando sesión con Microsoft"), move |c, p, rep, sh| async move {
                    let dc = auth::start_device_code(&c, &cid).await?;
                    sh.lock().unwrap().device_code = Some((dc.user_code.clone(), dc.verification_uri.clone()));
                    let _ = open::that(&dc.verification_uri);
                    rep.stage(t!("Introduce el código {} en {}", dc.user_code, dc.verification_uri));
                    let res = auth::poll_device_code(&c, &cid, &dc).await;
                    sh.lock().unwrap().device_code = None;
                    let acc = res?;
                    let mut st = AccountStore::load(&p);
                    rep.stage(t!("Sesión iniciada como {}", acc.username));
                    st.upsert(acc);
                    st.save(&p)
                });
            }
            Action::AddOffline => {
                let res = Account::offline(vw.offline_name.trim()).and_then(|a| {
                    let mut st = AccountStore::load(&self.p);
                    st.upsert(a);
                    st.save(&self.p)
                });
                match res {
                    Ok(()) => vw.offline_name.clear(),
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
                self.reload(mdl, vw);
            }
            Action::SkinLogin => {
                let server = if vw.skin_server < crate::account::yggdrasil::PRESETS.len() {
                    crate::account::yggdrasil::PRESETS[vw.skin_server].0.to_string()
                } else {
                    vw.skin_custom.trim().to_string()
                };
                let user = vw.skin_user.trim().to_string();
                // La contraseña no se guarda: se usa una vez para obtener el token.
                let pass = std::mem::take(&mut vw.skin_pass);
                if server.is_empty() || user.is_empty() || pass.is_empty() {
                    self.sh.lock().unwrap().error = Some(t!("rellena servidor, usuario y contraseña").into());
                    return;
                }
                self.spawn(t!("Iniciando sesión"), move |c, p, rep, _sh| async move {
                    let acc = auth::yggdrasil_login(&c, &server, &user, &pass).await?;
                    rep.stage(t!("Sesión iniciada como {} ({})", acc.username, acc.label()));
                    let mut st = AccountStore::load(&p);
                    st.upsert(acc);
                    st.save(&p)
                });
            }
            Action::SetActive(uuid) => {
                let mut st = AccountStore::load(&self.p);
                let _ = st.set_active(&uuid);
                let _ = st.save(&self.p);
                self.reload(mdl, vw);
            }
            Action::RemoveAccount(uuid) => {
                let mut st = AccountStore::load(&self.p);
                st.remove(&uuid);
                let _ = st.save(&self.p);
                self.reload(mdl, vw);
            }
            Action::CfInstall(id, class) => {
                let key = mdl.settings.curseforge_api_key.clone();
                let conc = mdl.settings.concurrency;
                if class == curseforge::Class::Modpack {
                    self.spawn(t!("Instalando modpack de CurseForge"), move |c, p, rep, _sh| async move {
                        let i = curseforge::install_modpack(&c, &key, &p, id, conc, &rep).await?;
                        rep.stage(t!("Modpack instalado como '{}'", i.name));
                        actions::install_only(&c, &p, &i.id, false, &rep).await
                    });
                } else {
                    let Some(target) = vw.target.clone().or(mdl.selected.clone()) else {
                        self.sh.lock().unwrap().error = Some(t!("elige una instancia de destino").into());
                        return;
                    };
                    self.spawn(t!("Instalando desde CurseForge"), move |c, p, rep, _sh| async move {
                        let inst = Instance::load(&p, &target)?;
                        let files = curseforge::install(&c, &key, &p, &inst, id, class, &rep).await?;
                        rep.stage(t!("Instalado: {}", files.join(", ")));
                        anyhow::Ok(())
                    });
                }
            }
            Action::Kill(id) => {
                if !launch::kill(&id) {
                    self.sh.lock().unwrap().error = Some(t!("ese juego no está abierto desde Brasa").into());
                }
            }
            Action::Perf(id) => {
                self.spawn(t!("Instalando perfil de rendimiento"), move |c, p, rep, _sh| async move {
                    let inst = Instance::load(&p, &id)?;
                    let (ok, missing) = modrinth::install_performance(&c, &p, &inst, &rep).await?;
                    let mut msg = t!("Rendimiento: instalados {}", ok.join(", "));
                    if !missing.is_empty() {
                        msg.push_str(&t!(" · no disponibles: {}", missing.join(", ")));
                    }
                    rep.stage(msg);
                    anyhow::Ok(())
                });
            }
            Action::ExportPack(id) => {
                let Ok(inst) = Instance::load(&self.p, &id) else { return };
                let Some(out) = rfd::FileDialog::new()
                    .set_title(t!("Exportar como .mrpack"))
                    .set_file_name(format!("{}.mrpack", crate::paths::sanitize(&inst.name)))
                    .add_filter(t!("Modpack de Modrinth"), &["mrpack"])
                    .save_file()
                else {
                    return;
                };
                let saves = vw.export_saves;
                self.spawn(t!("Exportando modpack"), move |c, p, rep, sh| async move {
                    let (refd, emb) = modrinth::export_mrpack(&c, &p, &inst, &out, saves, &rep).await?;
                    sh.lock().unwrap().info =
                        Some(t!("Exportado {} ({refd} enlazados a Modrinth, {emb} dentro del pack)", out.display(), refd = refd, emb = emb));
                    anyhow::Ok(())
                });
            }
            Action::PickJars => {
                if let Some(files) = rfd::FileDialog::new().add_filter(t!("Mods"), &["jar"]).set_title(t!("Añadir mods")).pick_files() {
                    for f in files {
                        self.handle(Action::AddJar(f), mdl, vw);
                    }
                }
            }
            Action::PickPack => {
                if let Some(f) = rfd::FileDialog::new()
                    .add_filter(t!("Modpacks"), &["mrpack", "zip"])
                    .set_title(t!("Importar modpack (.mrpack de Modrinth o .zip de CurseForge)"))
                    .pick_file()
                {
                    self.import_pack_file(&mdl.settings, f);
                }
            }
            Action::ScanImport => {
                let latest = self.sh.lock().unwrap().latest_release.clone();
                let mut v = importers::scan_prism();
                v.extend(importers::scan_curseforge());
                v.extend(importers::scan_atlauncher());
                v.extend(importers::scan_official(&latest));
                if v.is_empty() {
                    self.sh.lock().unwrap().info =
                        Some(t!("No se encontraron instancias de Prism/MultiMC, CurseForge, ATLauncher ni del launcher oficial. Si están en otra carpeta, usa «Elegir carpeta».").into());
                }
                mdl.importable = v;
            }
            Action::ImportFound(i) => {
                let Some(f) = mdl.importable.get(i).cloned() else { return };
                self.spawn(t!("Importando instancia"), move |c, p, rep, _sh| async move {
                    let f2 = f.clone();
                    let p2 = p.clone();
                    let inst = tokio::task::spawn_blocking(move || importers::import(&p2, &f2)).await??;
                    rep.stage(t!("Importada '{}' desde {}", inst.name, f.source));
                    actions::install_only(&c, &p, &inst.id, false, &rep).await
                });
            }
            Action::ImportFolder => {
                let Some(dir) = rfd::FileDialog::new().set_title(t!("Carpeta de una instancia (Prism, MultiMC, CurseForge o ATLauncher)")).pick_folder() else {
                    return;
                };
                match importers::read_any(&dir) {
                    Ok(f) => {
                        mdl.importable.insert(0, f);
                        self.handle(Action::ImportFound(0), mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::NetTest => {
                self.sh.lock().unwrap().net_report = Some(t!("Probando…").into());
                self.spawn(t!("Probando conexión"), |c, _p, _rep, sh| async move {
                    let r = actions::network_report(&c).await;
                    sh.lock().unwrap().net_report = Some(r);
                    anyhow::Ok(())
                });
            }
            Action::CheckUpdate => {
                let repo = mdl.settings.update_repo.clone();
                self.spawn(t!("Buscando actualizaciones de Brasa"), move |c, _p, _rep, sh| async move {
                    let r = updater::check(&c, &repo).await?;
                    let mut s = sh.lock().unwrap();
                    match r {
                        Some(rel) => s.update = Some(rel),
                        None => s.info = Some(t!("Brasa {} está al día", env!("CARGO_PKG_VERSION"))),
                    }
                    anyhow::Ok(())
                });
            }
            Action::ApplyUpdate => {
                let Some(rel) = self.sh.lock().unwrap().update.clone() else { return };
                self.spawn(t!("Actualizando Brasa"), move |c, _p, _rep, sh| async move {
                    updater::apply(&c, &rel).await?;
                    let mut s = sh.lock().unwrap();
                    s.update = None;
                    s.info = Some(t!("Actualizado a {}: cierra y vuelve a abrir Brasa", rel.tag_name));
                    anyhow::Ok(())
                });
            }
            Action::SelectCharacter(uuid, profile) => {
                let cid = mdl.settings.msa_client_id.clone();
                self.spawn(t!("Cambiando de personaje"), move |c, p, rep, _sh| async move {
                    let mut st = AccountStore::load(&p);
                    let acc = st.accounts.iter().find(|a| a.uuid == uuid).cloned().context(t!("cuenta no encontrada"))?;
                    let acc = auth::ensure_fresh(&c, &cid, &acc).await?;
                    let new = auth::yggdrasil_select(&c, &acc, &profile).await?;
                    st.accounts.retain(|a| a.uuid != acc.uuid);
                    rep.stage(t!("Ahora juegas como {}", new.username));
                    st.upsert(new);
                    st.save(&p)
                });
            }
            Action::PickPremiumSkin(slim) => {
                let Some(file) = rfd::FileDialog::new().add_filter(t!("Skin PNG"), &["png"]).set_title(t!("Elegir skin (64×64)")).pick_file() else { return };
                let cid = mdl.settings.msa_client_id.clone();
                self.spawn(t!("Subiendo skin"), move |c, p, rep, _sh| async move {
                    let mut st = AccountStore::load(&p);
                    let acc = st.active().cloned().context(t!("no hay cuenta activa"))?;
                    let acc = auth::ensure_fresh(&c, &cid, &acc).await?;
                    skins::upload_premium(&c, &acc, &file, slim).await?;
                    st.upsert(acc);
                    st.save(&p)?;
                    rep.stage(t!("Skin subida a tu cuenta premium"));
                    anyhow::Ok(())
                });
            }
            Action::ResetPremiumSkin => {
                let cid = mdl.settings.msa_client_id.clone();
                self.spawn(t!("Quitando skin"), move |c, p, rep, _sh| async move {
                    let st = AccountStore::load(&p);
                    let acc = st.active().cloned().context(t!("no hay cuenta activa"))?;
                    let acc = auth::ensure_fresh(&c, &cid, &acc).await?;
                    skins::reset_premium(&c, &acc).await?;
                    rep.stage(t!("Skin quitada"));
                    anyhow::Ok(())
                });
            }
            Action::PickOfflineSkin => {
                let Some(file) = rfd::FileDialog::new().add_filter(t!("Skin PNG"), &["png"]).set_title(t!("Elegir skin local (64×64)")).pick_file() else {
                    return;
                };
                if let Err(e) = skins::read_skin(&file) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                    return;
                }
                let mut st = AccountStore::load(&self.p);
                if let Some(id) = st.active.clone() {
                    if let Some(a) = st.accounts.iter_mut().find(|a| a.uuid == id) {
                        a.skin_path = Some(file.to_string_lossy().into_owned());
                    }
                }
                let _ = st.save(&self.p);
                self.sh.lock().unwrap().info = Some(t!("Skin local guardada: se verá al lanzar el juego").into());
                self.reload(mdl, vw);
            }
            Action::ClearOfflineSkin => {
                let mut st = AccountStore::load(&self.p);
                if let Some(id) = st.active.clone() {
                    if let Some(a) = st.accounts.iter_mut().find(|a| a.uuid == id) {
                        a.skin_path = None;
                    }
                }
                let _ = st.save(&self.p);
                self.reload(mdl, vw);
            }
            Action::OpenUrl(u) => {
                let _ = open::that(u);
            }
            Action::RetryManifest => self.load_manifest(),
            Action::ApplySettings(s) => {
                self.presence.configure(crate::discord::Config::from_settings(&s));
                // el idioma cambia al momento, sin reiniciar
                crate::i18n::apply(&s.language);
                mdl.settings = s;
            }
            Action::DismissError => self.sh.lock().unwrap().error = None,
            Action::DismissInfo => self.sh.lock().unwrap().info = None,
            Action::Copy(text, notice) => {
                self.ctx.output_mut(|o| o.copied_text = text);
                self.sh.lock().unwrap().info = Some(notice);
            }
            Action::SetLoadout(key, l) => {
                mdl.wardrobe.set(&key, l);
                if let Err(e) = mdl.wardrobe.save(&mdl.p) {
                    self.sh.lock().unwrap().error = Some(t!("no se pudieron guardar los cosméticos: {e:#}", e = e));
                }
            }
            Action::OpenNotice(n) => {
                let (name, text) = n.file_and_text();
                let path = mdl.p.cache().join("legal").join(name);
                let res = std::fs::create_dir_all(path.parent().unwrap())
                    .and_then(|_| std::fs::write(&path, text))
                    .map_err(anyhow::Error::from)
                    .and_then(|_| open::that(&path).map_err(anyhow::Error::from));
                if let Err(e) = res {
                    self.sh.lock().unwrap().error = Some(t!("no se pudo abrir {name}: {e:#}", name = name, e = e));
                }
            }
            Action::ReportProblem(description, with_log) => {
                let text = crate::gui::view::report::report_text(mdl, &self.sh.lock().unwrap(), &description, with_log);
                self.sh.lock().unwrap().report = Some(crate::gui::model::ReportState::Uploading);
                self.spawn(t!("Subiendo el informe"), move |c, _p, _r, sh| async move {
                    let r = crate::report::upload(&c, &text).await;
                    sh.lock().unwrap().report = Some(match r {
                        Ok(url) => crate::gui::model::ReportState::Done(url),
                        Err(e) => crate::gui::model::ReportState::Failed(format!("{e:#}")),
                    });
                    anyhow::Ok(())
                });
            }
            Action::QuickStart(perf) => {
                let (latest, releases) = {
                    let s = self.sh.lock().unwrap();
                    let rel: Vec<String> = s.manifest.iter().filter(|v| v.kind == "release").map(|v| v.id.clone()).take(8).collect();
                    (s.latest_release.clone(), rel)
                };
                if latest.is_empty() {
                    self.sh.lock().unwrap().error = Some(t!("Aún no se ha cargado la lista de versiones de Minecraft").into());
                    return;
                }
                self.sh.lock().unwrap().quickstart = Some(crate::gui::model::QuickStart { perf, ..Default::default() });
                self.spawn(t!("Preparando tu primera instancia"), move |c, p, rep, sh| async move {
                    let sh2 = sh.clone();
                    let r = async move {
                        // la versión más nueva que ya tiene Sodium para Fabric (los mods tardan unos días en llegar)
                        let mut mc = latest.clone();
                        if perf {
                            rep.stage(t!("Buscando la versión con mods de rendimiento"));
                            for v in &releases {
                                match modrinth::compatible_versions(&c, "sodium", ProjectType::Mod, v, crate::config::Loader::Fabric).await {
                                    Ok(list) if !list.is_empty() => {
                                        mc = v.clone();
                                        break;
                                    }
                                    Ok(_) => continue,
                                    Err(e) => {
                                        rep.log(format!("[brasa] Modrinth: {e:#}"));
                                        break;
                                    }
                                }
                            }
                        }
                        let (name, loader) = if perf {
                            (t!("Rendimiento {}", mc), crate::config::Loader::Fabric)
                        } else {
                            (format!("Minecraft {mc}"), crate::config::Loader::Vanilla)
                        };
                        let inst = Instance::create(&p, &name, &mc, loader, None)?;
                        {
                            let mut s = sh.lock().unwrap();
                            if let Some(q) = &mut s.quickstart {
                                q.id = Some(inst.id.clone());
                                q.name = inst.name.clone();
                                q.mc = mc.clone();
                            }
                            s.refresh = true;
                        }
                        if perf {
                            let (ok, missing) = modrinth::install_performance(&c, &p, &inst, &rep).await?;
                            if !missing.is_empty() {
                                rep.log(format!("[brasa] Sin versión para {mc}: {}", missing.join(", ")));
                            }
                            if let Some(q) = &mut sh.lock().unwrap().quickstart {
                                q.mods = ok.len();
                            }
                        }
                        actions::install_only(&c, &p, &inst.id, false, &rep).await
                    }
                    .await;
                    if let Some(q) = &mut sh2.lock().unwrap().quickstart {
                        q.done = true;
                        q.error = r.as_ref().err().map(|e| format!("{e:#}"));
                    }
                    r
                });
            }
            Action::FinishOnboarding => {
                mdl.settings.language = vw.settings.language.clone();
                mdl.settings.onboarded = true;
                vw.settings.onboarded = true;
                if let Err(e) = mdl.settings.save(&self.p) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                self.reload(mdl, vw);
                let id = self.sh.lock().unwrap().quickstart.as_ref().and_then(|q| q.id.clone());
                if let Some(id) = id.filter(|id| mdl.instances.iter().any(|i| &i.id == id)) {
                    vw.show_create = false;
                    self.select(mdl, vw, &id);
                } else if mdl.selected.is_none() {
                    if let Some(first) = mdl.instances.first().map(|i| i.id.clone()) {
                        vw.show_create = false;
                        self.select(mdl, vw, &first);
                    }
                }
            }
            Action::HostCreate => {
                let f = vw.hosting.form.clone();
                match crate::hosting::Server::create(&self.p, &f.name, f.kind, &f.version, None, !f.no_premium) {
                    Ok(mut srv) => {
                        srv.eula_accepted = f.eula;
                        if let Err(e) = srv.save(&self.p).and_then(|_| srv.write_files(&self.p)) {
                            self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                        }
                        mdl.hosted = crate::hosting::Server::list(&self.p);
                        vw.hosting.selected = Some(srv.id.clone());
                        vw.hosting.creating = false;
                        vw.hosting.tab = crate::gui::view::hosting::HostTab::Console;
                        vw.hosting.draft = None;
                        self.sh.lock().unwrap().info = Some(t!("Servidor creado: pulsa Iniciar (la primera vez descarga el software)").into());
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::HostStart(id) => {
                let ctx = self.ctx.clone();
                let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
                let (c, p) = (self.client.clone(), self.p.clone());
                let sh = self.sh.clone();
                self.rt.spawn(async move {
                    let _ = crate::hosting::run::start(c, p, id, notify).await;
                    sh.lock().unwrap().refresh = true;
                });
            }
            Action::HostStop(id) => crate::hosting::run::stop(&id),
            Action::HostKill(id) => crate::hosting::run::kill(&id),
            Action::HostCommand(id, cmd) => {
                if !crate::hosting::run::send(&id, &cmd) {
                    self.sh.lock().unwrap().error = Some(t!("el servidor no está en marcha").into());
                }
            }
            Action::HostSave(id, d) => {
                let r = crate::hosting::Server::update(&self.p, &id, |srv| {
                    if d.name.trim().is_empty() {
                        anyhow::bail!("el nombre no puede quedar vacío");
                    }
                    if d.port != srv.port {
                        // el túnel de playit apunta al puerto viejo: se creará otro
                        srv.playit_tunnel = None;
                    }
                    srv.name = d.name.trim().to_string();
                    srv.memory_mb = d.memory_mb;
                    srv.port = d.port;
                    srv.online_mode = d.online_mode;
                    srv.whitelist = d.whitelist;
                    srv.lan_open = d.lan_open;
                    let path = srv.dir(&self.p).join("server.properties");
                    let mut pr = crate::hosting::props::Properties::load(&path);
                    d.props.write(&mut pr);
                    pr.save(&path)?;
                    srv.write_files(&self.p)?;
                    Ok(())
                });
                match r {
                    Ok(()) => self.sh.lock().unwrap().info = Some(t!("Ajustes del servidor guardados").into()),
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
                vw.hosting.draft = None;
            }
            Action::HostDelete(id) => {
                match crate::hosting::Server::delete(&self.p, &id) {
                    Ok(()) => {
                        vw.hosting.selected = None;
                        self.sh.lock().unwrap().info = Some(t!("Servidor borrado").into());
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::HostSearch(id, q) => {
                let Ok(srv) = crate::hosting::Server::load(&self.p, &id) else { return };
                self.sh.lock().unwrap().host_searching = true;
                self.spawn(t!("Buscando en Modrinth"), move |c, _p, _rep, sh| async move {
                    let r = crate::hosting::content::search(&c, &srv, &q).await;
                    let mut s = sh.lock().unwrap();
                    s.host_searching = false;
                    s.host_hits = r?;
                    anyhow::Ok(())
                });
            }
            Action::HostInstall(id, project) => {
                self.spawn(t!("Instalando en el servidor"), move |c, p, rep, sh| async move {
                    let added = crate::hosting::content::install(&c, &p, &id, &project, &rep).await?;
                    let names: Vec<String> = added.iter().map(|a| a.title.clone()).collect();
                    let mut s = sh.lock().unwrap();
                    s.info = Some(t!("Instalado: {} (se carga al reiniciar el servidor)", names.join(", ")));
                    s.refresh = true;
                    anyhow::Ok(())
                });
            }
            Action::HostRemove(id, file) => {
                if let Err(e) = crate::hosting::content::remove(&self.p, &id, &file) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::HostJoin(id) => {
                let r = crate::hosting::Server::load(&self.p, &id).and_then(|srv| {
                    let addr = crate::hosting::address("localhost", srv.port);
                    crate::hosting::content::join_instance(&self.p, &srv, &addr)
                });
                match r {
                    Ok(inst) => {
                        mdl.instances = Instance::list(&self.p);
                        vw.tab = crate::gui::state::Tab::Instances;
                        vw.show_create = false;
                        self.select(mdl, vw, &inst.id);
                        self.sh.lock().unwrap().info = Some(t!("Instancia «{}» creada, con el servidor en su lista", inst.name));
                        self.handle(Action::Install(inst.id, false), mdl, vw);
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
            }
            Action::HostSetPlayit(id, on) => {
                let r = crate::hosting::Server::update(&self.p, &id, |srv| {
                    srv.playit = on;
                    Ok(())
                });
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::PlayitLink => {
                let code = crate::hosting::playit::claim_code();
                let url = crate::hosting::playit::claim_url(&code);
                let _ = open::that(&url);
                let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
                {
                    let mut s = self.sh.lock().unwrap();
                    s.playit_claim = Some(url);
                    s.playit_cancel = Some(cancel.clone());
                }
                let (p, sh, ctx) = (self.p.clone(), self.sh.clone(), self.ctx.clone());
                self.rt.spawn(async move {
                    let r = crate::hosting::playit::link(&p, &code, cancel).await;
                    let mut s = sh.lock().unwrap();
                    s.playit_claim = None;
                    s.playit_cancel = None;
                    s.refresh = true;
                    match r {
                        Ok(()) => s.info = Some(t!("playit.gg enlazado: activa el túnel en el servidor").into()),
                        Err(e) => {
                            let msg = format!("{e:#}");
                            if msg != "cancelado" {
                                s.error = Some(msg);
                            }
                        }
                    }
                    drop(s);
                    ctx.request_repaint();
                });
            }
            Action::P2pInvite(server, name, hours) => {
                {
                    let mut s = self.sh.lock().unwrap();
                    s.p2p_inviting = true;
                    s.p2p_code = None;
                }
                self.spawn(t!("Creando la invitación"), move |_c, p, _rep, sh| async move {
                    let r = crate::hosting::p2p::invite(&p, &server, &name, hours).await;
                    let mut s = sh.lock().unwrap();
                    s.p2p_inviting = false;
                    s.refresh = true;
                    s.p2p_code = Some(r?);
                    anyhow::Ok(())
                });
            }
            Action::P2pJoinReset => self.sh.lock().unwrap().p2p_join = None,
            Action::P2pInviteReset => self.sh.lock().unwrap().p2p_code = None,
            Action::P2pCancelInvite(server, inv) => {
                let r = crate::hosting::Server::update(&self.p, &server, |s| {
                    s.invites.retain(|i| i.id != inv);
                    Ok(())
                });
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::P2pRevoke(server, friend) => {
                let r = crate::hosting::Server::update(&self.p, &server, |s| {
                    let name = s.friends.iter().find(|f| f.id == friend).map(|f| f.name.clone()).unwrap_or_default();
                    s.friends.retain(|f| f.id != friend);
                    Ok(name)
                });
                match r {
                    Ok(name) => {
                        crate::hosting::p2p::kick_revoked(&self.p);
                        self.sh.lock().unwrap().info = Some(t!("{} ya no puede entrar a este servidor", name));
                    }
                    Err(e) => self.sh.lock().unwrap().error = Some(format!("{e:#}")),
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::P2pDecide(key, ok) => crate::hosting::p2p::decide(key, ok),
            Action::HostSetGatekeeper(id, on) => {
                let r = crate::hosting::Server::update(&self.p, &id, |s| {
                    s.gatekeeper = on;
                    Ok(())
                });
                if let Err(e) = r {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.hosted = crate::hosting::Server::list(&self.p);
            }
            Action::SetHideIp(on) => {
                mdl.settings.p2p_hide_ip = on;
                vw.settings.p2p_hide_ip = on;
                if let Err(e) = mdl.settings.save(&self.p) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                // se rehace la conexión con el modo nuevo
                let p = self.p.clone();
                self.rt.spawn(async move {
                    crate::hosting::p2p::shutdown().await;
                    if !crate::hosting::run::running_ids().is_empty() {
                        let _ = crate::hosting::p2p::endpoint(&p).await;
                    }
                });
                self.sh.lock().unwrap().info = Some(if on {
                    t!("Ocultar mi IP: los amigos se conectan siempre por el relé").into()
                } else {
                    t!("Conexión directa activada (más rápida)").into()
                });
            }
            Action::P2pJoin(code) => {
                self.sh.lock().unwrap().p2p_join = Some((crate::hosting::p2p::JoinStep::Code, None));
                self.spawn(t!("Uniéndote al servidor de tu amigo"), move |c, p, rep, sh| async move {
                    use crate::hosting::p2p::{self as p2p, JoinStep};
                    let last = Arc::new(std::sync::Mutex::new(JoinStep::Code));
                    let (sh2, last2) = (sh.clone(), last.clone());
                    let r: anyhow::Result<Vec<String>> = async {
                        let (mut j, w) = p2p::join(&p, &code, move |st| {
                            *last2.lock().unwrap() = st;
                            sh2.lock().unwrap().p2p_join = Some((st, None));
                        })
                        .await?;
                        *last.lock().unwrap() = JoinStep::Preparing;
                        sh.lock().unwrap().p2p_join = Some((JoinStep::Preparing, None));
                        let inst = p2p_instance(&p, &mut j, &w)?;
                        let missing = p2p::sync_mods(&c, &p, &mut j, &inst, &w.mods).await?;
                        p2p::update_joined(&p, &j)?;
                        actions::install_only(&c, &p, &inst.id, false, &rep).await?;
                        Ok(missing)
                    }
                    .await;
                    let mut s = sh.lock().unwrap();
                    s.refresh = true;
                    match r {
                        Ok(missing) => {
                            s.p2p_join = Some((JoinStep::Done, None));
                            s.p2p_missing = missing;
                        }
                        Err(e) => s.p2p_join = Some((*last.lock().unwrap(), Some(format!("{e:#}")))),
                    }
                    anyhow::Ok(())
                });
            }
            Action::P2pPlay(key) => {
                let Some(j) = mdl.joined.iter().find(|j| j.key() == key).cloned() else { return };
                // hay que entrar con el nombre de la invitación: si hay una cuenta así, se usa
                let active = mdl.accounts.active().map(|a| a.username.clone());
                if !active.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&j.name)) {
                    match mdl.accounts.accounts.iter().find(|a| a.username.eq_ignore_ascii_case(&j.name)).map(|a| a.uuid.clone()) {
                        Some(uuid) => self.handle(Action::SetActive(uuid), mdl, vw),
                        None => {
                            self.sh.lock().unwrap().error = Some(t!(
                                "Este servidor te espera como «{n}» y tu cuenta se llama distinto. Añade una cuenta llamada {n} en Cuentas (si es no premium, basta con el nombre).",
                                n = j.name
                            ));
                            return;
                        }
                    }
                }
                self.spawn(t!("Conectando con tu amigo"), move |c, p, _rep, sh| async move {
                    use crate::hosting::p2p;
                    let (port, w) = p2p::open_tunnel(&p, &j).await?;
                    let mut j = j;
                    let inst = p2p_instance(&p, &mut j, &w)?;
                    let missing = p2p::sync_mods(&c, &p, &mut j, &inst, &w.mods).await?;
                    p2p::update_joined(&p, &j)?;
                    let mut s = sh.lock().unwrap();
                    s.p2p_missing = missing;
                    s.p2p_launch = Some((inst.id.clone(), format!("127.0.0.1:{port}")));
                    s.refresh = true;
                    anyhow::Ok(())
                });
            }
            Action::P2pForget(key) => {
                if let Err(e) = crate::hosting::p2p::forget(&self.p, &key) {
                    self.sh.lock().unwrap().error = Some(format!("{e:#}"));
                }
                mdl.joined = crate::hosting::p2p::joined(&self.p);
                vw.hosting.friend_sel = None;
            }
            Action::PlayitCancel => {
                if let Some(c) = self.sh.lock().unwrap().playit_cancel.take() {
                    c.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.sh.lock().unwrap().playit_claim = None;
            }
            Action::PlayitUnlink => {
                crate::hosting::playit::unlink(&self.p);
                mdl.playit_linked = false;
                self.sh.lock().unwrap().info = Some(t!("playit.gg desenlazado de este equipo").into());
            }
            Action::DismissReport => {
                let mut s = self.sh.lock().unwrap();
                if s.report != Some(crate::gui::model::ReportState::Uploading) {
                    s.report = None;
                }
            }
            Action::ClearLogs => {
                let mut s = self.sh.lock().unwrap();
                s.logs_dropped += s.logs.len();
                s.logs.clear();
            }
            Action::CopyError(e) => {
                self.ctx.output_mut(|o| o.copied_text = e);
                let mut s = self.sh.lock().unwrap();
                s.info = Some(t!("Error copiado al portapapeles").into());
                s.error = None;
            }
            Action::DetectJava => {
                self.spawn(t!("Buscando Java instalados"), |_c, p, _rep, sh| async move {
                    let list = tokio::task::spawn_blocking(move || {
                        let mut v = crate::game::java::detect_system();
                        v.extend(crate::game::java::detect_managed(&p));
                        v
                    })
                    .await?;
                    sh.lock().unwrap().javas = Some(list);
                    anyhow::Ok(())
                });
            }
            Action::PickJava(global) => {
                let name = if cfg!(windows) { "javaw.exe" } else { "java" };
                if let Some(f) = rfd::FileDialog::new().set_title(t!("Elegir {name}", name = name)).pick_file() {
                    let path = f.to_string_lossy().into_owned();
                    if global {
                        vw.settings.java_path = path.clone();
                        mdl.settings.java_path = path;
                    } else {
                        vw.edit_java = path;
                    }
                }
            }
            Action::SaveSettings => {
                vw.settings.extra_jvm_args = vw.settings_jvm.split_whitespace().map(String::from).collect();
                mdl.settings = vw.settings.clone();
                self.presence.configure(crate::discord::Config::from_settings(&mdl.settings));
                let r = mdl.settings.save(&self.p);
                let mut s = self.sh.lock().unwrap();
                match r {
                    Ok(()) => s.info = Some(t!("Ajustes guardados").into()),
                    Err(e) => s.error = Some(format!("{e:#}")),
                }
            }
        }
    }

    /// Lanza una instancia (opcionalmente entrando a un servidor) y la muestra en Discord mientras dura.
    fn launch(&self, mdl: &Model, id: String, server: Option<String>) {
        {
            let mut s = self.sh.lock().unwrap();
            if s.launches.contains_key(&id) {
                return; // ya se está lanzando o jugando
            }
            s.playing.insert(id.clone());
            s.launches.insert(id.clone(), Launch::Preparing { stage: t!("Preparando…").into(), progress: None });
        }
        let discord = mdl.instances.iter().find(|i| i.id == id).map(crate::discord::Playing::of);
        let presence = self.presence.clone();
        let prev_stage = self.sh.lock().unwrap().stage.clone();
        self.spawn(t!("Preparando juego"), move |c, p, rep, sh| async move {
            // El progreso de este lanzamiento va a su propia instancia (lo pinta el botón JUGAR).
            let (sh2, id2, presence2, ctx_rep) = (sh.clone(), id.clone(), presence.clone(), rep.clone());
            let rep = Reporter::new(move |e| {
                {
                    let mut s = sh2.lock().unwrap();
                    match &e {
                        Event::Stage(t) => {
                            if let Some(Launch::Preparing { stage, progress }) = s.launches.get_mut(&id2) {
                                *stage = t.clone();
                                *progress = None;
                            }
                        }
                        Event::Progress { done, total } => {
                            if let Some(Launch::Preparing { progress, .. }) = s.launches.get_mut(&id2) {
                                *progress = Some((*done, *total));
                            }
                        }
                        Event::Started => {
                            s.launches.insert(id2.clone(), Launch::Running { since: std::time::Instant::now() });
                            if let Some(d) = &discord {
                                presence2.start_playing(&id2, crate::discord::Playing { since: crate::config::now_secs(), ..d.clone() });
                            }
                        }
                        Event::Log(_) => {}
                    }
                }
                // al registro, sí; a la actividad de fondo, no (las fases las pinta el botón JUGAR)
                match e {
                    Event::Stage(t) => ctx_rep.log(format!("==> {t}")),
                    Event::Log(_) => ctx_rep.send(e),
                    _ => {}
                }
            });
            let r = actions::play_on(&c, &p, &id, false, server, &rep).await;
            presence.stop_playing(&id);
            {
                let mut s = sh.lock().unwrap();
                s.playing.remove(&id);
                s.launches.remove(&id);
            }
            r.map(|_| ())
        });
        self.sh.lock().unwrap().stage = prev_stage;
    }
}

/// La instancia para el servidor de un amigo: la de antes si existe (al día con la versión del
/// servidor), o una nueva con su versión y su loader.
fn p2p_instance(p: &crate::paths::Paths, j: &mut crate::hosting::p2p::Joined, w: &crate::hosting::p2p::Welcome) -> anyhow::Result<Instance> {
    let loader = w.kind.client_loader();
    let lv = if loader == crate::config::Loader::Vanilla { None } else { w.loader_version.clone() };
    if let Some(mut i) = j.instance.as_deref().and_then(|id| Instance::load(p, id).ok()) {
        if i.mc_version != w.mc || i.loader != loader || (loader != crate::config::Loader::Vanilla && i.loader_version != lv) {
            i.mc_version = w.mc.clone();
            i.loader = loader;
            i.loader_version = lv;
            i.resolved_version = None;
            i.save(p)?;
        }
        return Ok(i);
    }
    let i = Instance::create(p, &w.title, &w.mc, loader, lv)?;
    j.instance = Some(i.id.clone());
    j.title = w.title.clone();
    j.mc = w.mc.clone();
    j.kind = w.kind;
    j.loader_version = w.loader_version.clone();
    Ok(i)
}

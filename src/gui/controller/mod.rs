//! Controlador: recibe [`Action`](crate::gui::action::Action)s de la vista, cambia el
//! [`Model`] y lanza trabajo asíncrono.
//!
//! Es el único sitio que modifica el modelo. Posee el runtime de tokio y el cliente HTTP.

mod handle;
mod tasks;

use crate::gui::model::{Model, Session};
use crate::gui::state::ViewState;
use crate::net;
use crate::paths::Paths;
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct Controller {
    pub(super) p: Paths,
    pub(super) rt: tokio::runtime::Runtime,
    pub(super) client: reqwest::Client,
    pub(super) sh: Arc<Mutex<Session>>,
    pub(super) ctx: egui::Context,
    pub(super) presence: crate::discord::Presence,
}

impl Controller {
    pub fn new(p: Paths, sh: Arc<Mutex<Session>>, ctx: egui::Context) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime tokio");
        let (sh2, ctx2) = (sh.clone(), ctx.clone());
        let presence = crate::discord::Presence::start(crate::discord::Config::default(), move |st| {
            sh2.lock().unwrap().discord = st.clone();
            ctx2.request_repaint();
        });
        let ctx3 = ctx.clone();
        crate::hosting::p2p::set_notify(Arc::new(move || ctx3.request_repaint()));
        Controller { p, rt, client: net::client(), sh, ctx, presence }
    }

    /// Selecciona una instancia: carga sus listas y prepara los borradores de edición.
    pub fn select(&self, mdl: &mut Model, vw: &mut ViewState, id: &str) {
        mdl.selected = Some(id.to_string());
        vw.show_create = false;
        vw.confirm_delete = false;
        if let Some(i) = mdl.instances.iter().find(|i| i.id == id).cloned() {
            mdl.load_selected_lists();
            vw.load_instance_drafts(&i);
        }
        self.sh.lock().unwrap().updates.clear();
        self.load_mod_meta(mdl);
    }

    /// Lee en segundo plano los datos de los mods de la instancia (con caché: la segunda vez es
    /// instantáneo). No cuenta como tarea visible: es sólo para enseñar nombres e iconos.
    pub fn load_mod_meta(&self, mdl: &Model) {
        let jars: Vec<std::path::PathBuf> = mdl.mods.iter().map(|m| m.path.clone()).collect();
        if jars.is_empty() {
            return;
        }
        let (cache, sh, ctx) = (mdl.p.cache(), self.sh.clone(), self.ctx.clone());
        self.rt.spawn_blocking(move || {
            let found = crate::content::modmeta::load_all(&cache, &jars);
            sh.lock().unwrap().mod_meta.extend(found);
            ctx.request_repaint();
        });
    }

    /// Relee el disco tras terminar una tarea, conservando las ediciones en curso.
    pub fn reload(&self, mdl: &mut Model, vw: &mut ViewState) {
        let selected = mdl.reload_from_disk();
        if selected.is_some() {
            self.load_mod_meta(mdl);
        }
        match selected {
            Some(i) => {
                if let Some(e) = &mut vw.edit {
                    e.playtime_secs = i.playtime_secs;
                    e.last_played = i.last_played;
                    e.resolved_version = i.resolved_version.clone();
                }
            }
            None => {
                if mdl.selected.is_none() {
                    vw.edit = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Instance, Loader, Settings};
    use crate::gui::action::Action;
    use crate::instance::worlds;

    /// Modelo + estado de vista + controlador sobre una carpeta temporal, sin ventana.
    fn setup() -> (Model, ViewState, Controller, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let p = Paths { root: root.clone() };
        let model = Model::load(p.clone());
        let view = ViewState::new(&model.settings);
        let ctl = Controller::new(p, model.session.clone(), egui::Context::default());
        (model, view, ctl, root)
    }

    fn add_world(p: &Paths, inst: &Instance, name: &str) {
        let w = inst.game_dir(p).join("saves").join(name);
        std::fs::create_dir_all(&w).unwrap();
        std::fs::write(w.join("level.dat"), "x").unwrap();
    }

    #[test]
    fn select_loads_model_and_drafts() {
        let (mut m, mut v, c, root) = setup();
        let mut inst = Instance::create(&m.p, "Prueba", "1.21.1", Loader::Vanilla, None).unwrap();
        inst.wrapper = Some("gamemoderun".into());
        inst.save(&m.p).unwrap();
        add_world(&m.p, &inst, "Mundo");
        m.instances = Instance::list(&m.p);
        v.show_create = true;

        c.select(&mut m, &mut v, &inst.id);

        assert_eq!(m.selected.as_deref(), Some(inst.id.as_str()));
        assert_eq!(m.worlds.len(), 1, "el modelo carga los mundos de la instancia");
        assert!(!v.show_create, "seleccionar cierra el formulario de creación");
        assert_eq!(v.edit_wrapper, "gamemoderun", "el borrador de la vista se rellena desde el modelo");
        assert!(v.edit.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_changes_memory_but_not_disk() {
        let (mut m, mut v, c, root) = setup();
        let mut s = m.settings.clone();
        s.backups_to_keep = 11;
        c.handle(Action::ApplySettings(s), &mut m, &mut v);
        assert_eq!(m.settings.backups_to_keep, 11);
        assert!(!m.p.settings_file().exists(), "aplicar no guarda en disco");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn save_settings_persists_the_draft() {
        let (mut m, mut v, c, root) = setup();
        v.settings.backups_to_keep = 7;
        v.settings_jvm = "-Xss2m  -Dfoo=bar".into();
        c.handle(Action::SaveSettings, &mut m, &mut v);
        let on_disk = Settings::load(&m.p);
        assert_eq!(on_disk.backups_to_keep, 7);
        assert_eq!(on_disk.extra_jvm_args, vec!["-Xss2m", "-Dfoo=bar"]);
        assert_eq!(m.settings, on_disk, "el modelo queda igual que el disco");
        assert_eq!(c.sh.lock().unwrap().info.as_deref(), Some("Ajustes guardados"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_backup_updates_the_model() {
        let (mut m, mut v, c, root) = setup();
        let inst = Instance::create(&m.p, "B", "1.21.1", Loader::Vanilla, None).unwrap();
        add_world(&m.p, &inst, "W");
        let b = worlds::backup_world(&m.p, &inst, "W").unwrap();
        m.instances = Instance::list(&m.p);
        c.select(&mut m, &mut v, &inst.id);
        assert_eq!(m.backups.len(), 1);

        c.handle(Action::DeleteBackup(inst.id.clone(), b.file), &mut m, &mut v);
        assert!(m.backups.is_empty(), "tras borrar, el modelo ya no lista la copia");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dismissing_messages_clears_session() {
        let (mut m, mut v, c, root) = setup();
        {
            let mut s = c.sh.lock().unwrap();
            s.error = Some("fallo".into());
            s.info = Some("aviso".into());
        }
        c.handle(Action::DismissError, &mut m, &mut v);
        assert!(c.sh.lock().unwrap().error.is_none());
        c.handle(Action::DismissInfo, &mut m, &mut v);
        assert!(c.sh.lock().unwrap().info.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saving_an_instance_stores_its_group() {
        let (mut m, mut v, c, root) = setup();
        let inst = Instance::create(&m.p, "G", "1.21.1", Loader::Vanilla, None).unwrap();
        m.instances = Instance::list(&m.p);
        c.select(&mut m, &mut v, &inst.id);
        v.edit_group = "  Con   amigos ".into();
        c.handle(Action::SaveInstance, &mut m, &mut v);
        assert_eq!(Instance::load(&m.p, &inst.id).unwrap().group.as_deref(), Some("Con amigos"));
        v.edit_group = "   ".into();
        c.handle(Action::SaveInstance, &mut m, &mut v);
        assert_eq!(Instance::load(&m.p, &inst.id).unwrap().group, None, "vacío = sin grupo");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sort_and_collapsed_groups_are_remembered() {
        let (mut m, mut v, c, root) = setup();
        c.handle(Action::SetSort(crate::instance::organize::Sort::Playtime), &mut m, &mut v);
        c.handle(Action::ToggleGroup("Técnicos".into()), &mut m, &mut v);
        let saved = crate::instance::organize::UiPrefs::load(&m.p);
        assert_eq!(saved.sort, crate::instance::organize::Sort::Playtime);
        assert!(saved.is_collapsed("Técnicos"));
        c.handle(Action::ToggleGroup("Técnicos".into()), &mut m, &mut v);
        assert!(!crate::instance::organize::UiPrefs::load(&m.p).is_collapsed("Técnicos"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn removing_the_icon_returns_to_the_avatar() {
        let (mut m, mut v, c, root) = setup();
        let mut inst = Instance::create(&m.p, "I", "1.21.1", Loader::Vanilla, None).unwrap();
        let src = root.join("img.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([1, 2, 3, 255])).save(&src).unwrap();
        inst.icon = Some(crate::instance::organize::set_icon(&inst.dir(&m.p), &src).unwrap());
        inst.save(&m.p).unwrap();
        m.instances = Instance::list(&m.p);
        c.select(&mut m, &mut v, &inst.id);
        assert!(m.icon_uri(&m.instances[0]).is_some());
        c.handle(Action::RemoveIcon(inst.id.clone()), &mut m, &mut v);
        let after = Instance::load(&m.p, &inst.id).unwrap();
        assert!(after.icon.is_none() && m.icon_uri(&after).is_none());
        assert!(v.edit.as_ref().unwrap().icon.is_none(), "el borrador tampoco conserva el icono viejo");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reload_keeps_edits_and_drops_deleted_instance() {
        let (mut m, mut v, c, root) = setup();
        let inst = Instance::create(&m.p, "R", "1.21.1", Loader::Vanilla, None).unwrap();
        m.instances = Instance::list(&m.p);
        c.select(&mut m, &mut v, &inst.id);
        v.edit.as_mut().unwrap().name = "Editando…".into();

        c.reload(&mut m, &mut v);
        assert_eq!(v.edit.as_ref().unwrap().name, "Editando…", "recargar no pisa lo que el usuario está escribiendo");

        std::fs::remove_dir_all(inst.dir(&m.p)).unwrap();
        c.reload(&mut m, &mut v);
        assert!(m.selected.is_none() && v.edit.is_none(), "si la instancia desaparece, se limpia la selección");
        let _ = std::fs::remove_dir_all(root);
    }
}

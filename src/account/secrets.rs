//! Guardado de tokens en el almacén seguro del sistema (Administrador de credenciales de Windows,
//! Llavero de macOS). En Linux (o si el almacén falla) se queda en accounts.json con permisos 600.

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    const SERVICE: &str = "brasa-launcher";
    pub fn set(key: &str, value: &str) -> bool {
        keyring::Entry::new(SERVICE, key).and_then(|e| e.set_password(value)).is_ok()
    }
    pub fn get(key: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, key).and_then(|e| e.get_password()).ok()
    }
    pub fn delete(key: &str) {
        if let Ok(e) = keyring::Entry::new(SERVICE, key) {
            let _ = e.delete_credential();
        }
    }
    pub const AVAILABLE: bool = true;
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub fn set(_key: &str, _value: &str) -> bool {
        false
    }
    pub fn get(_key: &str) -> Option<String> {
        None
    }
    pub fn delete(_key: &str) {}
    pub const AVAILABLE: bool = false;
}

pub use imp::{delete, get, set, AVAILABLE};

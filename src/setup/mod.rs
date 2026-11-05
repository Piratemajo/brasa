//! Instalador y desinstalador propios: el mismo `brasa.exe` hace de instalador cuando su nombre
//! contiene «setup» (`Brasa-Setup-windows-x64.exe`), o con `brasa install`.
//!
//! Instalar = copiarse a la carpeta de programas del usuario (sin pedir administrador), crear los
//! accesos directos y apuntarse en «Agregar o quitar programas». Desinstalar = lo contrario, con la
//! opción de borrar también los datos (instancias, mundos, cuentas).
//!
//! | | Windows | Linux |
//! |---|---|---|
//! | Programa | `%LOCALAPPDATA%\Programs\Brasa\brasa.exe` | `~/.local/share/brasa-launcher/brasa` |
//! | Accesos | menú Inicio y (opcional) escritorio | `~/.local/share/applications/brasa-launcher.desktop` |
//! | Registro | `HKCU\…\Uninstall\BrasaLauncher` (con `reg.exe`) | — |
//!
//! `BRASA_SETUP_ROOT=<carpeta>` lo hace todo dentro de esa carpeta (para probar sin tocar el sistema).

pub mod ui;

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_NAME: &str = "Brasa Launcher";
/// Clave del registro de desinstalación (Windows).
pub const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\BrasaLauncher";

/// ¿Se ha abierto como instalador (por el nombre del fichero)?
pub fn launched_as_setup() -> bool {
    std::env::current_exe().ok().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase())).is_some_and(|n| n.contains("setup"))
}

/// Dónde va cada cosa.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub dir: PathBuf,
    pub exe: PathBuf,
    /// Linux: el `.desktop` del menú de aplicaciones.
    pub menu_entry: Option<PathBuf>,
    /// Linux: enlace en `~/.local/bin` para usar `brasa` desde la terminal.
    pub bin_link: Option<PathBuf>,
    /// Con `BRASA_SETUP_ROOT`: no se toca el registro ni las carpetas del sistema.
    pub sandbox: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Ficheros que pone Brasa en su carpeta: al desinstalar sólo se borran éstos (y la carpeta si
/// queda vacía), nunca nada más que el usuario tenga ahí.
pub fn own_files() -> Vec<String> {
    let exe = exe_name();
    let stem = exe.trim_end_matches(".exe");
    vec![exe.to_string(), format!("{stem}.old"), format!("{stem}.new"), "brasa.png".into(), "LICENSE.txt".into(), "THIRD_PARTY_NOTICES.md".into()]
}

/// Carpeta final a partir de la que elige el usuario: si ya tiene otras cosas (p. ej. `D:\Juegos`),
/// Brasa va en una subcarpeta `Brasa` para no mezclarse ni arriesgar nada al desinstalar.
pub fn install_dir_for(chosen: &Path) -> PathBuf {
    let named_brasa = chosen.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("brasa"));
    let empty = std::fs::read_dir(chosen).map(|mut d| d.next().is_none()).unwrap_or(true);
    let ours = chosen.join(exe_name()).exists();
    if named_brasa || empty || ours {
        chosen.to_path_buf()
    } else {
        chosen.join("Brasa")
    }
}

/// ¿Se puede escribir ahí sin permisos de administrador? Se prueba en la carpeta (o en la primera
/// que exista por encima) creando y borrando un fichero.
pub fn check_writable(dir: &Path) -> Result<()> {
    let mut probe = dir.to_path_buf();
    while !probe.exists() {
        match probe.parent() {
            Some(p) => probe = p.to_path_buf(),
            None => bail!("la ruta no existe"),
        }
    }
    if !probe.is_dir() {
        bail!("{} no es una carpeta", probe.display());
    }
    let f = probe.join(format!(".brasa-write-test-{}", std::process::id()));
    if let Err(e) = std::fs::write(&f, b"ok") {
        // sin el «os error» del sistema: una frase que se entienda
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            bail!("{}", t!("esa carpeta necesita permisos de administrador"));
        }
        bail!("{}", t!("no se puede escribir en {}", probe.display()));
    }
    let _ = std::fs::remove_file(&f);
    Ok(())
}

impl Layout {
    /// La misma disposición con otra carpeta para el programa (los accesos no cambian de sitio).
    pub fn with_dir(&self, dir: PathBuf) -> Layout {
        Layout { exe: dir.join(exe_name()), dir, ..self.clone() }
    }

    /// Donde está instalado Brasa, si lo está: lo dice la entrada del menú (Linux) o el registro de
    /// desinstalación (Windows).
    pub fn previous_install(&self) -> Option<PathBuf> {
        if let Some(entry) = &self.menu_entry {
            let text = std::fs::read_to_string(entry).ok()?;
            let exec = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
            let exe = PathBuf::from(exec.trim().trim_matches('"'));
            return exe.exists().then(|| exe.parent().map(Path::to_path_buf)).flatten();
        }
        if cfg!(windows) && !self.sandbox {
            let out = hidden(Command::new("reg").args(["query", UNINSTALL_KEY, "/v", "InstallLocation"])).output().ok()?;
            let dir = parse_reg_value(&String::from_utf8_lossy(&out.stdout), "InstallLocation")?;
            let dir = PathBuf::from(dir);
            return dir.join(exe_name()).exists().then_some(dir);
        }
        None
    }

    /// Para desinstalar desde la copia instalada: la carpeta del propio ejecutable.
    pub fn installed_here(&self) -> Layout {
        match std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            Some(d) if d.join(exe_name()).exists() => self.with_dir(d),
            _ => self.previous_install().map(|d| self.with_dir(d)).unwrap_or_else(|| self.clone()),
        }
    }

    pub fn default_for_this_system() -> Layout {
        if let Some(root) = std::env::var_os("BRASA_SETUP_ROOT").map(PathBuf::from) {
            let dir = root.join("Programs").join("Brasa");
            return Layout {
                exe: dir.join(exe_name()),
                dir,
                menu_entry: Some(root.join("applications").join("brasa-launcher.desktop")),
                bin_link: None,
                sandbox: true,
            };
        }
        if cfg!(windows) {
            let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Local"));
            let dir = base.join("Programs").join("Brasa");
            Layout { exe: dir.join(exe_name()), dir, menu_entry: None, bin_link: None, sandbox: false }
        } else {
            let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local").join("share"));
            let dir = data.join("brasa-launcher");
            Layout {
                exe: dir.join(exe_name()),
                dir,
                menu_entry: Some(data.join("applications").join("brasa-launcher.desktop")),
                bin_link: Some(home().join(".local").join("bin").join("brasa")),
                sandbox: false,
            }
        }
    }

    pub fn is_installed(&self) -> bool {
        self.exe.exists()
    }
}

pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "brasa.exe"
    } else {
        "brasa"
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub desktop_shortcut: bool,
    pub launch_after: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { desktop_shortcut: true, launch_after: true }
    }
}

/// Copia el ejecutable a su sitio. Si ya hay uno (actualizar o reinstalar) se aparta primero: en
/// Windows un .exe en uso no se puede sobrescribir, pero sí renombrar.
pub fn copy_exe(src: &Path, dst: &Path) -> Result<()> {
    if let (Ok(a), Ok(b)) = (src.canonicalize(), dst.canonicalize()) {
        if a == b {
            return Ok(()); // ya se está ejecutando desde la carpeta instalada
        }
    }
    let dir = dst.parent().context("ruta de destino")?;
    std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    let tmp = dst.with_extension("new");
    std::fs::copy(src, &tmp).with_context(|| format!("no se pudo copiar a {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    if dst.exists() {
        let old = dst.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(dst, &old).context("no se pudo apartar la versión instalada (¿está Brasa abierto?)")?;
    }
    std::fs::rename(&tmp, dst).context("no se pudo colocar el programa")?;
    Ok(())
}

/// Las entradas del registro de desinstalación (cada una, los argumentos de `reg add`).
pub fn uninstall_entries(l: &Layout, version: &str, size_kb: u64) -> Vec<Vec<String>> {
    let exe = l.exe.display().to_string();
    let s = |name: &str, value: String| {
        vec!["add".into(), UNINSTALL_KEY.into(), "/v".into(), name.into(), "/t".into(), "REG_SZ".into(), "/d".into(), value, "/f".into()]
    };
    let d = |name: &str, value: u64| {
        vec!["add".into(), UNINSTALL_KEY.into(), "/v".into(), name.into(), "/t".into(), "REG_DWORD".into(), "/d".into(), value.to_string(), "/f".into()]
    };
    vec![
        s("DisplayName", APP_NAME.into()),
        s("DisplayVersion", version.into()),
        s("Publisher", "Brasa".into()),
        s("DisplayIcon", format!("{exe},0")),
        s("InstallLocation", l.dir.display().to_string()),
        s("UninstallString", format!("\"{exe}\" uninstall")),
        s("QuietUninstallString", format!("\"{exe}\" uninstall --quiet")),
        d("NoModify", 1),
        d("NoRepair", 1),
        d("EstimatedSize", size_kb),
    ]
}

/// Valor de `reg query`: `    InstallLocation    REG_SZ    C:\…`.
pub fn parse_reg_value(out: &str, name: &str) -> Option<String> {
    out.lines().find(|l| l.trim_start().starts_with(name)).and_then(|l| l.split_once("REG_SZ")).map(|(_, v)| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Script de PowerShell que crea (o borra) los accesos directos del menú Inicio y del escritorio.
/// Las carpetas se piden a Windows (el escritorio puede estar en OneDrive, por ejemplo).
pub fn shortcut_script(exe: &Path, desktop: bool, remove: bool) -> String {
    let q = |s: &str| s.replace('\'', "''");
    let exe_s = q(&exe.display().to_string());
    let dir_s = q(&exe.parent().map(|p| p.display().to_string()).unwrap_or_default());
    let mut folders = vec!["[Environment]::GetFolderPath('Programs')"];
    if desktop || remove {
        folders.push("[Environment]::GetFolderPath('Desktop')");
    }
    let body = if remove {
        "Remove-Item -LiteralPath $p -ErrorAction SilentlyContinue".to_string()
    } else {
        format!("$s = $ws.CreateShortcut($p); $s.TargetPath = '{exe_s}'; $s.WorkingDirectory = '{dir_s}'; $s.IconLocation = '{exe_s},0'; $s.Description = '{APP_NAME}'; $s.Save()")
    };
    format!("$ws = New-Object -ComObject WScript.Shell; foreach ($d in @({})) {{ $p = Join-Path $d '{APP_NAME}.lnk'; {body} }}", folders.join(", "))
}

/// Entrada del menú de aplicaciones de Linux.
pub fn desktop_entry(exe: &Path, icon: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={APP_NAME}\nComment=Launcher de Minecraft\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Game;\nStartupWMClass=brasa\n",
        exe.display(),
        icon.display()
    )
}

#[cfg(windows)]
fn hidden(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}
#[cfg(not(windows))]
fn hidden(cmd: &mut Command) -> &mut Command {
    cmd
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let st = hidden(cmd).status().with_context(|| format!("no se pudo ejecutar {what}"))?;
    if !st.success() {
        bail!("{what} terminó con error ({st})");
    }
    Ok(())
}

fn powershell(script: &str) -> Result<()> {
    run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]), "PowerShell")
}

/// Pasos de la instalación, para enseñarlos en el asistente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Copy,
    Shortcuts,
    Register,
}

pub fn install(src: &Path, l: &Layout, o: &Options, mut progress: impl FnMut(Step)) -> Result<()> {
    progress(Step::Copy);
    copy_exe(src, &l.exe)?;
    // el icono, para el menú de Linux (en Windows va dentro del .exe)
    let icon = l.dir.join("brasa.png");
    std::fs::write(&icon, include_bytes!("../../assets/icon/brasa-256.png"))?;
    for (name, text) in [("LICENSE.txt", include_str!("../../LICENSE")), ("THIRD_PARTY_NOTICES.md", include_str!("../../THIRD_PARTY_NOTICES.md"))] {
        std::fs::write(l.dir.join(name), text)?;
    }

    progress(Step::Shortcuts);
    if let Some(entry) = &l.menu_entry {
        if let Some(d) = entry.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(entry, desktop_entry(&l.exe, &icon))?;
        if o.desktop_shortcut && !l.sandbox {
            if let Some(desk) = desktop_dir() {
                let p = desk.join("brasa-launcher.desktop");
                std::fs::write(&p, desktop_entry(&l.exe, &icon))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
                }
            }
        }
    }
    #[cfg(unix)]
    if let Some(link) = &l.bin_link {
        if let Some(d) = link.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::remove_file(link);
        let _ = std::os::unix::fs::symlink(&l.exe, link);
    }
    if cfg!(windows) && !l.sandbox {
        powershell(&shortcut_script(&l.exe, o.desktop_shortcut, false)).context("no se pudieron crear los accesos directos")?;
    }

    progress(Step::Register);
    if cfg!(windows) && !l.sandbox {
        let size_kb = std::fs::metadata(&l.exe).map(|m| m.len() / 1024).unwrap_or(0);
        for args in uninstall_entries(l, env!("CARGO_PKG_VERSION"), size_kb) {
            run(Command::new("reg").args(&args), "reg.exe").context("no se pudo registrar en «Agregar o quitar programas»")?;
        }
    }
    Ok(())
}

fn desktop_dir() -> Option<PathBuf> {
    let d = directories::UserDirs::new()?.desktop_dir()?.to_path_buf();
    d.exists().then_some(d)
}

/// Quita accesos, registro y (si se pide) los datos. La carpeta del programa se borra al salir:
/// en Windows el .exe en marcha no se puede borrar, así que se encarga un `cmd` que espera un poco.
pub fn uninstall(l: &Layout, data: Option<&Path>) -> Result<()> {
    if let Some(entry) = &l.menu_entry {
        let _ = std::fs::remove_file(entry);
    }
    if !l.sandbox {
        if let Some(desk) = desktop_dir() {
            let _ = std::fs::remove_file(desk.join("brasa-launcher.desktop"));
        }
    }
    if let Some(link) = &l.bin_link {
        let _ = std::fs::remove_file(link);
    }
    if cfg!(windows) && !l.sandbox {
        let _ = powershell(&shortcut_script(&l.exe, true, true));
        let _ = hidden(Command::new("reg").args(["delete", UNINSTALL_KEY, "/f"])).status();
    }
    if let Some(d) = data {
        std::fs::remove_dir_all(d).with_context(|| format!("no se pudieron borrar los datos en {}", d.display()))?;
    }
    // sólo lo nuestro; la carpeta, si queda vacía
    let files: Vec<PathBuf> = own_files().iter().map(|f| l.dir.join(f)).collect();
    if cfg!(windows) && !l.sandbox {
        // el .exe en marcha no se puede borrar: lo hace un cmd cuando Brasa ya se ha cerrado
        let dels: Vec<String> = files.iter().map(|f| format!("\"{}\"", f.display())).collect();
        let script = format!("timeout /T 2 /NOBREAK >NUL & del /F /Q {} & rmdir \"{}\"", dels.join(" "), l.dir.display());
        hidden(Command::new("cmd").args(["/C", &script])).spawn().context("no se pudo programar el borrado de la carpeta")?;
    } else {
        // en Linux (y en pruebas) se puede borrar aunque el programa esté en marcha
        for f in &files {
            let _ = std::fs::remove_file(f);
        }
        let _ = std::fs::remove_dir(&l.dir);
    }
    Ok(())
}

/// Abre el Brasa instalado (desacoplado del instalador) para que éste pueda cerrarse.
pub fn launch(l: &Layout) -> Result<()> {
    hidden(Command::new(&l.exe).current_dir(&l.dir)).spawn().context("no se pudo abrir Brasa")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> (PathBuf, Layout) {
        let root = std::env::temp_dir().join(format!("brasa-setup-{}", uuid::Uuid::new_v4()));
        let dir = root.join("Programs").join("Brasa");
        let l = Layout {
            exe: dir.join(exe_name()),
            dir,
            menu_entry: Some(root.join("applications").join("brasa-launcher.desktop")),
            bin_link: None,
            sandbox: true,
        };
        (root, l)
    }

    #[test]
    fn installs_upgrades_and_uninstalls_in_a_sandbox() {
        let (root, l) = sandbox();
        let src = root.join("Brasa-Setup.exe");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&src, b"v1").unwrap();
        let mut steps = vec![];
        install(&src, &l, &Options::default(), |s| steps.push(s)).unwrap();
        assert_eq!(steps, vec![Step::Copy, Step::Shortcuts, Step::Register]);
        assert!(l.is_installed());
        assert_eq!(std::fs::read(&l.exe).unwrap(), b"v1");
        assert!(l.dir.join("brasa.png").exists() && l.dir.join("LICENSE.txt").exists());
        let entry = std::fs::read_to_string(l.menu_entry.as_ref().unwrap()).unwrap();
        assert!(entry.contains("Name=Brasa Launcher") && entry.contains(&l.exe.display().to_string()));

        // otra vez con una versión nueva: sustituye la instalada
        std::fs::write(&src, b"v2").unwrap();
        install(&src, &l, &Options::default(), |_| {}).unwrap();
        assert_eq!(std::fs::read(&l.exe).unwrap(), b"v2");

        // desinstalar con los datos
        let data = root.join("datos");
        std::fs::create_dir_all(data.join("instances")).unwrap();
        uninstall(&l, Some(&data)).unwrap();
        assert!(!l.dir.exists() && !data.exists() && !l.menu_entry.as_ref().unwrap().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn chosen_folders_and_safe_uninstall() {
        let (root, l) = sandbox();
        let games = root.join("Juegos");
        std::fs::create_dir_all(&games).unwrap();
        assert_eq!(install_dir_for(&games), games, "carpeta vacía: se usa tal cual");
        std::fs::write(games.join("otro-juego.exe"), b"x").unwrap();
        assert_eq!(install_dir_for(&games), games.join("Brasa"), "con cosas dentro: subcarpeta Brasa");
        assert_eq!(install_dir_for(&root.join("no-existe")), root.join("no-existe"));
        assert_eq!(install_dir_for(&root.join("BRASA")), root.join("BRASA"));
        check_writable(&root.join("no-existe").join("tampoco")).unwrap();

        // instalar ahí, recordar dónde y desinstalar sin tocar lo ajeno
        let custom = l.with_dir(games.clone());
        let src = root.join("Brasa-Setup.exe");
        std::fs::write(&src, b"v1").unwrap();
        install(&src, &custom, &Options::default(), |_| {}).unwrap();
        assert_eq!(custom.previous_install().unwrap(), games, "se encuentra la instalación por la entrada del menú");
        uninstall(&custom, None).unwrap();
        assert!(games.join("otro-juego.exe").exists(), "lo que no es de Brasa se queda");
        assert!(!games.join(exe_name()).exists() && !games.join("brasa.png").exists());
        assert!(custom.previous_install().is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reads_the_install_location_from_reg_query() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\…\\BrasaLauncher\r\n    InstallLocation    REG_SZ    D:\\Juegos\\Brasa\r\n\r\n";
        assert_eq!(parse_reg_value(out, "InstallLocation").as_deref(), Some("D:\\Juegos\\Brasa"));
        assert_eq!(parse_reg_value("ERROR: no se encontró", "InstallLocation"), None);
    }

    #[test]
    fn running_from_the_installed_copy_is_fine() {
        let (root, l) = sandbox();
        std::fs::create_dir_all(&l.dir).unwrap();
        std::fs::write(&l.exe, b"ya").unwrap();
        copy_exe(&l.exe, &l.exe).unwrap();
        assert_eq!(std::fs::read(&l.exe).unwrap(), b"ya");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_entries_and_shortcut_script() {
        let l = Layout {
            dir: PathBuf::from(r"C:\Users\Ana O'Neil\AppData\Local\Programs\Brasa"),
            exe: PathBuf::from(r"C:\Users\Ana O'Neil\AppData\Local\Programs\Brasa\brasa.exe"),
            menu_entry: None,
            bin_link: None,
            sandbox: false,
        };
        let e = uninstall_entries(&l, "1.0.0-beta.1", 20480);
        let find = |name: &str| e.iter().find(|a| a[3] == name).unwrap().clone();
        assert_eq!(find("UninstallString")[7], r#""C:\Users\Ana O'Neil\AppData\Local\Programs\Brasa\brasa.exe" uninstall"#);
        assert_eq!(find("EstimatedSize")[5], "REG_DWORD");
        assert!(e.iter().all(|a| a[0] == "add" && a[1] == UNINSTALL_KEY && a.last().unwrap() == "/f"));
        let s = shortcut_script(&l.exe, true, false);
        assert!(s.contains("Ana O''Neil"), "comilla simple escapada para PowerShell: {s}");
        assert!(s.contains("GetFolderPath('Desktop')") && s.contains("CreateShortcut"));
        assert!(!shortcut_script(&l.exe, false, false).contains("Desktop"));
        assert!(shortcut_script(&l.exe, false, true).contains("Remove-Item"));
    }
}

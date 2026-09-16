//! Loom Setup: a bespoke installer with the same glass language as the app.
//!
//! The payload zip is embedded in the binary (`include_bytes!`), so Setup is a
//! single portable exe: no installer framework, no sidecar files. Installing
//! extracts the payload, writes shortcuts, registers an uninstaller under
//! HKCU, and writes `uninstall.cmd` next to the app. Silent mode supports
//! scripted installs:
//!
//! ```text
//! Loom Setup.exe --silent --dir "C:\path\to\install"
//! ```

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// The Add/Remove Programs entry: written on install, read back so an update
/// finds the app wherever it was put.
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Loom";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInfo {
    payload: Option<String>,
    payload_bytes: u64,
    default_dir: String,
    current_version: String,
    installed_version: Option<String>,
    autostart_enabled: bool,
}

/// Emitted while the install runs. `phase` names what is happening; during
/// extraction `file`/`done`/`total` describe the bytes being written, and the
/// tail phases carry no counts of their own so the UI can look busy instead.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupProgress {
    phase: &'static str,
    file: Option<String>,
    done: u64,
    total: u64,
}

impl SetupProgress {
    fn phase(phase: &'static str) -> Self {
        Self {
            phase,
            file: None,
            done: 0,
            total: 0,
        }
    }
}

fn emit(app: &AppHandle, progress: SetupProgress) {
    let _ = app.emit("setup://progress", progress);
}

fn default_install_dir() -> PathBuf {
    // An existing install wins: updates land where the app already lives,
    // even when that is outside LOCALAPPDATA.
    if let Some(existing) = installed_location() {
        return existing;
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Programs").join("Loom")
}

/// Where the last install went, per its uninstall entry. `None` when there is
/// no entry, no recorded location, or the app is gone from that folder.
fn installed_location() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let recorded: Option<String> = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(UNINSTALL_KEY)
            .ok()
            .and_then(|key| key.get_value("InstallLocation").ok());
        let location = PathBuf::from(recorded?);
        return location.join("loom.exe").exists().then_some(location);
    }
    #[cfg(not(windows))]
    None
}

/// Finds the payload zip: bundled resource first, next to the exe second.
/// The app payload, compiled into the Setup binary.
static PAYLOAD: &[u8] = include_bytes!("../payload.zip");

/// Reads a payload override next to the exe during development, falling back
/// to the embedded copy. Release builds always use the embedded payload, so a
/// stray `payload.zip` in `target\release` can never shadow what this
/// installer actually ships.
fn payload_bytes() -> Vec<u8> {
    if cfg!(debug_assertions) {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(directory) = exe.parent() {
                let sidecar = directory.join("payload.zip");
                if let Ok(bytes) = std::fs::read(&sidecar) {
                    if !bytes.is_empty() {
                        return bytes;
                    }
                }
            }
        }
    }
    PAYLOAD.to_vec()
}

/// The version recorded by the last install, or `None` when nothing is
/// installed. The uninstall entry is the source of truth, so an app installed
/// to a custom folder is still recognised.
fn installed_version() -> Option<String> {
    installed_location()?;
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let recorded: Option<String> = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(UNINSTALL_KEY)
            .ok()
            .and_then(|key| key.get_value("DisplayVersion").ok());
        Some(recorded.unwrap_or_else(current_version))
    }
    #[cfg(not(windows))]
    Some(current_version())
}

fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn setup_info(_app: AppHandle) -> SetupInfo {
    let bytes = payload_bytes();

    SetupInfo {
        payload: if bytes.is_empty() {
            None
        } else {
            Some("embedded".to_string())
        },
        payload_bytes: bytes.len() as u64,
        default_dir: default_install_dir().to_string_lossy().into_owned(),
        current_version: current_version(),
        installed_version: installed_version(),
        autostart_enabled: autostart_enabled(),
    }
}

#[tauri::command]
fn sha256_of(path: String) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Extracts the payload and creates shortcuts + uninstall information.
///
/// Async so unpacking never blocks the window: the UI stays live to paint the
/// progress events this emits.
#[tauri::command]
async fn install(
    app: AppHandle,
    dir: String,
    desktop_shortcut: bool,
    start_at_login: bool,
) -> Result<String, String> {
    let install_dir = PathBuf::from(&dir);
    std::fs::create_dir_all(&install_dir).map_err(|error| {
        format!(
            "Cannot use \"{dir}\": {error}\n\nChoose a folder under your user profile, such as {}.",
            default_install_dir().display()
        )
    })?;

    let bytes = payload_bytes();
    if bytes.is_empty() {
        return Err("this build has no payload embedded".into());
    }

    // A running Loom holds loom.exe open, so an in-place update would fail on
    // the one file that matters. Close it first.
    if kill_running_app() {
        emit(&app, SetupProgress::phase("closing"));
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    let progress_app = app.clone();
    let mut on_bytes = move |file: &str, done: u64, total: u64| {
        emit(
            &progress_app,
            SetupProgress {
                phase: "extracting",
                file: Some(file.to_string()),
                done,
                total,
            },
        );
    };
    extract_zip_bytes(&bytes, &install_dir, &mut on_bytes)?;

    emit(&app, SetupProgress::phase("shortcuts"));
    let uninstaller = write_uninstaller(&install_dir)?;
    create_shortcut(&install_dir, desktop_shortcut)?;

    emit(&app, SetupProgress::phase("registering"));
    register_uninstall(&install_dir, &uninstaller, bytes.len() as u64)?;
    // Best-effort: the app can still set this later from Settings, so a
    // registry hiccup must not read as a failed install.
    if let Err(error) = write_autostart(&install_dir, start_at_login) {
        eprintln!("loom-setup: autostart not set: {error}");
    }

    Ok(install_dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn launch_app(dir: String) -> Result<(), String> {
    let exe = PathBuf::from(dir).join("loom.exe");
    if !exe.exists() {
        return Err(format!(
            "{} is missing — the install may have been moved or removed.",
            exe.display()
        ));
    }
    std::process::Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start Loom: {error}"))
}

/// Spawns a console program without flashing a console window.
fn hidden_command(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Stops a running Loom so the payload can replace `loom.exe`. Returns true
/// when a process was actually ended.
fn kill_running_app() -> bool {
    hidden_command("taskkill")
        .args(["/IM", "loom.exe", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Unpacks the embedded zip, reporting the file being written and how many
/// uncompressed bytes are done so far.
fn extract_zip_bytes(
    bytes: &[u8],
    target: &Path,
    progress: &mut dyn FnMut(&str, u64, u64),
) -> Result<(), String> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let total: u64 = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .map(|entry| entry.size())
                .unwrap_or(0)
        })
        .sum::<u64>()
        .max(1);
    let mut done: u64 = 0;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = entry
            .enclosed_name()
            .ok_or("unsafe path in payload")?
            .to_path_buf();
        let destination = target.join(name);

        if entry.is_dir() {
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let display = entry.name().to_string();
        let mut out = std::fs::File::create(&destination)
            .map_err(|error| format!("could not write {}: {error}", destination.display()))?;
        let mut buffer = vec![0u8; 512 * 1024];
        loop {
            let read = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            out.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
            done += read as u64;
            progress(&display, done, total);
        }
    }
    Ok(())
}

/// Writes the uninstaller to `%LOCALAPPDATA%\Loom\uninstall.cmd` and returns
/// its path.
///
/// It deliberately lives outside the install folder: a batch file cannot
/// delete the directory it is running from.
fn write_uninstaller(install_dir: &Path) -> Result<PathBuf, String> {
    let home = uninstall_home()?;
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let script = home.join("uninstall.cmd");

    // The install path is baked in, so the script needs no arguments and no
    // self-copy dance. `ping` is the pause because `timeout` refuses to run
    // when stdin is redirected (which is how Windows launches uninstallers).
    let body = format!(
        "@echo off\r\n\
         echo Removing Loom...\r\n\
         taskkill /IM loom.exe /F >nul 2>&1\r\n\
         ping 127.0.0.1 -n 2 >nul\r\n\
         reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Loom\" /f >nul 2>&1\r\n\
         reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\" /v Loom /f >nul 2>&1\r\n\
         del \"%USERPROFILE%\\Desktop\\Loom.lnk\" >nul 2>&1\r\n\
         del \"%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Loom.lnk\" >nul 2>&1\r\n\
         rmdir /S /Q \"{install}\"\r\n\
         del \"%~f0\" >nul 2>&1\r\n\
         exit /b 0\r\n",
        install = install_dir.display(),
    );

    std::fs::write(&script, body).map_err(|e| e.to_string())?;
    Ok(script)
}

/// `%LOCALAPPDATA%\Loom` — where the uninstaller lives.
fn uninstall_home() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("LOCALAPPDATA is not set")?;
    Ok(base.join("Loom"))
}

/// Creates Start Menu (and optionally desktop) shortcuts via WScript.Shell.
fn create_shortcut(install_dir: &Path, desktop: bool) -> Result<(), String> {
    let exe = install_dir.join("loom.exe");
    let start_menu = format!(
        "{}\\Microsoft\\Windows\\Start Menu\\Programs\\Loom.lnk",
        std::env::var("APPDATA").unwrap_or_default()
    );
    let desktop_path = format!(
        "{}\\Desktop\\Loom.lnk",
        std::env::var("USERPROFILE").unwrap_or_default()
    );

    let mut script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{start_menu}'); \
         $s.TargetPath = '{exe}'; $s.WorkingDirectory = '{dir}'; $s.Save();",
        start_menu = start_menu,
        exe = exe.display(),
        dir = install_dir.display(),
    );
    if desktop {
        script.push_str(&format!(
            " $d = (New-Object -ComObject WScript.Shell).CreateShortcut('{desktop_path}'); \
             $d.TargetPath = '{exe}'; $d.WorkingDirectory = '{dir}'; $d.Save();",
            desktop_path = desktop_path,
            exe = exe.display(),
            dir = install_dir.display(),
        ));
    }

    let status = hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("shortcut creation failed".to_string())
    }
}

/// Adds or removes the per-user Run entry. `Loom` is the name the app's
/// autostart plugin uses, so a fresh install and the Settings toggle refer to
/// the same entry.
fn write_autostart(install_dir: &Path, enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                KEY_WRITE,
            )
            .map_err(|e| e.to_string())?;
        if enabled {
            let exe = install_dir.join("loom.exe");
            return key
                .set_value("Loom", &format!("\"{}\"", exe.display()))
                .map_err(|e| e.to_string());
        }
        match key.delete_value("Loom") {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    #[cfg(not(windows))]
    let _ = (install_dir, enabled);
    Ok(())
}

/// Whether the Run entry exists (so an update can show the true state).
fn autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        return RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .and_then(|key| key.get_value::<String, _>("Loom"))
            .is_ok();
    }
    #[cfg(not(windows))]
    false
}

#[cfg(windows)]
fn register_uninstall(
    install_dir: &Path,
    uninstaller: &Path,
    payload_bytes: u64,
) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey_with_flags(UNINSTALL_KEY, KEY_WRITE)
        .map_err(|e| e.to_string())?;

    key.set_value("DisplayName", &"Loom")
        .map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &current_version())
        .map_err(|e| e.to_string())?;
    key.set_value("Publisher", &"Ellio")
        .map_err(|e| e.to_string())?;
    key.set_value(
        "InstallLocation",
        &install_dir.to_string_lossy().into_owned(),
    )
    .map_err(|e| e.to_string())?;
    key.set_value(
        "DisplayIcon",
        &install_dir.join("loom.exe").to_string_lossy().into_owned(),
    )
    .map_err(|e| e.to_string())?;
    key.set_value(
        "UninstallString",
        &format!("cmd /C \"{}\"", uninstaller.display()),
    )
    .map_err(|e| e.to_string())?;
    // Installed size for Settings → Apps (in KB); the payload is the app.
    key.set_value("EstimatedSize", &((payload_bytes / 1024).max(1) as u32))
        .map_err(|e| e.to_string())?;
    key.set_value("NoModify", &1u32)
        .map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(windows))]
fn register_uninstall(
    _install_dir: &Path,
    _uninstaller: &Path,
    _payload_bytes: u64,
) -> Result<(), String> {
    Ok(())
}

/// Handles `--silent --dir <path>` scripted installs before showing any UI.
fn maybe_run_silent(_app: &AppHandle) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|arg| arg == "--silent") {
        return false;
    }

    let dir = args
        .iter()
        .position(|arg| arg == "--dir")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
        .unwrap_or_else(default_install_dir);

    let result = (|| -> Result<(), String> {
        let bytes = payload_bytes();
        if bytes.is_empty() {
            return Err("this build has no payload embedded".to_string());
        }
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("cannot use \"{}\": {error}", dir.display()))?;
        if kill_running_app() {
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        extract_zip_bytes(&bytes, &dir, &mut |_, _, _| {})?;
        let uninstaller = write_uninstaller(&dir)?;
        create_shortcut(&dir, false)?;
        register_uninstall(&dir, &uninstaller, bytes.len() as u64)?;
        Ok(())
    })();

    match result {
        Ok(()) => println!("loom-setup: installed to {}", dir.display()),
        Err(error) => {
            eprintln!("loom-setup: {error}");
            std::process::exit(1);
        }
    }

    true
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            setup_info, sha256_of, install, launch_app
        ])
        .setup(|app| {
            if maybe_run_silent(app.handle()) {
                app.handle().exit(0);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Loom Setup");
}

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

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInfo {
    payload: Option<String>,
    payload_bytes: u64,
    default_dir: String,
    current_version: String,
    installed_version: Option<String>,
}

fn default_install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Programs").join("Loom")
}

/// Finds the payload zip: bundled resource first, next to the exe second.
/// The app payload, compiled into the Setup binary.
static PAYLOAD: &[u8] = include_bytes!("../payload.zip");

/// Reads a payload override next to the exe (used by `scripts/make-payload`
/// during development so rebuilding Setup is not required), falling back to
/// the embedded copy.
fn payload_bytes() -> Vec<u8> {
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
    PAYLOAD.to_vec()
}

fn installed_version() -> Option<String> {
    let exe = default_install_dir().join("loom.exe");
    exe.exists().then(|| current_version())
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
#[tauri::command]
fn install(_app: AppHandle, dir: String, desktop_shortcut: bool) -> Result<String, String> {
    let install_dir = PathBuf::from(&dir);
    std::fs::create_dir_all(&install_dir).map_err(|e| e.to_string())?;

    let bytes = payload_bytes();
    if bytes.is_empty() {
        return Err("this build has no payload embedded".into());
    }
    extract_zip_bytes(&bytes, &install_dir)?;
    let uninstaller = write_uninstaller(&install_dir)?;
    create_shortcut(&install_dir, desktop_shortcut)?;
    register_uninstall(&install_dir, &uninstaller)?;

    Ok(install_dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn launch_app(dir: String) -> Result<(), String> {
    let exe = PathBuf::from(dir).join("loom.exe");
    std::process::Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn extract_zip_bytes(bytes: &[u8], target: &Path) -> Result<(), String> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;

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
        let mut out = std::fs::File::create(&destination).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
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

    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("shortcut creation failed".to_string())
    }
}

#[cfg(windows)]
fn register_uninstall(install_dir: &Path, uninstaller: &Path) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Loom",
            KEY_WRITE,
        )
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
    key.set_value("NoModify", &1u32)
        .map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(windows))]
fn register_uninstall(_install_dir: &Path, _uninstaller: &Path) -> Result<(), String> {
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
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        extract_zip_bytes(&bytes, &dir)?;
        let uninstaller = write_uninstaller(&dir)?;
        create_shortcut(&dir, false)?;
        register_uninstall(&dir, &uninstaller)?;
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

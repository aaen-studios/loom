//! Putting loom on the computer and taking it off again, for the person alone, with no
//! administrator: the program in %LOCALAPPDATA%\Programs\loom, a Start menu shortcut
//! carrying loom's AppUserModelID (which Windows shows notifications under), and an
//! entry in Settings > Apps, all under HKCU.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::*;

/// Windows shows loom's notifications under this, so the shortcut must carry it.
pub const AUMID: &str = "AaenStudios.Loom";
/// The entry in Settings > Apps. The Inno Setup installer it replaces used the same
/// id, with `_is1` after it, which is removed when this one is written.
const UNINSTALL_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{6F7B4E2A-3C1D-4B8E-9A5F-2D7C8E1B4A90}";
const INNO_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{6F7B4E2A-3C1D-4B8E-9A5F-2D7C8E1B4A90}_is1";
/// v0.1, the Tauri app, as its NSIS installer registered it.
const V01_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Loom";
/// Where the app's "Start at login" puts it (the auto-launch crate, by the app's name).
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

fn env_dir(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} isn't set, so there's nowhere to put loom"))
}

/// %LOCALAPPDATA%\Programs\loom.
pub fn install_dir() -> Result<PathBuf, String> {
    Ok(env_dir("LOCALAPPDATA")?.join("Programs").join("loom"))
}

/// The app's own data: agents, chats and the instance on this computer.
pub fn data_dir() -> Result<PathBuf, String> {
    Ok(env_dir("APPDATA")?.join("Loom"))
}

fn shortcut_path() -> Result<PathBuf, String> {
    Ok(env_dir("APPDATA")?
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join("loom.lnk"))
}

// --- The registry, under HKCU ---

/// A key's path. With LOOM_SETUP_SANDBOX set, as tests set it with LOCALAPPDATA and
/// APPDATA in a folder of their own, the keys are under Software\loom-setup-sandbox
/// instead, so trying the setup never touches the real entries.
fn reg(path: &str) -> String {
    if std::env::var_os("LOOM_SETUP_SANDBOX").is_some() {
        path.replace(
            r"Software\Microsoft\Windows\CurrentVersion",
            r"Software\loom-setup-sandbox",
        )
    } else {
        path.to_string()
    }
}

fn reg_get(key: &str, name: &str) -> Option<String> {
    let (k, n) = (wide(reg(key)), wide(name));
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its length in bytes are passed together.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as _,
            &mut len,
        )
    };
    if r != 0 {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars]))
}

fn reg_set(key: HKEY, name: &str, value: &str) {
    let (n, v) = (wide(name), wide(value));
    // SAFETY: a NUL-terminated wide string and its size in bytes.
    unsafe {
        RegSetValueExW(
            key,
            n.as_ptr(),
            0,
            REG_SZ,
            v.as_ptr() as _,
            (v.len() * 2) as u32,
        )
    };
}

fn reg_set_dword(key: HKEY, name: &str, value: u32) {
    let n = wide(name);
    // SAFETY: four bytes.
    unsafe { RegSetValueExW(key, n.as_ptr(), 0, REG_DWORD, &value as *const u32 as _, 4) };
}

fn reg_delete_tree(key: &str) {
    let k = wide(reg(key));
    // SAFETY: a NUL-terminated path under HKCU; a missing key is no error worth telling.
    unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
    unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, k.as_ptr()) };
}

fn reg_delete_value(key: &str, name: &str) {
    let (k, n) = (wide(reg(key)), wide(name));
    // SAFETY: NUL-terminated strings.
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
}

/// The version of loom this installer put here, if it did.
pub fn installed_version() -> Option<String> {
    reg_get(UNINSTALL_KEY, "DisplayVersion")
}

/// Writes the entry in Settings > Apps.
fn register(dir: &Path, version: &str, bytes: u64) -> Result<(), String> {
    let k = wide(reg(UNINSTALL_KEY));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: creates or opens a key under HKCU, closed below.
    let r = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if r != 0 {
        return Err(format!(
            "can't write loom's entry in Settings > Apps (error {r})"
        ));
    }
    let uninstall = dir.join("uninstall.exe");
    let exe = dir.join("loom.exe");
    reg_set(key, "DisplayName", "loom");
    reg_set(key, "DisplayVersion", version);
    reg_set(key, "Publisher", "Aaen Studios");
    reg_set(key, "URLInfoAbout", "https://github.com/aaen-studios/loom");
    reg_set(key, "DisplayIcon", &exe.to_string_lossy());
    reg_set(key, "InstallLocation", &dir.to_string_lossy());
    reg_set(
        key,
        "UninstallString",
        &format!("\"{}\" --uninstall", uninstall.display()),
    );
    reg_set(
        key,
        "QuietUninstallString",
        &format!("\"{}\" --uninstall --silent", uninstall.display()),
    );
    reg_set_dword(key, "NoModify", 1);
    reg_set_dword(key, "NoRepair", 1);
    reg_set_dword(key, "EstimatedSize", (bytes / 1024) as u32);
    // SAFETY: the key opened above.
    unsafe { RegCloseKey(key) };
    reg_delete_tree(INNO_KEY);
    Ok(())
}

// --- v0.1 ---

/// v0.1, the Tauri app, if it's installed: its version, where it is, and the script its
/// installer left to remove it.
pub struct Old {
    pub version: String,
    dir: Option<PathBuf>,
    script: Option<PathBuf>,
}

pub fn find_v01() -> Option<Old> {
    let version = reg_get(V01_KEY, "DisplayVersion")?;
    if !version.starts_with("0.1") {
        return None;
    }
    let dir = reg_get(V01_KEY, "InstallLocation").map(PathBuf::from);
    // `cmd /C "...\uninstall.cmd"`, as v0.1's installer wrote it.
    let script = reg_get(V01_KEY, "UninstallString").and_then(|s| {
        let path = s.split('"').nth(1)?.to_string();
        path.to_lowercase()
            .ends_with("uninstall.cmd")
            .then(|| PathBuf::from(path))
    });
    Some(Old {
        version,
        dir,
        script,
    })
}

/// Takes v0.1 off once the new loom is in place at `exe`, without running its own
/// uninstaller: that script ends every loom.exe, deletes the Start menu shortcut and the
/// folder v0.1 shares with the new loom. So the setup does what's left of it itself: the
/// Settings entry, the script, v0.1's icon and program where they aren't the new
/// one's, and a desktop shortcut and starting at login are pointed at the new loom.
/// Its data in ~/.loom stays where it is.
pub fn remove_v01(old: &Old, exe: &Path) {
    let me = std::env::current_exe().ok();
    if let Some(dir) = &old.dir {
        let _ = std::fs::remove_file(dir.join("loom.ico"));
        let old_exe = dir.join("loom.exe");
        if !same_path(&old_exe, exe) {
            close(&running(&old_exe));
            if me.as_deref().is_some_and(|m| same_path(m, &old_exe)) {
                remove_later(&old_exe);
            } else {
                let _ = std::fs::remove_file(&old_exe);
                let _ = std::fs::remove_dir(dir);
            }
        }
    }
    if let Some(script) = &old.script {
        let _ = std::fs::remove_file(script);
        if let Some(parent) = script.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    if let Some(desktop) =
        std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(r"Desktop\Loom.lnk"))
    {
        if desktop.exists() {
            let _ = make_shortcut(exe, &desktop);
        }
    }
    if reg_get(RUN_KEY, "Loom").is_some() {
        if let Some(key) = open_key(RUN_KEY) {
            reg_set(key, "loom", &format!("\"{}\" --background", exe.display()));
            // SAFETY: opened by open_key.
            unsafe { RegCloseKey(key) };
        }
    }
    reg_delete_tree(V01_KEY);
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

// --- Processes ---

/// The loom programs running from `exe`.
pub fn running(exe: &Path) -> Vec<u32> {
    let want = exe.to_string_lossy().to_lowercase();
    let mut found = Vec::new();
    // SAFETY: a snapshot of the processes, walked and closed; each process is opened
    // only to read its program's path, and closed.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        while more {
            let name = String::from_utf16_lossy(
                &e.szExeFile[..e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
            );
            // Never this program, which is loom.exe itself when v0.1 hands over to it.
            if name.eq_ignore_ascii_case("loom.exe") && e.th32ProcessID != std::process::id() {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                if !h.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0
                        && String::from_utf16_lossy(&buf[..len as usize]).to_lowercase() == want
                    {
                        found.push(e.th32ProcessID);
                    }
                    CloseHandle(h);
                }
            }
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    found
}

/// Waits up to `ms` for process `pid` to end. True once it has, or if it isn't there.
pub fn wait_for(pid: u32, ms: u32) -> bool {
    // SAFETY: the handle is checked, waited on, and closed.
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return true;
        }
        let r = WaitForSingleObject(h, ms);
        CloseHandle(h);
        r == WAIT_OBJECT_0
    }
}

/// Ends the loom programs given, and waits for them to go.
pub fn close(pids: &[u32]) {
    for &pid in pids {
        // SAFETY: opened to end it, then closed.
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
            if !h.is_null() {
                TerminateProcess(h, 0);
                WaitForSingleObject(h, 5000);
                CloseHandle(h);
            }
        }
    }
}

/// Whether this program runs with an administrator's full rights.
pub fn elevated() -> bool {
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    // SAFETY: this process's token, read and closed.
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut e: TOKEN_ELEVATION = std::mem::zeroed();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut e as *mut _ as _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        ) != 0;
        CloseHandle(token);
        ok && e.TokenIsElevated != 0
    }
}

pub fn launch(exe: &Path, args: &[&str]) -> Result<std::process::Child, String> {
    std::process::Command::new(exe)
        .args(args)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| format!("can't start loom: {e}"))
}

// --- Files ---

/// Writes `data` to `path` beside where it goes, then moves it there.
fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let part = path.with_extension("part");
    std::fs::write(&part, data).map_err(|e| format!("can't write {}: {e}", part.display()))?;
    retry(|| std::fs::rename(&part, path))
        .map_err(|e| format!("can't put {} in place: {e}", path.display()))
}

/// A file a program just let go of can stay locked for a moment (an antivirus
/// reading it, say), so a move is tried for up to five seconds.
fn retry(mut f: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..50 {
        match f() {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(last.unwrap())
}

/// Puts this version of loom in place: the program, with the one before kept as
/// loom.old.exe until the new one has started, and this setup's own program as
/// uninstall.exe, then the shortcut and the Settings entry.
pub fn place(dir: &Path, exe: &[u8], stub: &[u8], version: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    let target = dir.join("loom.exe");
    let old = dir.join("loom.old.exe");
    let new = dir.join("loom.new.exe");
    std::fs::write(&new, exe).map_err(|e| format!("can't write {}: {e}", new.display()))?;
    if target.exists() {
        let _ = std::fs::remove_file(&old);
        retry(|| std::fs::rename(&target, &old))
            .map_err(|e| format!("can't move the loom that's there aside: {e}"))?;
    }
    if let Err(e) = retry(|| std::fs::rename(&new, &target)) {
        if old.exists() {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(format!("can't put loom in place: {e}"));
    }
    write_atomic(&dir.join("uninstall.exe"), stub)?;
    let _ = std::fs::write(dir.join("OFL.txt"), crate::draw::FONT_LICENCE);
    shortcut(&target)?;
    register(dir, version, (exe.len() + stub.len()) as u64)?;
    Ok(())
}

/// Puts loom.old.exe back, after a new version that didn't start.
pub fn roll_back(dir: &Path, version: &str) -> Result<(), String> {
    let target = dir.join("loom.exe");
    let old = dir.join("loom.old.exe");
    if !old.exists() {
        return Err("the version before isn't there to go back to".into());
    }
    let broken = dir.join("loom.broken.exe");
    let _ = std::fs::remove_file(&broken);
    retry(|| std::fs::rename(&target, &broken))
        .map_err(|e| format!("can't move the new loom aside: {e}"))?;
    retry(|| std::fs::rename(&old, &target))
        .map_err(|e| format!("can't put the old loom back: {e}"))?;
    let _ = std::fs::remove_file(&broken);
    if let Some(key) = open_key(UNINSTALL_KEY) {
        reg_set(key, "DisplayVersion", version);
        // SAFETY: opened by open_key.
        unsafe { RegCloseKey(key) };
    }
    Ok(())
}

/// An existing key under HKCU, open for writing, for the caller to close.
fn open_key(path: &str) -> Option<HKEY> {
    let k = wide(reg(path));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: opens an existing key for writing; the caller closes it.
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, k.as_ptr(), 0, KEY_WRITE, &mut key) };
    (r == 0).then_some(key)
}

/// Removes loom.old.exe once the new version has started well.
pub fn drop_old(dir: &Path) {
    let _ = retry(|| std::fs::remove_file(dir.join("loom.old.exe")));
}

/// Takes loom off: the program, the shortcut, starting at login and the Settings entry,
/// and the person's data too when `data`.
pub fn remove(dir: &Path, data: bool) -> Result<(), String> {
    let dir_s = dir.to_string_lossy().to_lowercase();
    // Only ever the folder loom is installed in, never a parent of it.
    if !dir_s.ends_with(r"\programs\loom") {
        return Err(format!(
            "{} isn't where loom installs, so nothing there is touched",
            dir.display()
        ));
    }
    close(&running(&dir.join("loom.exe")));
    // Normally this program runs from a copy in the temporary folder, but if it runs
    // from the folder itself, it's removed once it has exited rather than failing.
    let me = std::env::current_exe().ok();
    let mut later = false;
    for name in [
        "loom.exe",
        "loom.old.exe",
        "loom.new.exe",
        "loom.broken.exe",
        "uninstall.exe",
        "uninstall.part",
        "OFL.txt",
        "loom.ico",
    ] {
        let p = dir.join(name);
        if me.as_deref().is_some_and(|m| same_path(m, &p)) {
            later = true;
        } else if p.exists() {
            retry(|| std::fs::remove_file(&p)).map_err(|e| format!("can't remove {name}: {e}"))?;
        }
    }
    match me {
        Some(m) if later => remove_later(&m),
        _ => {
            let _ = std::fs::remove_dir(dir);
        }
    }
    if let Ok(lnk) = shortcut_path() {
        let _ = std::fs::remove_file(lnk);
    }
    reg_delete_value(RUN_KEY, "loom");
    reg_delete_tree(UNINSTALL_KEY);
    reg_delete_tree(INNO_KEY);
    if data {
        let d = data_dir()?;
        if d.ends_with("Loom") && d.exists() {
            std::fs::remove_dir_all(&d)
                .map_err(|e| format!("can't remove {}: {e}", d.display()))?;
        }
    }
    Ok(())
}

/// A copy of this program in the temporary folder, to uninstall from, since a running
/// program can't remove itself.
pub fn copy_to_temp() -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let to = std::env::temp_dir().join(format!("loom-uninstall-{}.exe", std::process::id()));
    std::fs::copy(&me, &to)
        .map_err(|e| format!("can't copy the uninstaller to {}: {e}", to.display()))?;
    Ok(to)
}

/// Has Windows remove this program a few seconds after it ends: the uninstaller's copy
/// in the temporary folder.
pub fn remove_self_later() {
    let Ok(me) = std::env::current_exe() else {
        return;
    };
    if me.starts_with(std::env::temp_dir()) {
        remove_later(&me);
    }
}

/// Has Windows remove the program at `path`, which is still running, a few seconds
/// after this one ends, and its folder if that leaves it empty.
pub fn remove_later(path: &Path) {
    // Tried each second for up to 20, while the program is still ending.
    let p = path.display();
    let mut line = format!(
        "for /l %i in (1,1,20) do @(if exist \"{p}\" (ping -n 2 127.0.0.1 >nul & del /q \"{p}\" >nul 2>&1))"
    );
    if let Some(dir) = path.parent() {
        // rmdir without /s takes only an empty folder.
        line.push_str(&format!(" & rmdir \"{}\" >nul 2>&1", dir.display()));
    }
    let _ = std::process::Command::new("cmd.exe")
        .arg("/c")
        .raw_arg(line)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

// --- Shortcuts ---

fn shortcut(exe: &Path) -> Result<(), String> {
    make_shortcut(exe, &shortcut_path()?)
}

/// A shortcut at `lnk` to `exe`, with loom's AppUserModelID.
fn make_shortcut(exe: &Path, lnk: &Path) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    if let Some(parent) = lnk.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let fail = |e: windows::core::Error| format!("can't make the Start menu shortcut: {e}");
    // SAFETY: COM on this thread, for a shell link that's made, given its target and
    // AppUserModelID, saved and released.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(fail)?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))
            .map_err(fail)?;
        if let Some(dir) = exe.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))
                .map_err(fail)?;
        }
        // loom.exe's light icon, or its dark one for a dark system; loom keeps it
        // matching its own theme after (desktop/src/shell.rs).
        let icon = if crate::window::dark_mode() { 1 } else { 0 };
        link.SetIconLocation(&HSTRING::from(exe.as_os_str()), icon)
            .map_err(fail)?;
        link.SetDescription(&HSTRING::from("loom")).map_err(fail)?;
        let store: IPropertyStore = link.cast().map_err(fail)?;
        let id = windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from(AUMID);
        store.SetValue(&PKEY_AppUserModel_ID, &id).map_err(fail)?;
        store.Commit().map_err(fail)?;
        let file: IPersistFile = link.cast().map_err(fail)?;
        file.Save(&HSTRING::from(lnk.as_os_str()), true)
            .map_err(fail)?;
    }
    Ok(())
}

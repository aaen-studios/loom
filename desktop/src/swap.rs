//! A Mac update, put in place by the new loom itself (PLAN.md, "Updates and versions").
//!
//! The core downloads the new release's `loom-mac.dmg` and checks it against the signed
//! manifest (core/update.calx). Restarting to update, the app copies the new `loom.app`
//! out of it beside the disk image (`stage`), starts that copy with `--swap`, and quits
//! (`hand_over`). The new loom waits for the old one to end, moves the old bundle aside
//! and itself into its place, starts itself from there and watches it for ten seconds
//! (`run`). If it ends with an error, the old bundle goes back and starts again, so the
//! person always gets a loom back. Each step is a line in `update.log` in the data folder.

use std::io;
use std::path::Path;

/// Moves the bundle at `target` aside to `aside`, and `new` into its place. If the
/// second move fails, the first is undone, and the old bundle is where it was.
pub fn swap_in(target: &Path, new: &Path, aside: &Path) -> io::Result<()> {
    std::fs::rename(target, aside)?;
    if let Err(e) = std::fs::rename(new, target) {
        std::fs::rename(aside, target)?;
        return Err(e);
    }
    Ok(())
}

/// Undoes `swap_in`: the new bundle at `target` goes back to `new`, and the old one at
/// `aside` back to `target`.
pub fn put_back(target: &Path, new: &Path, aside: &Path) -> io::Result<()> {
    std::fs::rename(target, new)?;
    std::fs::rename(aside, target)
}

#[cfg(target_os = "macos")]
pub use mac::{hand_over, run, stage};

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// How long the new loom has to start before it counts as started.
    const WATCH: Duration = Duration::from_secs(10);
    /// How long the swap waits for the old loom to quit, as it stops its cores.
    const QUIT_WAIT: Duration = Duration::from_secs(60);

    fn log(line: &str) {
        eprintln!("loom: {line}");
        let path = Path::new(&crate::app::data_dir()).join("update.log");
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = std::io::Write::write_all(&mut f, format!("{at} {line}\n").as_bytes());
        }
    }

    fn run_tool(program: &str, args: &[&std::ffi::OsStr]) -> io::Result<()> {
        let out = Command::new(program).args(args).output()?;
        if out.status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }

    /// Copies the `loom.app` in the disk image at `dmg` out beside it, and gives where.
    /// The disk image is mounted where nothing shows it, and put away again after.
    pub fn stage(dmg: &Path) -> io::Result<PathBuf> {
        let dir = dmg
            .parent()
            .ok_or_else(|| io::Error::other("the update has no folder"))?;
        let staged = dir.join("loom.app");
        // Left from an update that didn't finish: this folder is the core's, for updates.
        if staged.exists() {
            std::fs::remove_dir_all(&staged)?;
        }
        let mount = std::env::temp_dir().join(format!("loom-update-{}", std::process::id()));
        std::fs::create_dir_all(&mount)?;
        run_tool(
            "hdiutil",
            &[
                "attach".as_ref(),
                "-nobrowse".as_ref(),
                "-readonly".as_ref(),
                "-noautoopen".as_ref(),
                "-noverify".as_ref(),
                "-mountpoint".as_ref(),
                mount.as_os_str(),
                dmg.as_os_str(),
            ],
        )?;
        let copied = std::fs::read_dir(&mount).and_then(|entries| {
            let app = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or_else(|| io::Error::other("the update's disk image has no app in it"))?;
            run_tool("ditto", &[app.as_os_str(), staged.as_os_str()])
        });
        let detached = run_tool("hdiutil", &["detach".as_ref(), mount.as_os_str()]).or_else(|_| {
            run_tool(
                "hdiutil",
                &["detach".as_ref(), "-force".as_ref(), mount.as_os_str()],
            )
        });
        if let Err(e) = detached {
            log(&format!("the update's disk image stays mounted: {e}"));
        }
        let _ = std::fs::remove_dir(&mount);
        copied?;
        Ok(staged)
    }

    /// Starts the new loom at `staged` to put itself in place of the bundle at `target`
    /// once this loom has quit, in the menu bar when `background`.
    pub fn hand_over(staged: &Path, target: &Path, background: bool) -> io::Result<()> {
        let mut c = Command::new(staged.join("Contents/MacOS/loom"));
        c.arg("--swap")
            .arg(target)
            .arg("--pid")
            .arg(std::process::id().to_string());
        if background {
            c.arg("--background");
        }
        c.spawn()?;
        Ok(())
    }

    /// Whether the process `pid` is still running.
    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only asks whether the process is there.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Starts the loom in the bundle at `bundle`, in the menu bar when `background`.
    fn start(bundle: &Path, background: bool) -> io::Result<std::process::Child> {
        let mut c = Command::new(bundle.join("Contents/MacOS/loom"));
        if background {
            c.arg("--background");
        }
        c.spawn()
    }

    /// Whether the loom just started ended with an error within `WATCH`, and how.
    fn watch(mut child: std::process::Child) -> Option<String> {
        let began = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if !status.success() => {
                    return Some(format!("ended at once, {status}"))
                }
                Ok(Some(_)) => return None,
                Ok(None) if began.elapsed() >= WATCH => return None,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Some(format!("can't be watched: {e}")),
            }
        }
    }

    /// `loom --swap TARGET --pid PID [--background]`: run by the new loom from where
    /// `stage` put it. Gives the exit status.
    pub fn run() -> i32 {
        let args: Vec<String> = std::env::args().collect();
        let after = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let background = args.iter().any(|a| a == "--background");
        let (Some(target), Some(pid)) = (after("--swap"), after("--pid")) else {
            eprintln!("usage: loom --swap TARGET --pid PID [--background]");
            return 2;
        };
        let target = PathBuf::from(target);
        let pid: i32 = pid.parse().unwrap_or(0);
        let Some(new) = crate::mac::bundle() else {
            log("the update isn't in an app, so it can't put itself in place");
            return 1;
        };
        let Some(aside) = new.parent().map(|d| d.join("previous.app")) else {
            return 1;
        };
        let waiting = Instant::now();
        while pid > 0 && alive(pid) {
            if waiting.elapsed() > QUIT_WAIT {
                log(&format!("loom {pid} didn't quit, so the update waits"));
                return 1;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // Left from an update that didn't finish, in the core's folder for updates.
        if aside.exists() {
            if let Err(e) = std::fs::remove_dir_all(&aside) {
                log(&format!("can't clear {}: {e}", aside.display()));
                return 1;
            }
        }
        let version = crate::VERSION;
        if let Err(e) = super::swap_in(&target, &new, &aside) {
            log(&format!(
                "can't put loom {version} in place of {}: {e}",
                target.display()
            ));
            let _ = start(&target, background);
            return 1;
        }
        log(&format!(
            "put loom {version} in place of {}",
            target.display()
        ));
        let failed = match start(&target, background) {
            Err(e) => Some(format!("didn't start: {e}")),
            Ok(child) => watch(child),
        };
        let Some(why) = failed else {
            if let Err(e) = std::fs::remove_dir_all(&aside) {
                log(&format!("the old loom stays at {}: {e}", aside.display()));
            }
            log(&format!("loom {version} started"));
            return 0;
        };
        log(&format!("loom {version} {why}, so the old loom goes back"));
        match super::put_back(&target, &new, &aside) {
            Ok(()) => {
                let _ = start(&target, background);
            }
            Err(e) => log(&format!("the old loom couldn't go back: {e}")),
        }
        1
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn bundle(at: &Path, version: &str) {
        std::fs::create_dir_all(at.join("Contents")).unwrap();
        std::fs::write(at.join("Contents/version"), version).unwrap();
    }

    fn version(at: &Path) -> String {
        std::fs::read_to_string(at.join("Contents/version")).unwrap()
    }

    /// The new bundle goes in the old one's place, which waits aside, and back again.
    #[test]
    fn swaps_a_bundle_in_and_puts_the_old_one_back() {
        let dir = std::env::temp_dir().join(format!("loom-swap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("update")).unwrap();
        let target = dir.join("loom.app");
        let new = dir.join("update/loom.app");
        let aside = dir.join("update/previous.app");
        bundle(&target, "0.2.5");
        bundle(&new, "0.2.6");
        super::swap_in(&target, &new, &aside).unwrap();
        assert_eq!(version(&target), "0.2.6");
        assert_eq!(version(&aside), "0.2.5");
        assert!(!new.exists());
        super::put_back(&target, &new, &aside).unwrap();
        assert_eq!(version(&target), "0.2.5");
        assert_eq!(version(&new), "0.2.6");
        assert!(!aside.exists());
        // A new bundle that isn't there leaves the old one in its place.
        std::fs::remove_dir_all(&new).unwrap();
        assert!(super::swap_in(&target, &new, &aside).is_err());
        assert_eq!(version(&target), "0.2.5");
        assert!(!aside.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

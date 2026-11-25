# setup

`loom-setup.exe`: loom's installer, updater and uninstaller for Windows, in one small program.

It's a program of its own rather than part of the GPUI app, so that it stays small and opens at once: a plain Win32 window with no frame of Windows' own, drawn by hand with [tiny-skia](https://github.com/linebender/tiny-skia) (paths) and [fontdue](https://github.com/mooman219/fontdue) (Geist), in loom's look. The colours, sizes and motion come from `design/tokens.toml` (`design/generated/tokens.rs`), the knot and the wordmark from `design/logo`, the buttons and the switch are drawn as the app draws its own (`app.rs`, `button_owned` and `switch`), and the thread moves as `desktop/src/thread.rs` moves it: it draws itself across when the window opens, the mark settles onto it, and while the setup works, a loop travels along it and the thread behind the loop takes loom's colour, so the thread is the progress.

Measured on the reference PC, release build:

| | |
|---|---|
| The setup's own program | 1.18 MB |
| With loom 0.2.0 packed in (`loom-setup.exe` on a release) | 11.3 MB, from loom.exe's 39.4 MB |
| Its window, from starting it | about 40 ms |

For comparison, the same window was tried three ways first (LOO-152): GPUI on its own came to 4.7 MB and 240 ms, Tauri to 2.8 MB plus WebView2's processes, and drawing by hand to 0.68 MB and 25 ms.

## What it does

```text
loom-setup.exe                                  install, with a window
loom-setup.exe --silent                         install, without one
loom-setup.exe --update --pid N [--background]  the app hands over an update and quits
uninstall.exe --uninstall [--silent]            take loom off again
```

- **Installing** puts `loom.exe` in `%LOCALAPPDATA%\Programs\loom`, for the person alone, with no administrator. It keeps a copy of its own program beside it as `uninstall.exe`, makes the Start menu shortcut with loom's AppUserModelID (`AaenStudios.Loom`, which Windows shows loom's notifications under) and the app icon in the system's theme, light or dark (loom keeps it matching its own theme after), and writes the entry in Settings > Apps and makes `loom://` links open loom (`Software\Classes\loom`, which a `loom://add?…` link that adds this computer needs), all under HKCU. If loom is running, it asks to close it first.
- **Updating.** The core downloads a newer release's `loom-setup.exe` and checks it against the signed manifest (`core/update.calx`), and the app starts it with `--update --pid` and quits (`shell::apply_update`). The setup waits for the app to end, puts the new `loom.exe` in, keeping the old one as `loom.old.exe`, starts the new one and watches it for ten seconds. If it ends with an error, or doesn't start, the old one goes back, and the window says so. Before it starts the old one, it keeps the version that failed, and why, in `update\failed.json` in the app's data folder, so the old loom doesn't put the same release in again on its own (`core/update.calx`). Otherwise `loom.old.exe` is removed. With `--background` (an update put in while loom was in the tray) there's no window at all, and loom starts in the tray again. Whatever happens, the person gets a loom back. Each update is a line in `update.log` in the app's data folder.
- **Uninstalling** runs from a copy in the temporary folder, so it can remove everything: the program, the shortcut, starting at login, the Settings entry and `loom://` links, and with the switch on, the agents and chats in the app's data folder. The copy removes itself once it has ended.
- **v0.1.** Installing over v0.1 takes it off without running its own uninstaller, which ends every `loom.exe` and deletes the folder the new loom goes in: the setup removes its Settings entry, its uninstall script and its icon, and points its desktop shortcut and starting at login at the new loom. Its data in `~/.loom` stays. v0.1's own updater hands over too: a release carries an `update.json` in v0.1's format (`scripts/release.calx bridge`), whose payload is this setup named `loom.exe`, so v0.1 swaps it in and starts it, and the setup, finding itself called `loom.exe`, installs loom and takes v0.1 off.

## What it trusts

Nothing is installed unless it's signed with loom's release key. `scripts/release.calx pack` puts `loom.exe` on the end of the setup's program as one zstd frame, under a header with its version, SHA-256 and size, and signs `"loom setup\n"` and that header with the release key (the layout is in [`src/pack.rs`](src/pack.rs)). The setup checks that signature with the public key from `shared/release.calx`, built in by `build.rs`, then unpacks and checks the SHA-256 and the size, before it writes anything. A setup that the app downloads is checked against the release's signed manifest as well.

## Building and trying it

`cargo build --release` here builds the setup's program, which carries nothing yet: on its own it says it can't install loom. To try one end to end without the release key, build it with `LOOM_SETUP_TEST_KEY` set to a test key's public half, and pack it with `LOOM_PACK_TEST_SEED` set to that key's seed (`scripts/release.calx` prints the public half for you).

- `LOCALAPPDATA` and `APPDATA` say where it installs, and `LOOM_SETUP_SANDBOX` keeps its registry keys under `Software\loom-setup-sandbox`, so pointing the first two into a folder of their own and setting the third leaves the computer's own loom alone. The loom it starts then doesn't turn starting at login on either.
- `LOOM_THEME` (`dark` or `light`) picks the theme instead of following the system, as in the app, and `LOOM_SETUP_NO_FOCUS` opens the window off screen and without taking the focus, for tests that capture it with PrintWindow.

[`tests/setup/e2e.calx`](../../tests/setup/e2e.calx) does all of that: it installs, rolls back an update that can't start, updates, refuses a setup signed with another key, uninstalls, and has v0.1 hand over, in a folder of its own. CI runs it on Windows.

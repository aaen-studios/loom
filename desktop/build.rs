// Builds the Calx core into a static library with `calx build --staticlib`,
// and links it. The core's source is ../core; Calx finds the modules it
// imports from the repository's root.
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let lib = if cfg!(windows) {
        out.join("loomcore.lib")
    } else {
        out.join("libloomcore.a")
    };
    let calx = env::var("CALX").unwrap_or_else(|_| "calx".into());
    let mut cmd = Command::new(calx);
    cmd.current_dir(&root).arg("build").arg("--staticlib");
    if env::var("PROFILE").as_deref() == Ok("release") {
        cmd.arg("--release");
    }
    let status = cmd
        .arg("core/lib.calx")
        .arg("-o")
        .arg(&lib)
        .status()
        .expect("calx isn't on PATH");
    assert!(status.success(), "calx build --staticlib failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=loomcore");
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=dylib=m");
        println!("cargo:rustc-link-lib=dylib=pthread");
        println!("cargo:rustc-link-lib=dylib=dl");
    }
    // The exe's icon, which the taskbar, the window and Explorer show.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(&root.join("design/logo/app.ico").to_string_lossy());
        res.set("ProductName", "loom");
        res.set("FileDescription", "loom");
        res.compile().expect("the Windows resources didn't build");
    }
    for dir in ["core", "shared", "server"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
}

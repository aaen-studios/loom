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
    // `loom mcp` alone, a small program of its own (server/mcp_main.calx), which the
    // app carries and writes into its folder for the instance's agent sessions: it
    // starts in half the time the app does (LOO-222).
    let mcp = out.join(if cfg!(windows) {
        "loom-mcp.exe"
    } else {
        "loom-mcp"
    });
    let mut build_mcp = Command::new(&calx);
    build_mcp.current_dir(&root).arg("build");
    if env::var("PROFILE").as_deref() == Ok("release") {
        build_mcp.arg("--release");
    }
    let built = build_mcp
        .arg("server/mcp_main.calx")
        .arg("-o")
        .arg(&mcp)
        .status()
        .expect("calx isn't on PATH");
    assert!(built.success(), "calx build server/mcp_main.calx failed");
    let bytes = std::fs::read(&mcp).expect("the built loom mcp isn't there");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&bytes, &mut hasher);
    println!("cargo:rustc-env=LOOM_MCP_PROGRAM={}", mcp.display());
    println!(
        "cargo:rustc-env=LOOM_MCP_HASH={:016x}",
        std::hash::Hasher::finish(&hasher)
    );
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
        // The light app icon first, which Explorer shows, and the dark one second, for
        // a dark theme (shell::follow_theme).
        res.set_icon(&root.join("design/logo/app-light.ico").to_string_lossy());
        res.set_icon_with_id(&root.join("design/logo/app.ico").to_string_lossy(), "2");
        res.set("ProductName", "loom");
        res.set("FileDescription", "loom");
        res.compile().expect("the Windows resources didn't build");
    }
    for dir in ["core", "shared", "server"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    // Twemoji's pictures (design/twemoji), which the app draws built-in emoji with: each
    // SVG's name and bytes, in the order of their names, for src/emoji.rs.
    let twemoji = root.join("design/twemoji/svg");
    let mut names: Vec<String> = std::fs::read_dir(&twemoji)
        .expect("design/twemoji/svg isn't there: run calx scripts/emoji.calx")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".svg").map(String::from))
        .collect();
    // By name without .svg, as src/emoji.rs looks them up: `270b` before `270b-1f3fb`.
    names.sort();
    let mut table = String::from("pub static SVGS: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let path = twemoji.join(format!("{n}.svg"));
        table.push_str(&format!(
            "    ({:?}, include_bytes!({:?})),\n",
            n,
            path.to_string_lossy()
        ));
    }
    table.push_str("];\n");
    std::fs::write(out.join("twemoji.rs"), table).expect("can't write twemoji.rs");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("design/twemoji").display()
    );
}

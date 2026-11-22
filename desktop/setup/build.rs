//! Takes the release key from shared/release.calx, the one place it's written, and gives
//! the program loom's icon and a manifest that asks for no administrator.

fn main() {
    let release = std::fs::read_to_string("../../shared/release.calx")
        .expect("shared/release.calx, for the release key");
    let key = release
        .lines()
        .find_map(|l| {
            l.strip_prefix("pub const RELEASE_KEY: str = \"")
                .and_then(|r| r.strip_suffix('"'))
        })
        .expect("RELEASE_KEY in shared/release.calx");
    // A build for trying packs signed with a key of your own (scripts/release.calx pack
    // with LOOM_PACK_TEST_SEED). Release builds never set it.
    let key = std::env::var("LOOM_SETUP_TEST_KEY").unwrap_or_else(|_| key.to_string());
    println!("cargo:rustc-env=LOOM_RELEASE_KEY_HEX={key}");
    println!("cargo:rerun-if-changed=../../shared/release.calx");
    println!("cargo:rerun-if-env-changed=LOOM_SETUP_TEST_KEY");
    let version = std::env::var("LOOM_VERSION").unwrap_or_else(|_| "0.0.0-dev".into());
    println!("cargo:rustc-env=LOOM_SETUP_VERSION={version}");
    println!("cargo:rerun-if-env-changed=LOOM_VERSION");

    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../design/logo/app.ico");
        res.set("FileDescription", "loom setup");
        res.set("ProductName", "loom");
        res.set("CompanyName", "Aaen Studios");
        res.set_manifest(MANIFEST);
        res.compile().expect("the program's resources");
    }
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges>
      <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
    </requestedPrivileges></security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
</assembly>
"#;

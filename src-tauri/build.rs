fn main() {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.contains("windows") && target.contains("windows") {
        // cargo-xwin cannot invoke rc.exe from WSL. Tauri still needs to generate
        // its ACL and context data, but Windows resources are added by native
        // release builds on GitHub Actions.
        unsafe { std::env::set_var("TARGET", &host) };
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    tauri_build::build()
}

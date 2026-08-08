fn main() {
    println!("cargo:rerun-if-changed=assets/icon/lmukit.ico");

    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if host.contains("windows") && target.contains("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/icon/lmukit.ico")
            .set("ProductName", "LMUKit")
            .set("FileDescription", "LMUKit for Le Mans Ultimate")
            .compile()
            .expect("failed to embed LMUKit's Windows resources");
    }
}

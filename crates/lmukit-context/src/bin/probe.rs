#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use lmukit_context::windows::WindowsLmuCarContextProvider;
    use lmukit_core::car_context::LmuCarContextProvider;

    let mut provider = WindowsLmuCarContextProvider::connect()?;
    let context = provider.current_context()?;
    println!("{}", serde_json::to_string_pretty(&context)?);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("lmukit-context-probe only reads LMU shared memory on Windows");
    std::process::exit(1);
}

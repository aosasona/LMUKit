#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("lmukit-virtual-probe is only available on Windows");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    std::thread::spawn(move || windows_probe::run(&args))
        .join()
        .map_err(|_| anyhow::anyhow!("the probe worker stopped unexpectedly"))?
}

#[cfg(target_os = "windows")]
mod windows_probe {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::{Duration, Instant},
    };

    use anyhow::{Context, Result, bail, ensure};
    use lmukit_virtual::{
        TwoButtonMirror, VirtualInputBackend,
        direct_input::{DirectInputButtonReader, list_devices},
        vjoy::VJoyBackend,
    };
    use windows::{
        Gaming::Input::{GameControllerSwitchPosition, RawGameController},
        Win32::{
            Foundation::BOOL,
            System::{
                Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler},
                WinRT::{RO_INIT_SINGLETHREADED, RoInitialize},
            },
        },
    };

    static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn console_handler(control_type: u32) -> BOOL {
        if control_type == CTRL_C_EVENT || control_type == CTRL_BREAK_EVENT {
            STOP_REQUESTED.store(true, Ordering::SeqCst);
            BOOL(1)
        } else {
            BOOL(0)
        }
    }

    pub fn run(args: &[String]) -> Result<()> {
        // SAFETY: This worker is a fresh OS thread and owns its WinRT apartment.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
            .ok()
            .context("initialising Windows Runtime input discovery")?;
        // SAFETY: The handler only updates an atomic flag and remains valid for
        // the lifetime of this process.
        unsafe { SetConsoleCtrlHandler(Some(console_handler), true) }
            .context("installing the console cleanup handler")?;

        match args.first().map(String::as_str) {
            Some("list") => list_controllers(),
            Some("directinput-list") => list_direct_input_controllers(),
            Some("pulse") => pulse(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u64(args.get(2), 1_500, "button hold milliseconds")?,
            ),
            Some("button") => press_button(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u32(args.get(2), 1, "button ID")?,
                parse_u64(args.get(3), 1_500, "button hold milliseconds")?,
            ),
            Some("mirror") => mirror(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u32(args.get(2), 0, "controller index")?,
                parse_u64(args.get(3), 30, "duration seconds")?,
            ),
            Some("directinput-mirror") => direct_input_mirror(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u32(args.get(2), 0, "DirectInput controller index")? as usize,
                parse_u64(args.get(3), 30, "duration seconds")?,
            ),
            Some("verify") => verify(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u32(args.get(2), 0, "DirectInput controller index")? as usize,
            ),
            _ => {
                print_help();
                Ok(())
            }
        }
    }

    fn parse_u32(value: Option<&String>, default: u32, label: &str) -> Result<u32> {
        value.map_or(Ok(default), |value| {
            value.parse().with_context(|| format!("invalid {label}"))
        })
    }

    fn parse_u64(value: Option<&String>, default: u64, label: &str) -> Result<u64> {
        value.map_or(Ok(default), |value| {
            value.parse().with_context(|| format!("invalid {label}"))
        })
    }

    fn print_help() {
        println!("LMUKit experimental virtual-input probe");
        println!("  lmukit-virtual-probe list");
        println!("  lmukit-virtual-probe directinput-list");
        println!("  lmukit-virtual-probe pulse [vjoy-device-id] [hold-ms]");
        println!("  lmukit-virtual-probe button [vjoy-device-id] [button-id] [hold-ms]");
        println!("  lmukit-virtual-probe mirror [vjoy-device-id] [controller-index] [seconds]");
        println!(
            "  lmukit-virtual-probe directinput-mirror [vjoy-device-id] [directinput-index] [seconds]"
        );
        println!("  lmukit-virtual-probe verify [vjoy-device-id] [directinput-index]");
        println!("This tool does not install a driver or modify LMU configuration.");
    }

    fn list_controllers() -> Result<()> {
        let controllers = RawGameController::RawGameControllers()?;
        let count = controllers.Size()?;
        if count == 0 {
            println!("No RawGameController devices are currently visible.");
            return Ok(());
        }
        for index in 0..count {
            let controller = controllers.GetAt(index)?;
            println!(
                "[{index}] {} — VID {:04X}, PID {:04X}, {} buttons, {} switches, {} axes",
                controller.DisplayName()?,
                controller.HardwareVendorId()?,
                controller.HardwareProductId()?,
                controller.ButtonCount()?,
                controller.SwitchCount()?,
                controller.AxisCount()?,
            );
        }
        println!("Device instance identifiers are intentionally omitted from output.");
        Ok(())
    }

    fn list_direct_input_controllers() -> Result<()> {
        let devices = list_devices()?;
        if devices.is_empty() {
            println!("No attached DirectInput controllers are visible.");
            return Ok(());
        }
        for device in devices {
            println!(
                "[{}] {} — product {}, {} buttons",
                device.index, device.name, device.product_name, device.button_count
            );
        }
        Ok(())
    }

    fn verify(device_id: u32, direct_input_index: usize) -> Result<()> {
        let reader = DirectInputButtonReader::open(direct_input_index)?;
        println!(
            "Observing DirectInput [{}] {} (product {}, {} buttons)",
            reader.summary.index,
            reader.summary.name,
            reader.summary.product_name,
            reader.summary.button_count
        );
        let mut backend = VJoyBackend::new(device_id)?;
        backend.connect()?;
        ensure!(
            backend.capabilities()?.button_count >= 2,
            "configure at least two vJoy buttons"
        );
        backend.release_all()?;
        wait_for_buttons(&reader, [false, false], "initial release")?;

        for button in 1..=2_usize {
            let mut expected = [false, false];
            expected[button - 1] = true;
            let started = Instant::now();
            backend.set_button(button as u32, true)?;
            wait_for_buttons(&reader, expected, &format!("button {button} press"))?;
            println!(
                "Button {button} press observed after {:?}",
                started.elapsed()
            );

            let started = Instant::now();
            backend.set_button(button as u32, false)?;
            wait_for_buttons(&reader, [false, false], &format!("button {button} release"))?;
            println!(
                "Button {button} release observed after {:?}",
                started.elapsed()
            );
        }

        backend.disconnect()?;
        wait_for_buttons(&reader, [false, false], "disconnect cleanup")?;
        println!("PASS: DirectInput observed both press/release pairs and clean disconnect.");
        Ok(())
    }

    fn wait_for_buttons(
        reader: &DirectInputButtonReader,
        expected: [bool; 2],
        label: &str,
    ) -> Result<()> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            if reader.first_two_buttons()? == expected {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(1));
        }
        bail!(
            "DirectInput did not observe {label}; expected {:?}, last state {:?}",
            expected,
            reader.first_two_buttons()?
        )
    }

    fn pulse(device_id: u32, hold_milliseconds: u64) -> Result<()> {
        ensure!(
            (100..=5_000).contains(&hold_milliseconds),
            "button hold duration must be 100..=5000 milliseconds"
        );
        let mut backend = VJoyBackend::new(device_id)?;
        backend.connect()?;
        let capabilities = backend.capabilities()?;
        ensure!(
            capabilities.button_count >= 2,
            "configure at least two vJoy buttons"
        );
        println!(
            "Connected to {} with {} buttons. LMU recognition is not yet proven.",
            capabilities.identity, capabilities.button_count
        );
        for button in 1..=2 {
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                break;
            }
            println!("Pressing virtual button {button}");
            backend.set_button(button, true)?;
            thread::sleep(Duration::from_millis(hold_milliseconds));
            backend.set_button(button, false)?;
            println!("Released virtual button {button}");
            thread::sleep(Duration::from_millis(500));
        }
        backend.disconnect()?;
        println!("All virtual buttons released; device disconnected.");
        Ok(())
    }

    fn press_button(device_id: u32, button_id: u32, hold_milliseconds: u64) -> Result<()> {
        ensure!(
            (100..=5_000).contains(&hold_milliseconds),
            "button hold duration must be 100..=5000 milliseconds"
        );
        let mut backend = VJoyBackend::new(device_id)?;
        backend.connect()?;
        let capabilities = backend.capabilities()?;
        ensure!(
            (1..=capabilities.button_count).contains(&button_id),
            "button {button_id} is outside the configured range 1..={}",
            capabilities.button_count
        );
        println!("Pressing virtual button {button_id}");
        backend.set_button(button_id, true)?;
        thread::sleep(Duration::from_millis(hold_milliseconds));
        backend.set_button(button_id, false)?;
        backend.disconnect()?;
        println!("Released virtual button {button_id}; all outputs cleared.");
        Ok(())
    }

    fn mirror(device_id: u32, controller_index: u32, seconds: u64) -> Result<()> {
        ensure!(
            (1..=300).contains(&seconds),
            "duration must be 1..=300 seconds"
        );
        let controllers = RawGameController::RawGameControllers()?;
        ensure!(
            controller_index < controllers.Size()?,
            "controller index is not available"
        );
        let controller = controllers.GetAt(controller_index)?;
        ensure!(
            controller.ButtonCount()? >= 2,
            "physical controller has fewer than two buttons"
        );
        let controller_id = controller.NonRoamableId()?.to_string();
        let display_name = controller.DisplayName()?.to_string();

        let backend = VJoyBackend::new(device_id)?;
        let mut pipeline = TwoButtonMirror::connect(backend)?;
        let capabilities = pipeline.capabilities()?;
        let mut previous = [false; 2];
        let started = Instant::now();
        println!(
            "Mirroring the first two buttons from {display_name} to {} for {seconds}s. Press Ctrl+C to stop safely.",
            capabilities.identity
        );

        while started.elapsed() < Duration::from_secs(seconds)
            && !STOP_REQUESTED.load(Ordering::SeqCst)
        {
            let controller = find_controller(&controller_id)?;
            let buttons = read_buttons(&controller)?;
            let current = [buttons[0], buttons[1]];
            pipeline.update(current)?;
            for index in 0..2 {
                if current[index] != previous[index] {
                    let button_id = index + 1;
                    println!(
                        "Physical button {button_id} -> virtual button {button_id} {}",
                        if current[index] {
                            "pressed"
                        } else {
                            "released"
                        }
                    );
                }
            }
            previous = current;
            thread::sleep(Duration::from_millis(5));
        }
        pipeline.disconnect()?;
        println!("All virtual buttons released; device disconnected.");
        Ok(())
    }

    fn direct_input_mirror(device_id: u32, direct_input_index: usize, seconds: u64) -> Result<()> {
        ensure!(
            (1..=300).contains(&seconds),
            "duration must be 1..=300 seconds"
        );
        let reader = DirectInputButtonReader::open(direct_input_index)?;
        ensure!(
            !reader
                .summary
                .product_name
                .to_ascii_lowercase()
                .contains("vjoy"),
            "refusing to mirror the vJoy output device back into itself"
        );
        let backend = VJoyBackend::new(device_id)?;
        let mut pipeline = TwoButtonMirror::connect(backend)?;
        let capabilities = pipeline.capabilities()?;
        let mut previous = [false; 2];
        let started = Instant::now();
        println!(
            "Mirroring the first two buttons from DirectInput [{}] {} (product {}) to {} for {seconds}s. Press Ctrl+C to stop safely.",
            reader.summary.index,
            reader.summary.name,
            reader.summary.product_name,
            capabilities.identity
        );

        while started.elapsed() < Duration::from_secs(seconds)
            && !STOP_REQUESTED.load(Ordering::SeqCst)
        {
            let current = reader.first_two_buttons()?;
            pipeline.update(current)?;
            for index in 0..2 {
                if current[index] != previous[index] {
                    let button_id = index + 1;
                    println!(
                        "Physical button {button_id} -> virtual button {button_id} {}",
                        if current[index] {
                            "pressed"
                        } else {
                            "released"
                        }
                    );
                }
            }
            previous = current;
            thread::sleep(Duration::from_millis(5));
        }
        pipeline.disconnect()?;
        println!("All virtual buttons released; device disconnected.");
        Ok(())
    }

    fn find_controller(non_roamable_id: &str) -> Result<RawGameController> {
        let controllers = RawGameController::RawGameControllers()?;
        for index in 0..controllers.Size()? {
            let controller = controllers.GetAt(index)?;
            if controller.NonRoamableId()?.to_string() == non_roamable_id {
                return Ok(controller);
            }
        }
        bail!("the selected physical controller was disconnected")
    }

    fn read_buttons(controller: &RawGameController) -> Result<Vec<bool>> {
        let mut buttons = vec![false; controller.ButtonCount()?.max(0) as usize];
        let mut switches =
            vec![GameControllerSwitchPosition::Center; controller.SwitchCount()?.max(0) as usize];
        let mut axes = vec![0.0; controller.AxisCount()?.max(0) as usize];
        controller.GetCurrentReading(&mut buttons, &mut switches, &mut axes)?;
        Ok(buttons)
    }
}

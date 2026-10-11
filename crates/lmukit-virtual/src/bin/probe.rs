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
    use lmukit_virtual::{TwoButtonMirror, VirtualInputBackend, vjoy::VJoyBackend};
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
            Some("pulse") => pulse(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u64(args.get(2), 1_500, "button hold milliseconds")?,
            ),
            Some("mirror") => mirror(
                parse_u32(args.get(1), 1, "vJoy device ID")?,
                parse_u32(args.get(2), 0, "controller index")?,
                parse_u64(args.get(3), 30, "duration seconds")?,
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
        println!("  lmukit-virtual-probe pulse [vjoy-device-id] [hold-ms]");
        println!("  lmukit-virtual-probe mirror [vjoy-device-id] [controller-index] [seconds]");
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

#[derive(Clone, Debug)]
pub struct PressedInput {
    pub vendor_id: u16,
    pub product_id: u16,
    pub button_index: usize,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::PressedInput;
    use std::collections::HashMap;
    use windows::{
        Gaming::Input::{GameControllerSwitchPosition, RawGameController},
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
    };

    pub struct InputListener {
        previous: HashMap<String, Vec<bool>>,
    }

    impl InputListener {
        pub fn new() -> Result<Self, String> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| error.to_string())?;
            let mut listener = Self {
                previous: HashMap::new(),
            };
            listener.snapshot()?;
            Ok(listener)
        }

        pub fn poll(&mut self) -> Result<Option<PressedInput>, String> {
            let controllers =
                RawGameController::RawGameControllers().map_err(|error| error.to_string())?;
            for index in 0..controllers.Size().map_err(|error| error.to_string())? {
                let controller = controllers
                    .GetAt(index)
                    .map_err(|error| error.to_string())?;
                let key = controller
                    .NonRoamableId()
                    .map_err(|error| error.to_string())?
                    .to_string();
                let buttons = read_buttons(&controller)?;
                let previous = self
                    .previous
                    .entry(key)
                    .or_insert_with(|| vec![false; buttons.len()]);
                let pressed = buttons
                    .iter()
                    .zip(previous.iter())
                    .position(|(current, old)| *current && !*old);
                *previous = buttons;
                if let Some(button_index) = pressed {
                    return Ok(Some(PressedInput {
                        vendor_id: controller
                            .HardwareVendorId()
                            .map_err(|error| error.to_string())?,
                        product_id: controller
                            .HardwareProductId()
                            .map_err(|error| error.to_string())?,
                        button_index,
                    }));
                }
            }
            Ok(None)
        }

        fn snapshot(&mut self) -> Result<(), String> {
            let controllers =
                RawGameController::RawGameControllers().map_err(|error| error.to_string())?;
            for index in 0..controllers.Size().map_err(|error| error.to_string())? {
                let controller = controllers
                    .GetAt(index)
                    .map_err(|error| error.to_string())?;
                let key = controller
                    .NonRoamableId()
                    .map_err(|error| error.to_string())?
                    .to_string();
                self.previous.insert(key, read_buttons(&controller)?);
            }
            Ok(())
        }
    }

    fn read_buttons(controller: &RawGameController) -> Result<Vec<bool>, String> {
        let mut buttons = vec![
            false;
            controller
                .ButtonCount()
                .map_err(|error| error.to_string())?
                .max(0) as usize
        ];
        let mut switches = vec![
            GameControllerSwitchPosition::Center;
            controller
                .SwitchCount()
                .map_err(|error| error.to_string())?
                .max(0) as usize
        ];
        let mut axes = vec![
            0.0;
            controller
                .AxisCount()
                .map_err(|error| error.to_string())?
                .max(0) as usize
        ];
        controller
            .GetCurrentReading(&mut buttons, &mut switches, &mut axes)
            .map_err(|error| error.to_string())?;
        Ok(buttons)
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::PressedInput;

    pub struct InputListener;

    impl InputListener {
        pub fn new() -> Result<Self, String> {
            Err("wheel input detection is available in the Windows build".into())
        }

        pub fn poll(&mut self) -> Result<Option<PressedInput>, String> {
            Ok(None)
        }
    }
}

pub use platform::InputListener;

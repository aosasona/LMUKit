#[derive(Clone, Debug)]
pub struct PressedInput {
    pub vendor_id: u16,
    pub product_id: u16,
    pub input_id: u64,
    pub control: String,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::PressedInput;
    use std::collections::HashMap;
    use windows::{
        Gaming::Input::{GameControllerSwitchPosition, RawGameController},
        Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize},
    };

    pub struct InputListener {
        previous: HashMap<String, ControllerState>,
    }

    #[derive(Clone)]
    struct ControllerState {
        buttons: Vec<bool>,
        switches: Vec<GameControllerSwitchPosition>,
        neutral_axes: Vec<f64>,
        axes: Vec<f64>,
        axis_directions: Vec<i8>,
    }

    impl InputListener {
        pub fn new() -> Result<Self, String> {
            unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.map_err(|error| error.to_string())?;
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
                let reading = read_controller(&controller)?;
                let previous = self.previous.entry(key).or_insert_with(|| reading.clone());
                let pressed_button = reading
                    .buttons
                    .iter()
                    .zip(previous.buttons.iter())
                    .position(|(current, old)| *current && !*old);
                previous.buttons = reading.buttons;

                let changed_switch = reading
                    .switches
                    .iter()
                    .zip(previous.switches.iter())
                    .enumerate()
                    .find(|(_, (current, old))| {
                        current != old && **current != GameControllerSwitchPosition::Center
                    })
                    .map(|(index, (position, _))| (index, *position));
                previous.switches = reading.switches;

                let moved_axis = reading.axes.iter().enumerate().find_map(|(index, value)| {
                    let delta = value - previous.neutral_axes[index];
                    let direction = if delta > 0.25 {
                        1
                    } else if delta < -0.25 {
                        -1
                    } else {
                        0
                    };
                    let changed = direction != 0 && direction != previous.axis_directions[index];
                    previous.axis_directions[index] = direction;
                    changed.then_some((index, direction))
                });

                let vendor_id = controller
                    .HardwareVendorId()
                    .map_err(|error| error.to_string())?;
                let product_id = controller
                    .HardwareProductId()
                    .map_err(|error| error.to_string())?;
                if let Some(button_index) = pressed_button {
                    return Ok(Some(PressedInput {
                        vendor_id,
                        product_id,
                        input_id: button_index as u64 + 32,
                        control: format!("Button {}", button_index + 1),
                    }));
                }
                if let Some((switch_index, position)) = changed_switch {
                    return Ok(Some(PressedInput {
                        vendor_id,
                        product_id,
                        input_id: 16 + switch_index as u64 * 8 + position.0 as u64 - 1,
                        control: format!("POV {} direction {}", switch_index + 1, position.0),
                    }));
                }
                if let Some((axis_index, direction)) = moved_axis {
                    return Ok(Some(PressedInput {
                        vendor_id,
                        product_id,
                        input_id: axis_index as u64 * 2 + u64::from(direction < 0),
                        control: format!(
                            "Axis {} {}",
                            axis_index + 1,
                            if direction > 0 { "+" } else { "−" }
                        ),
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
                self.previous.insert(key, read_controller(&controller)?);
            }
            Ok(())
        }
    }

    fn read_controller(controller: &RawGameController) -> Result<ControllerState, String> {
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
        let axis_directions = vec![0; axes.len()];
        Ok(ControllerState {
            buttons,
            switches,
            neutral_axes: axes.clone(),
            axes,
            axis_directions,
        })
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

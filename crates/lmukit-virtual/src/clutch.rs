use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};

use crate::PhysicalButton;

/// Stable identity for an axis exposed by a physical DirectInput device.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysicalAxis {
    device_id: String,
    axis_id: u32,
}

impl PhysicalAxis {
    pub fn new(device_id: impl Into<String>, axis_id: u32) -> Result<Self> {
        let device_id = device_id.into();
        ensure!(
            !device_id.trim().is_empty(),
            "physical device ID must not be empty"
        );
        ensure!(axis_id > 0, "physical axis ID must be greater than zero");
        Ok(Self { device_id, axis_id })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn axis_id(&self) -> u32 {
        self.axis_id
    }
}

/// User calibration for a one-direction physical control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisCalibration {
    pub minimum: i32,
    pub maximum: i32,
    pub inverted: bool,
    pub released_deadzone: f64,
    pub engaged_deadzone: f64,
}

impl AxisCalibration {
    pub fn new(
        minimum: i32,
        maximum: i32,
        inverted: bool,
        released_deadzone: f64,
        engaged_deadzone: f64,
    ) -> Result<Self> {
        ensure!(
            minimum < maximum,
            "axis calibration minimum must be below maximum"
        );
        ensure!(
            (0.0..0.5).contains(&released_deadzone),
            "released deadzone must be 0%..<50%"
        );
        ensure!(
            (0.0..0.5).contains(&engaged_deadzone),
            "engaged deadzone must be 0%..<50%"
        );
        ensure!(
            released_deadzone + engaged_deadzone < 1.0,
            "axis deadzones consume the whole range"
        );
        Ok(Self {
            minimum,
            maximum,
            inverted,
            released_deadzone,
            engaged_deadzone,
        })
    }

    pub fn normalize(&self, value: i32) -> f64 {
        let mut value = (f64::from(value) - f64::from(self.minimum))
            / (f64::from(self.maximum) - f64::from(self.minimum));
        value = value.clamp(0.0, 1.0);
        if self.inverted {
            value = 1.0 - value;
        }
        if value <= self.released_deadzone {
            return 0.0;
        }
        if value >= 1.0 - self.engaged_deadzone {
            return 1.0;
        }
        (value - self.released_deadzone) / (1.0 - self.released_deadzone - self.engaged_deadzone)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClutchInputKind {
    Button(PhysicalButton),
    Axis(PhysicalAxis),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClutchInput {
    pub source: ClutchInputKind,
    pub calibration: Option<AxisCalibration>,
}

impl ClutchInput {
    pub fn button(source: PhysicalButton) -> Self {
        Self {
            source: ClutchInputKind::Button(source),
            calibration: None,
        }
    }
    pub fn axis(source: PhysicalAxis, calibration: AxisCalibration) -> Self {
        Self {
            source: ClutchInputKind::Axis(source),
            calibration: Some(calibration),
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                (&self.source, self.calibration),
                (ClutchInputKind::Button(_), None) | (ClutchInputKind::Axis(_), Some(_))
            ),
            "axis clutch inputs require calibration and button inputs must not have it"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClutchMode {
    Direct {
        input: ClutchInput,
    },
    Dual {
        full: ClutchInput,
        bite: ClutchInput,
        bite_point: f64,
    },
    Combined {
        input: ClutchInput,
        firmware_bite_point: Option<f64>,
        bite_point: f64,
    },
}

impl ClutchMode {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Direct { input } => input.validate(),
            Self::Dual {
                full,
                bite,
                bite_point,
            } => {
                full.validate()?;
                bite.validate()?;
                ensure!(
                    full.source != bite.source,
                    "dual-clutch inputs must be independent"
                );
                validate_percentage(*bite_point, "bite point")
            }
            Self::Combined {
                input,
                firmware_bite_point,
                bite_point,
            } => {
                input.validate()?;
                ensure!(
                    matches!(input.source, ClutchInputKind::Axis(_)),
                    "combined clutch mode requires an analog axis"
                );
                validate_percentage(*bite_point, "bite point")?;
                if let Some(value) = firmware_bite_point {
                    validate_percentage(*value, "firmware bite point")?;
                }
                Ok(())
            }
        }
    }
}

fn validate_percentage(value: f64, name: &str) -> Result<()> {
    ensure!(
        (0.0..=1.0).contains(&value),
        "{name} must be between 0% and 100%"
    );
    Ok(())
}

/// Deterministic profile-local clutch processor. It owns only transient input
/// state; callers retain responsibility for writing its 0..=1 output to vJoy.
pub struct ClutchProcessor {
    mode: ClutchMode,
    pressed: BTreeSet<PhysicalButton>,
    axes: BTreeMap<PhysicalAxis, i32>,
}

impl ClutchProcessor {
    pub fn new(mode: ClutchMode) -> Result<Self> {
        mode.validate()?;
        Ok(Self {
            mode,
            pressed: BTreeSet::new(),
            axes: BTreeMap::new(),
        })
    }

    pub fn handle_button(&mut self, source: &PhysicalButton, pressed: bool) -> Option<f64> {
        if !self.references_button(source) {
            return None;
        }
        if pressed {
            self.pressed.insert(source.clone());
        } else {
            self.pressed.remove(source);
        }
        Some(self.output())
    }

    pub fn handle_axis(&mut self, source: &PhysicalAxis, value: i32) -> Option<f64> {
        if !self.references_axis(source) {
            return None;
        }
        self.axes.insert(source.clone(), value);
        Some(self.output())
    }

    pub fn disconnect_device(&mut self, device_id: &str) -> Option<f64> {
        let referenced = self.inputs().any(|input| match &input.source {
            ClutchInputKind::Button(source) => source.device_id() == device_id,
            ClutchInputKind::Axis(source) => source.device_id() == device_id,
        });
        if !referenced {
            return None;
        }
        self.pressed
            .retain(|source| source.device_id() != device_id);
        self.axes
            .retain(|source, _| source.device_id() != device_id);
        Some(self.output())
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
        self.axes.clear();
    }
    pub fn output(&self) -> f64 {
        match &self.mode {
            ClutchMode::Direct { input } => self.input_value(input),
            ClutchMode::Dual {
                full,
                bite,
                bite_point,
            } => self
                .input_value(full)
                .max(self.input_value(bite) * bite_point),
            ClutchMode::Combined {
                input,
                firmware_bite_point,
                bite_point,
            } => {
                let value = self.input_value(input);
                firmware_bite_point
                    .map_or(value, |firmware| remap_bite(value, firmware, *bite_point))
            }
        }
    }

    /// Current calibrated physical travels. The second value is present only
    /// for independent dual-clutch mode.
    pub fn input_values(&self) -> (f64, Option<f64>) {
        match &self.mode {
            ClutchMode::Direct { input } | ClutchMode::Combined { input, .. } => {
                (self.input_value(input), None)
            }
            ClutchMode::Dual { full, bite, .. } => {
                (self.input_value(full), Some(self.input_value(bite)))
            }
        }
    }

    fn input_value(&self, input: &ClutchInput) -> f64 {
        match &input.source {
            ClutchInputKind::Button(source) => f64::from(self.pressed.contains(source)),
            ClutchInputKind::Axis(source) => input
                .calibration
                .expect("validated analog input has calibration")
                .normalize(
                    self.axes
                        .get(source)
                        .copied()
                        .unwrap_or(input.calibration.unwrap().minimum),
                ),
        }
    }

    fn inputs(&self) -> Box<dyn Iterator<Item = &ClutchInput> + '_> {
        match &self.mode {
            ClutchMode::Direct { input } | ClutchMode::Combined { input, .. } => {
                Box::new(std::iter::once(input))
            }
            ClutchMode::Dual { full, bite, .. } => Box::new([full, bite].into_iter()),
        }
    }

    fn references_button(&self, source: &PhysicalButton) -> bool {
        self.inputs().any(|input| matches!(&input.source, ClutchInputKind::Button(candidate) if candidate == source))
    }
    fn references_axis(&self, source: &PhysicalAxis) -> bool {
        self.inputs().any(|input| matches!(&input.source, ClutchInputKind::Axis(candidate) if candidate == source))
    }
}

fn remap_bite(value: f64, firmware: f64, configured: f64) -> f64 {
    if firmware <= f64::EPSILON || firmware >= 1.0 - f64::EPSILON {
        return value;
    }
    if value <= firmware {
        value / firmware * configured
    } else {
        configured + (value - firmware) / (1.0 - firmware) * (1.0 - configured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn axis(id: u32) -> PhysicalAxis {
        PhysicalAxis::new("wheel", id).unwrap()
    }
    fn button(id: u32) -> PhysicalButton {
        PhysicalButton::new("wheel", id).unwrap()
    }
    fn calibration(inverted: bool) -> AxisCalibration {
        AxisCalibration::new(100, 1100, inverted, 0.0, 0.0).unwrap()
    }

    #[test]
    fn direct_axis_preserves_partial_travel_and_polarity() {
        let source = axis(1);
        let mut normal = ClutchProcessor::new(ClutchMode::Direct {
            input: ClutchInput::axis(source.clone(), calibration(false)),
        })
        .unwrap();
        assert_eq!(normal.handle_axis(&source, 600), Some(0.5));
        let mut inverted = ClutchProcessor::new(ClutchMode::Direct {
            input: ClutchInput::axis(source.clone(), calibration(true)),
        })
        .unwrap();
        assert_eq!(inverted.handle_axis(&source, 350), Some(0.75));
    }

    #[test]
    fn digital_dual_clutch_uses_bite_point_and_full_priority() {
        let full = button(1);
        let bite = button(2);
        let mut clutch = ClutchProcessor::new(ClutchMode::Dual {
            full: ClutchInput::button(full.clone()),
            bite: ClutchInput::button(bite.clone()),
            bite_point: 0.42,
        })
        .unwrap();
        assert_eq!(clutch.handle_button(&bite, true), Some(0.42));
        assert_eq!(clutch.handle_button(&full, true), Some(1.0));
        assert_eq!(clutch.handle_button(&full, false), Some(0.42));
        assert_eq!(clutch.handle_button(&bite, false), Some(0.0));
    }

    #[test]
    fn analog_dual_clutch_combines_progressive_contributions() {
        let full = axis(1);
        let bite = axis(2);
        let mut clutch = ClutchProcessor::new(ClutchMode::Dual {
            full: ClutchInput::axis(full.clone(), calibration(false)),
            bite: ClutchInput::axis(bite.clone(), calibration(false)),
            bite_point: 0.5,
        })
        .unwrap();
        assert_eq!(clutch.handle_axis(&bite, 1100), Some(0.5));
        assert_eq!(clutch.handle_axis(&full, 500), Some(0.5));
        assert_eq!(clutch.handle_axis(&full, 850), Some(0.75));
    }

    #[test]
    fn combined_axis_remaps_only_the_calibrated_firmware_bite_curve() {
        let source = axis(1);
        let mut clutch = ClutchProcessor::new(ClutchMode::Combined {
            input: ClutchInput::axis(source.clone(), calibration(false)),
            firmware_bite_point: Some(0.4),
            bite_point: 0.6,
        })
        .unwrap();
        assert!((clutch.handle_axis(&source, 500).unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(clutch.handle_axis(&source, 100), Some(0.0));
        assert_eq!(clutch.handle_axis(&source, 1100), Some(1.0));
    }

    #[test]
    fn disconnect_releases_only_referenced_device_state() {
        let full = button(1);
        let bite = button(2);
        let mut clutch = ClutchProcessor::new(ClutchMode::Dual {
            full: ClutchInput::button(full),
            bite: ClutchInput::button(bite.clone()),
            bite_point: 0.5,
        })
        .unwrap();
        clutch.handle_button(&bite, true);
        assert_eq!(clutch.disconnect_device("wheel"), Some(0.0));
        assert_eq!(clutch.disconnect_device("other"), None);
    }
}

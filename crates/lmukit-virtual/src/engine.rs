use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};

use crate::{VirtualCapabilities, VirtualInputBackend};

/// Stable semantic action shared by every virtual profile and wheel.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LogicalAction(String);

impl LogicalAction {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(!value.trim().is_empty(), "logical action must not be empty");
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque physical button identity supplied by a platform input adapter.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysicalButton {
    device_id: String,
    button_id: u32,
}

impl PhysicalButton {
    pub fn new(device_id: impl Into<String>, button_id: u32) -> Result<Self> {
        let device_id = device_id.into();
        ensure!(
            !device_id.trim().is_empty(),
            "physical device ID must not be empty"
        );
        ensure!(
            button_id > 0,
            "physical button ID must be greater than zero"
        );
        Ok(Self {
            device_id,
            button_id,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn button_id(&self) -> u32 {
        self.button_id
    }
}

/// Global logical-action to virtual-button assignment.
///
/// Both logical actions and virtual buttons are unique. Profiles reference the
/// actions and therefore cannot reinterpret the stable virtual layout.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VirtualLayout {
    outputs: BTreeMap<LogicalAction, u32>,
}

impl VirtualLayout {
    pub fn new(entries: impl IntoIterator<Item = (LogicalAction, u32)>) -> Result<Self> {
        let mut outputs = BTreeMap::new();
        let mut assigned_buttons = BTreeSet::new();
        for (action, button_id) in entries {
            ensure!(button_id > 0, "virtual button ID must be greater than zero");
            ensure!(
                !outputs.contains_key(&action),
                "logical action '{}' is assigned more than once",
                action.as_str()
            );
            ensure!(
                assigned_buttons.insert(button_id),
                "virtual button {button_id} is assigned more than once"
            );
            outputs.insert(action, button_id);
        }
        Ok(Self { outputs })
    }

    pub fn button_for(&self, action: &LogicalAction) -> Option<u32> {
        self.outputs.get(action).copied()
    }

    fn validate_capabilities(&self, capabilities: &VirtualCapabilities) -> Result<()> {
        if let Some(button_id) = self
            .outputs
            .values()
            .find(|button_id| **button_id > capabilities.button_count)
        {
            anyhow::bail!(
                "virtual layout button {button_id} exceeds the device's {} buttons",
                capabilities.button_count
            );
        }
        Ok(())
    }
}

/// Physical-button to logical-action mapping owned by one profile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfileMapping {
    actions: BTreeMap<PhysicalButton, LogicalAction>,
}

impl ProfileMapping {
    pub fn new(entries: impl IntoIterator<Item = (PhysicalButton, LogicalAction)>) -> Result<Self> {
        let mut actions = BTreeMap::new();
        for (source, action) in entries {
            ensure!(
                !actions.contains_key(&source),
                "physical button {} on '{}' is mapped more than once",
                source.button_id(),
                source.device_id()
            );
            actions.insert(source, action);
        }
        Ok(Self { actions })
    }

    pub fn action_for(&self, source: &PhysicalButton) -> Option<&LogicalAction> {
        self.actions.get(source)
    }

    fn validate_layout(&self, layout: &VirtualLayout) -> Result<()> {
        for action in self.actions.values() {
            ensure!(
                layout.button_for(action).is_some(),
                "profile action '{}' is missing from the virtual layout",
                action.as_str()
            );
        }
        Ok(())
    }
}

/// Button-only physical-to-virtual mapping engine.
///
/// Active physical sources own their virtual outputs. Several sources may hold
/// one output simultaneously; the output is released only after its last owner
/// releases it. Profile changes release every output before accepting new
/// input, preventing held controls from leaking across profiles.
pub struct MappingEngine<B: VirtualInputBackend> {
    backend: B,
    layout: VirtualLayout,
    profile: ProfileMapping,
    active_sources: BTreeMap<PhysicalButton, u32>,
    output_owners: BTreeMap<u32, BTreeSet<PhysicalButton>>,
    connected: bool,
}

impl<B: VirtualInputBackend> MappingEngine<B> {
    pub fn connect(mut backend: B, layout: VirtualLayout, profile: ProfileMapping) -> Result<Self> {
        profile.validate_layout(&layout)?;
        backend.connect()?;
        let mut initialise = || -> Result<()> {
            let capabilities = backend.capabilities()?;
            layout.validate_capabilities(&capabilities)?;
            backend.release_all()?;
            Ok(())
        };
        if let Err(error) = initialise() {
            let _ = backend.disconnect();
            return Err(error);
        }

        Ok(Self {
            backend,
            layout,
            profile,
            active_sources: BTreeMap::new(),
            output_owners: BTreeMap::new(),
            connected: true,
        })
    }

    pub fn capabilities(&self) -> Result<VirtualCapabilities> {
        self.backend.capabilities()
    }

    /// Returns true when the physical event belongs to the active profile.
    pub fn handle_button(&mut self, source: &PhysicalButton, pressed: bool) -> Result<bool> {
        ensure!(self.connected, "mapping engine is disconnected");
        if pressed {
            self.press(source)
        } else {
            self.release(source)
        }
    }

    pub fn set_profile(&mut self, profile: ProfileMapping) -> Result<()> {
        ensure!(self.connected, "mapping engine is disconnected");
        profile.validate_layout(&self.layout)?;
        self.backend.release_all()?;
        self.active_sources.clear();
        self.output_owners.clear();
        self.profile = profile;
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        ensure!(self.connected, "mapping engine is disconnected");
        self.backend.disconnect()?;
        self.active_sources.clear();
        self.output_owners.clear();
        self.connected = false;
        Ok(())
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    fn press(&mut self, source: &PhysicalButton) -> Result<bool> {
        if self.active_sources.contains_key(source) {
            return Ok(true);
        }
        let Some(action) = self.profile.action_for(source) else {
            return Ok(false);
        };
        let button_id = self
            .layout
            .button_for(action)
            .expect("profile mappings are validated against the layout");
        let first_owner = self
            .output_owners
            .get(&button_id)
            .is_none_or(BTreeSet::is_empty);
        if first_owner {
            self.backend.set_button(button_id, true)?;
        }
        self.output_owners
            .entry(button_id)
            .or_default()
            .insert(source.clone());
        self.active_sources.insert(source.clone(), button_id);
        Ok(true)
    }

    fn release(&mut self, source: &PhysicalButton) -> Result<bool> {
        let Some(button_id) = self.active_sources.get(source).copied() else {
            return Ok(false);
        };
        let last_owner = self
            .output_owners
            .get(&button_id)
            .is_some_and(|owners| owners.len() == 1 && owners.contains(source));
        if last_owner {
            self.backend.set_button(button_id, false)?;
        }

        self.active_sources.remove(source);
        if let Some(owners) = self.output_owners.get_mut(&button_id) {
            owners.remove(source);
            if owners.is_empty() {
                self.output_owners.remove(&button_id);
            }
        }
        Ok(true)
    }
}

impl<B: VirtualInputBackend> Drop for MappingEngine<B> {
    fn drop(&mut self) {
        if self.connected {
            let _ = self.backend.disconnect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogicalAction, MappingEngine, PhysicalButton, ProfileMapping, VirtualLayout};
    use crate::MockVirtualBackend;

    fn action(name: &str) -> LogicalAction {
        LogicalAction::new(name).unwrap()
    }

    fn button(device: &str, button_id: u32) -> PhysicalButton {
        PhysicalButton::new(device, button_id).unwrap()
    }

    fn layout() -> VirtualLayout {
        VirtualLayout::new([(action("pit_limiter"), 1), (action("headlights"), 2)]).unwrap()
    }

    fn profile(entries: &[(&str, u32, &str)]) -> ProfileMapping {
        ProfileMapping::new(
            entries
                .iter()
                .map(|(device, button_id, name)| (button(device, *button_id), action(name))),
        )
        .unwrap()
    }

    #[test]
    fn routes_press_and_release_through_the_stable_layout() {
        let source = button("gt-neo", 5);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter")]),
        )
        .unwrap();

        engine.handle_button(&source, true).unwrap();
        engine.handle_button(&source, false).unwrap();

        assert_eq!(engine.backend().events(), [(1, true), (1, false)]);
    }

    #[test]
    fn ignores_duplicate_states_and_unassigned_controls() {
        let mapped = button("gt-neo", 5);
        let unassigned = button("gt-neo", 6);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter")]),
        )
        .unwrap();

        engine.handle_button(&mapped, true).unwrap();
        engine.handle_button(&mapped, true).unwrap();
        engine.handle_button(&unassigned, true).unwrap();
        engine.handle_button(&unassigned, false).unwrap();
        engine.handle_button(&mapped, false).unwrap();
        engine.handle_button(&mapped, false).unwrap();

        assert_eq!(engine.backend().events(), [(1, true), (1, false)]);
    }

    #[test]
    fn keeps_an_output_held_until_its_last_source_releases() {
        let first = button("gt-neo", 5);
        let second = button("gt-neo", 6);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter"), ("gt-neo", 6, "pit_limiter")]),
        )
        .unwrap();

        engine.handle_button(&first, true).unwrap();
        engine.handle_button(&second, true).unwrap();
        engine.handle_button(&first, false).unwrap();
        assert_eq!(engine.backend().button(1), Some(true));
        engine.handle_button(&second, false).unwrap();

        assert_eq!(engine.backend().events(), [(1, true), (1, false)]);
    }

    #[test]
    fn profile_change_releases_held_outputs_before_using_new_mappings() {
        let source = button("gt-neo", 5);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter")]),
        )
        .unwrap();
        engine.handle_button(&source, true).unwrap();

        engine
            .set_profile(profile(&[("gt-neo", 5, "headlights")]))
            .unwrap();
        engine.handle_button(&source, false).unwrap();
        engine.handle_button(&source, true).unwrap();

        assert_eq!(
            engine.backend().events(),
            [(1, true), (1, false), (2, true)]
        );
    }

    #[test]
    fn invalid_profile_change_preserves_the_current_mapping_and_state() {
        let source = button("gt-neo", 5);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter")]),
        )
        .unwrap();
        engine.handle_button(&source, true).unwrap();
        let invalid = profile(&[("gt-neo", 5, "unknown_action")]);

        assert!(engine.set_profile(invalid).is_err());
        assert_eq!(engine.backend().button(1), Some(true));
        engine.handle_button(&source, false).unwrap();

        assert_eq!(engine.backend().events(), [(1, true), (1, false)]);
    }

    #[test]
    fn rejects_layout_buttons_outside_backend_capabilities() {
        let layout = VirtualLayout::new([(action("pit_limiter"), 9)]).unwrap();

        let result = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout,
            profile(&[("gt-neo", 5, "pit_limiter")]),
        );

        assert!(result.is_err());
    }

    #[test]
    fn disconnect_releases_every_held_output() {
        let source = button("gt-neo", 5);
        let mut engine = MappingEngine::connect(
            MockVirtualBackend::new(8),
            layout(),
            profile(&[("gt-neo", 5, "pit_limiter")]),
        )
        .unwrap();
        engine.handle_button(&source, true).unwrap();

        engine.disconnect().unwrap();

        assert_eq!(engine.backend().events(), [(1, true), (1, false)]);
    }
}

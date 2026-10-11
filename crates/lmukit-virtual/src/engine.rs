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

/// One physical trigger plus optional hold modifiers owned by a profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileBinding {
    trigger: PhysicalButton,
    modifiers: BTreeSet<PhysicalButton>,
    action: LogicalAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncoderMode {
    name: String,
    clockwise_action: LogicalAction,
    counterclockwise_action: LogicalAction,
}

impl EncoderMode {
    pub fn new(
        name: impl Into<String>,
        clockwise_action: LogicalAction,
        counterclockwise_action: LogicalAction,
    ) -> Result<Self> {
        let name = name.into();
        ensure!(
            !name.trim().is_empty(),
            "encoder mode name must not be empty"
        );
        Ok(Self {
            name,
            clockwise_action,
            counterclockwise_action,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncoderGroup {
    name: String,
    clockwise: PhysicalButton,
    counterclockwise: PhysicalButton,
    cycle: Option<PhysicalButton>,
    modes: Vec<EncoderMode>,
}

impl EncoderGroup {
    pub fn new(
        name: impl Into<String>,
        clockwise: PhysicalButton,
        counterclockwise: PhysicalButton,
        cycle: Option<PhysicalButton>,
        modes: Vec<EncoderMode>,
    ) -> Result<Self> {
        let name = name.into();
        ensure!(!name.trim().is_empty(), "encoder name must not be empty");
        ensure!(!modes.is_empty(), "encoder must have at least one mode");
        ensure!(
            clockwise != counterclockwise,
            "encoder directions must differ"
        );
        if let Some(cycle) = &cycle {
            ensure!(
                &clockwise != cycle && &counterclockwise != cycle,
                "encoder directions and mode control must use different physical buttons"
            );
        }
        ensure!(
            modes.len() == 1 || cycle.is_some(),
            "an encoder with multiple modes requires a mode control"
        );
        let mut mode_names = BTreeSet::new();
        for mode in &modes {
            ensure!(
                mode_names.insert(mode.name.to_lowercase()),
                "encoder mode names must be unique"
            );
        }
        Ok(Self {
            name,
            clockwise,
            counterclockwise,
            cycle,
            modes,
        })
    }
}

impl ProfileBinding {
    pub fn new(
        trigger: PhysicalButton,
        modifiers: impl IntoIterator<Item = PhysicalButton>,
        action: LogicalAction,
    ) -> Result<Self> {
        let modifiers = modifiers.into_iter().collect::<BTreeSet<_>>();
        ensure!(
            !modifiers.contains(&trigger),
            "a physical button cannot modify itself"
        );
        Ok(Self {
            trigger,
            modifiers,
            action,
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BindingInput {
    trigger: PhysicalButton,
    modifiers: BTreeSet<PhysicalButton>,
}

/// Physical chord to logical-action mapping owned by one profile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfileMapping {
    actions: BTreeMap<BindingInput, LogicalAction>,
    encoder_groups: BTreeMap<String, EncoderGroup>,
}

impl ProfileMapping {
    pub fn new(entries: impl IntoIterator<Item = (PhysicalButton, LogicalAction)>) -> Result<Self> {
        Self::from_bindings(
            entries
                .into_iter()
                .map(|(trigger, action)| ProfileBinding::new(trigger, [], action))
                .collect::<Result<Vec<_>>>()?,
        )
    }

    pub fn from_bindings(entries: impl IntoIterator<Item = ProfileBinding>) -> Result<Self> {
        let mut actions = BTreeMap::new();
        for binding in entries {
            let input = BindingInput {
                trigger: binding.trigger,
                modifiers: binding.modifiers,
            };
            ensure!(
                !actions.contains_key(&input),
                "physical chord ending with button {} on '{}' is mapped more than once",
                input.trigger.button_id(),
                input.trigger.device_id()
            );
            actions.insert(input, binding.action);
        }
        Ok(Self {
            actions,
            encoder_groups: BTreeMap::new(),
        })
    }

    pub fn with_encoder_groups(
        mut self,
        groups: impl IntoIterator<Item = EncoderGroup>,
    ) -> Result<Self> {
        let mut encoder_inputs = BTreeSet::new();
        for group in groups {
            let key = group.name.to_lowercase();
            ensure!(
                !self.encoder_groups.contains_key(&key),
                "encoder names must be unique"
            );
            for source in [&group.clockwise, &group.counterclockwise]
                .into_iter()
                .chain(group.cycle.iter())
            {
                ensure!(
                    !self.actions.keys().any(|input| {
                        &input.trigger == source || input.modifiers.contains(source)
                    }),
                    "encoder controls cannot also be regular profile controls"
                );
                ensure!(
                    encoder_inputs.insert(source.clone()),
                    "encoder controls cannot be shared between encoders"
                );
            }
            self.encoder_groups.insert(key, group);
        }
        Ok(self)
    }

    fn references(&self, source: &PhysicalButton) -> bool {
        self.actions
            .keys()
            .any(|input| &input.trigger == source || input.modifiers.contains(source))
            || self.encoder_groups.values().any(|group| {
                &group.clockwise == source
                    || &group.counterclockwise == source
                    || group.cycle.as_ref() == Some(source)
            })
    }

    fn active_bindings(
        &self,
        pressed: &BTreeSet<PhysicalButton>,
        encoder_modes: &BTreeMap<String, usize>,
    ) -> BTreeMap<BindingInput, LogicalAction> {
        let mut selected = BTreeMap::<PhysicalButton, (BindingInput, LogicalAction)>::new();
        for (input, action) in &self.actions {
            if !pressed.contains(&input.trigger)
                || !input
                    .modifiers
                    .iter()
                    .all(|modifier| pressed.contains(modifier))
            {
                continue;
            }
            let replace = selected.get(&input.trigger).is_none_or(|(current, _)| {
                input.modifiers.len() > current.modifiers.len()
                    || (input.modifiers.len() == current.modifiers.len() && input > current)
            });
            if replace {
                selected.insert(input.trigger.clone(), (input.clone(), action.clone()));
            }
        }
        for (key, group) in &self.encoder_groups {
            let mode_index = encoder_modes.get(key).copied().unwrap_or(0);
            let mode = &group.modes[mode_index % group.modes.len()];
            for (trigger, action) in [
                (&group.clockwise, &mode.clockwise_action),
                (&group.counterclockwise, &mode.counterclockwise_action),
            ] {
                if pressed.contains(trigger) {
                    let input = BindingInput {
                        trigger: trigger.clone(),
                        modifiers: BTreeSet::new(),
                    };
                    selected.insert(trigger.clone(), (input, action.clone()));
                }
            }
        }
        selected.into_values().collect()
    }

    fn validate_layout(&self, layout: &VirtualLayout) -> Result<()> {
        for action in self.actions.values() {
            ensure!(
                layout.button_for(action).is_some(),
                "profile action '{}' is missing from the virtual layout",
                action.as_str()
            );
        }
        for mode in self
            .encoder_groups
            .values()
            .flat_map(|group| group.modes.iter())
        {
            for action in [&mode.clockwise_action, &mode.counterclockwise_action] {
                ensure!(
                    layout.button_for(action).is_some(),
                    "encoder action '{}' is missing from the virtual layout",
                    action.as_str()
                );
            }
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
    pressed_sources: BTreeSet<PhysicalButton>,
    active_bindings: BTreeMap<BindingInput, u32>,
    output_owners: BTreeMap<u32, BTreeSet<BindingInput>>,
    encoder_modes: BTreeMap<String, usize>,
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
            pressed_sources: BTreeSet::new(),
            active_bindings: BTreeMap::new(),
            output_owners: BTreeMap::new(),
            encoder_modes: BTreeMap::new(),
            connected: true,
        })
    }

    pub fn capabilities(&self) -> Result<VirtualCapabilities> {
        self.backend.capabilities()
    }

    /// Returns true when the physical event belongs to the active profile.
    pub fn handle_button(&mut self, source: &PhysicalButton, pressed: bool) -> Result<bool> {
        ensure!(self.connected, "mapping engine is disconnected");
        let mapped = self.profile.references(source);
        let changed = if pressed {
            self.pressed_sources.insert(source.clone())
        } else {
            self.pressed_sources.remove(source)
        };
        if changed && pressed {
            for (key, group) in &self.profile.encoder_groups {
                if group.cycle.as_ref() == Some(source) {
                    let current = self.encoder_modes.entry(key.clone()).or_default();
                    *current = (*current + 1) % group.modes.len();
                }
            }
        }
        if changed {
            self.reconcile()?;
        }
        Ok(mapped)
    }

    pub fn set_profile(&mut self, profile: ProfileMapping) -> Result<()> {
        ensure!(self.connected, "mapping engine is disconnected");
        profile.validate_layout(&self.layout)?;
        self.backend.release_all()?;
        self.pressed_sources.clear();
        self.active_bindings.clear();
        self.output_owners.clear();
        self.encoder_modes.clear();
        self.profile = profile;
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        ensure!(self.connected, "mapping engine is disconnected");
        self.backend.disconnect()?;
        self.pressed_sources.clear();
        self.active_bindings.clear();
        self.output_owners.clear();
        self.encoder_modes.clear();
        self.connected = false;
        Ok(())
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn active_encoder_modes(&self) -> Vec<(&str, &str)> {
        self.profile
            .encoder_groups
            .iter()
            .map(|(key, group)| {
                let index = self.encoder_modes.get(key).copied().unwrap_or(0);
                (group.name.as_str(), group.modes[index].name.as_str())
            })
            .collect()
    }

    fn reconcile(&mut self) -> Result<()> {
        let desired_bindings = self
            .profile
            .active_bindings(&self.pressed_sources, &self.encoder_modes)
            .into_iter()
            .map(|(input, action)| {
                let button = self
                    .layout
                    .button_for(&action)
                    .expect("profile mappings are validated against the layout");
                (input, button)
            })
            .collect::<BTreeMap<_, _>>();
        let mut desired_owners = BTreeMap::<u32, BTreeSet<BindingInput>>::new();
        for (input, button) in &desired_bindings {
            desired_owners
                .entry(*button)
                .or_default()
                .insert(input.clone());
        }
        for button in self.output_owners.keys() {
            if !desired_owners.contains_key(button) {
                self.backend.set_button(*button, false)?;
            }
        }
        for button in desired_owners.keys() {
            if !self.output_owners.contains_key(button) {
                self.backend.set_button(*button, true)?;
            }
        }
        self.active_bindings = desired_bindings;
        self.output_owners = desired_owners;
        Ok(())
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
    use super::{
        EncoderGroup, EncoderMode, LogicalAction, MappingEngine, PhysicalButton, ProfileBinding,
        ProfileMapping, VirtualLayout,
    };
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

    #[test]
    fn held_modifier_selects_the_more_specific_mapping() {
        let modifier = button("gt-neo", 4);
        let trigger = button("gt-neo", 5);
        let mapping = ProfileMapping::from_bindings([
            ProfileBinding::new(trigger.clone(), [], action("pit_limiter")).unwrap(),
            ProfileBinding::new(trigger.clone(), [modifier.clone()], action("headlights")).unwrap(),
        ])
        .unwrap();
        let mut engine =
            MappingEngine::connect(MockVirtualBackend::new(8), layout(), mapping).unwrap();

        engine.handle_button(&modifier, true).unwrap();
        engine.handle_button(&trigger, true).unwrap();
        engine.handle_button(&trigger, false).unwrap();
        engine.handle_button(&modifier, false).unwrap();

        assert_eq!(engine.backend().events(), [(2, true), (2, false)]);
    }

    #[test]
    fn modifier_changes_reconcile_a_held_trigger_without_stuck_outputs() {
        let modifier = button("gt-neo", 4);
        let trigger = button("gt-neo", 5);
        let mapping = ProfileMapping::from_bindings([
            ProfileBinding::new(trigger.clone(), [], action("pit_limiter")).unwrap(),
            ProfileBinding::new(trigger.clone(), [modifier.clone()], action("headlights")).unwrap(),
        ])
        .unwrap();
        let mut engine =
            MappingEngine::connect(MockVirtualBackend::new(8), layout(), mapping).unwrap();

        engine.handle_button(&trigger, true).unwrap();
        engine.handle_button(&modifier, true).unwrap();
        engine.handle_button(&modifier, false).unwrap();
        engine.handle_button(&trigger, false).unwrap();

        assert_eq!(
            engine.backend().events(),
            [
                (1, true),
                (1, false),
                (2, true),
                (2, false),
                (1, true),
                (1, false)
            ]
        );
    }

    #[test]
    fn rejects_a_trigger_used_as_its_own_modifier() {
        let trigger = button("gt-neo", 5);
        assert!(ProfileBinding::new(trigger.clone(), [trigger], action("pit_limiter")).is_err());
    }

    #[test]
    fn cycles_encoder_modes_without_emitting_the_mode_control() {
        let encoder_layout = VirtualLayout::new([
            (action("front_arb_increase"), 1),
            (action("front_arb_decrease"), 2),
            (action("tc2_increase"), 3),
            (action("tc2_decrease"), 4),
        ])
        .unwrap();
        let clockwise = button("gt-neo", 10);
        let counterclockwise = button("gt-neo", 11);
        let cycle = button("gt-neo", 12);
        let mapping = ProfileMapping::default()
            .with_encoder_groups([EncoderGroup::new(
                "Encoder 1",
                clockwise.clone(),
                counterclockwise,
                Some(cycle.clone()),
                vec![
                    EncoderMode::new(
                        "Front ARB",
                        action("front_arb_increase"),
                        action("front_arb_decrease"),
                    )
                    .unwrap(),
                    EncoderMode::new("TC2", action("tc2_increase"), action("tc2_decrease"))
                        .unwrap(),
                ],
            )
            .unwrap()])
            .unwrap();
        let mut engine =
            MappingEngine::connect(MockVirtualBackend::new(8), encoder_layout, mapping).unwrap();

        engine.handle_button(&clockwise, true).unwrap();
        engine.handle_button(&clockwise, false).unwrap();
        engine.handle_button(&cycle, true).unwrap();
        engine.handle_button(&cycle, true).unwrap();
        engine.handle_button(&cycle, false).unwrap();
        engine.handle_button(&clockwise, true).unwrap();
        engine.handle_button(&clockwise, false).unwrap();

        assert_eq!(
            engine.backend().events(),
            [(1, true), (1, false), (3, true), (3, false)]
        );
        assert_eq!(engine.active_encoder_modes(), [("Encoder 1", "TC2")]);
    }

    #[test]
    fn routes_a_single_encoder_mode_without_a_mode_control() {
        let encoder_layout = VirtualLayout::new([
            (action("brake_bias_increase"), 1),
            (action("brake_bias_decrease"), 2),
        ])
        .unwrap();
        let clockwise = button("gt-neo", 10);
        let counterclockwise = button("gt-neo", 11);
        let mapping = ProfileMapping::default()
            .with_encoder_groups([EncoderGroup::new(
                "Brake bias",
                clockwise.clone(),
                counterclockwise.clone(),
                None,
                vec![
                    EncoderMode::new(
                        "Default",
                        action("brake_bias_increase"),
                        action("brake_bias_decrease"),
                    )
                    .unwrap(),
                ],
            )
            .unwrap()])
            .unwrap();
        let mut engine =
            MappingEngine::connect(MockVirtualBackend::new(8), encoder_layout, mapping).unwrap();

        engine.handle_button(&clockwise, true).unwrap();
        engine.handle_button(&clockwise, false).unwrap();
        engine.handle_button(&counterclockwise, true).unwrap();
        engine.handle_button(&counterclockwise, false).unwrap();

        assert_eq!(
            engine.backend().events(),
            [(1, true), (1, false), (2, true), (2, false)]
        );
    }

    #[test]
    fn requires_a_mode_control_for_multiple_encoder_modes() {
        assert!(
            EncoderGroup::new(
                "Encoder 1",
                button("gt-neo", 10),
                button("gt-neo", 11),
                None,
                vec![
                    EncoderMode::new("Front ARB", action("pit_limiter"), action("headlights"))
                        .unwrap(),
                    EncoderMode::new("TC2", action("headlights"), action("pit_limiter")).unwrap(),
                ],
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_shared_or_regular_encoder_controls() {
        let clockwise = button("gt-neo", 10);
        let mode =
            EncoderMode::new("Front ARB", action("pit_limiter"), action("headlights")).unwrap();
        let group = EncoderGroup::new(
            "Encoder 1",
            clockwise.clone(),
            button("gt-neo", 11),
            Some(button("gt-neo", 12)),
            vec![mode],
        )
        .unwrap();
        let mapping = profile(&[("gt-neo", 10, "pit_limiter")]);

        assert!(mapping.with_encoder_groups([group]).is_err());
    }
}

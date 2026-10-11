//! Replaceable virtual-controller output for LMUKit.
//!
//! This crate is experimental. A successful backend connection does not prove
//! that Le Mans Ultimate recognises or accepts the virtual device.

use anyhow::Result;
use std::collections::BTreeSet;

mod engine;

#[cfg(target_os = "windows")]
pub mod direct_input;
#[cfg(target_os = "windows")]
pub mod vjoy;

pub use engine::{LogicalAction, MappingEngine, PhysicalButton, ProfileMapping, VirtualLayout};

/// Stable information about the virtual device acquired by a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualCapabilities {
    pub button_count: u32,
    pub identity: String,
}

/// Minimal output boundary used by the mapping engine and feasibility probe.
pub trait VirtualInputBackend {
    fn connect(&mut self) -> Result<()>;
    fn capabilities(&self) -> Result<VirtualCapabilities>;
    fn set_button(&mut self, button_id: u32, pressed: bool) -> Result<()>;
    fn release_all(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
}

/// Selects the lowest virtual button not already present in LMU's bindings.
pub fn next_unused_button(button_count: u32, used_buttons: &[u32]) -> Result<u32> {
    anyhow::ensure!(button_count > 0, "virtual controller has no buttons");
    (1..=button_count)
        .find(|button| !used_buttons.contains(button))
        .ok_or_else(|| anyhow::anyhow!("all {button_count} virtual buttons are already used"))
}

/// Session-local allocator used while teaching LMU the stable virtual layout.
/// Successful pulses stay reserved even if LMU has not written its JSON yet;
/// failed pulses return their button to the sequence.
#[derive(Debug)]
pub struct SetupButtonAllocator {
    button_count: u32,
    reserved_buttons: BTreeSet<u32>,
    in_flight: Option<u32>,
}

impl SetupButtonAllocator {
    pub fn new(button_count: u32, used_buttons: impl IntoIterator<Item = u32>) -> Result<Self> {
        anyhow::ensure!(button_count > 0, "virtual controller has no buttons");
        let reserved_buttons = used_buttons.into_iter().collect::<BTreeSet<_>>();
        if let Some(button) = reserved_buttons
            .iter()
            .find(|button| !(1..=button_count).contains(button))
        {
            anyhow::bail!(
                "virtual button B{button} is outside the configured B1..B{button_count} range"
            );
        }
        Ok(Self {
            button_count,
            reserved_buttons,
            in_flight: None,
        })
    }

    pub fn button_count(&self) -> u32 {
        self.button_count
    }

    pub fn used_button_count(&self) -> usize {
        self.reserved_buttons.len()
    }

    pub fn is_sending(&self) -> bool {
        self.in_flight.is_some()
    }

    pub fn next_button(&self) -> Option<u32> {
        (1..=self.button_count).find(|button| !self.reserved_buttons.contains(button))
    }

    pub fn reserve_next(&mut self) -> Result<u32> {
        anyhow::ensure!(
            self.in_flight.is_none(),
            "a virtual button pulse is already in progress"
        );
        let button = self.next_button().ok_or_else(|| {
            anyhow::anyhow!("all {} virtual buttons are already used", self.button_count)
        })?;
        self.reserved_buttons.insert(button);
        self.in_flight = Some(button);
        Ok(button)
    }

    pub fn finish_pulse(&mut self, button: u32, succeeded: bool) -> Result<()> {
        anyhow::ensure!(
            self.in_flight == Some(button),
            "virtual button B{button} is not the active setup pulse"
        );
        self.in_flight = None;
        if !succeeded {
            self.reserved_buttons.remove(&button);
        }
        Ok(())
    }
}

/// Small feasibility pipeline that mirrors two physical button states onto
/// stable virtual buttons 1 and 2.
///
/// This deliberately is not the eventual profile mapping engine. It exists so
/// the same physical-to-virtual path used by the hardware probe can be tested
/// against the mock backend before persisted mappings are introduced.
pub struct TwoButtonMirror<B: VirtualInputBackend> {
    backend: B,
    previous: [bool; 2],
    connected: bool,
}

impl<B: VirtualInputBackend> TwoButtonMirror<B> {
    pub fn connect(mut backend: B) -> Result<Self> {
        backend.connect()?;
        let capabilities = backend.capabilities()?;
        if capabilities.button_count < 2 {
            let _ = backend.disconnect();
            anyhow::bail!("virtual controller must provide at least two buttons");
        }
        backend.release_all()?;
        Ok(Self {
            backend,
            previous: [false; 2],
            connected: true,
        })
    }

    pub fn capabilities(&self) -> Result<VirtualCapabilities> {
        self.backend.capabilities()
    }

    pub fn update(&mut self, physical_buttons: [bool; 2]) -> Result<()> {
        anyhow::ensure!(self.connected, "button mirror is disconnected");
        for (index, pressed) in physical_buttons.into_iter().enumerate() {
            if pressed != self.previous[index] {
                self.backend.set_button(index as u32 + 1, pressed)?;
                self.previous[index] = pressed;
            }
        }
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        anyhow::ensure!(self.connected, "button mirror is disconnected");
        self.backend.disconnect()?;
        self.previous = [false; 2];
        self.connected = false;
        Ok(())
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }
}

impl<B: VirtualInputBackend> Drop for TwoButtonMirror<B> {
    fn drop(&mut self) {
        if self.connected {
            let _ = self.backend.disconnect();
        }
    }
}

/// Deterministic backend for mapping-engine and lifecycle tests.
#[derive(Debug)]
pub struct MockVirtualBackend {
    capabilities: VirtualCapabilities,
    connected: bool,
    buttons: Vec<bool>,
    events: Vec<(u32, bool)>,
}

impl MockVirtualBackend {
    pub fn new(button_count: u32) -> Self {
        Self {
            capabilities: VirtualCapabilities {
                button_count,
                identity: "LMUKit mock virtual controller".to_owned(),
            },
            connected: false,
            buttons: vec![false; button_count as usize],
            events: Vec::new(),
        }
    }

    pub fn button(&self, button_id: u32) -> Option<bool> {
        button_id
            .checked_sub(1)
            .and_then(|index| self.buttons.get(index as usize))
            .copied()
    }

    pub fn events(&self) -> &[(u32, bool)] {
        &self.events
    }

    fn ensure_connected(&self) -> Result<()> {
        anyhow::ensure!(self.connected, "virtual controller is not connected");
        Ok(())
    }

    fn button_index(&self, button_id: u32) -> Result<usize> {
        anyhow::ensure!(
            (1..=self.capabilities.button_count).contains(&button_id),
            "virtual button {button_id} is outside 1..={}",
            self.capabilities.button_count
        );
        Ok((button_id - 1) as usize)
    }
}

impl VirtualInputBackend for MockVirtualBackend {
    fn connect(&mut self) -> Result<()> {
        anyhow::ensure!(!self.connected, "virtual controller is already connected");
        self.connected = true;
        Ok(())
    }

    fn capabilities(&self) -> Result<VirtualCapabilities> {
        self.ensure_connected()?;
        Ok(self.capabilities.clone())
    }

    fn set_button(&mut self, button_id: u32, pressed: bool) -> Result<()> {
        self.ensure_connected()?;
        let index = self.button_index(button_id)?;
        self.buttons[index] = pressed;
        self.events.push((button_id, pressed));
        Ok(())
    }

    fn release_all(&mut self) -> Result<()> {
        self.ensure_connected()?;
        for (index, state) in self.buttons.iter_mut().enumerate() {
            if *state {
                *state = false;
                self.events.push((index as u32 + 1, false));
            }
        }
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.ensure_connected()?;
        self.release_all()?;
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MockVirtualBackend, SetupButtonAllocator, TwoButtonMirror, VirtualInputBackend,
        next_unused_button,
    };

    #[test]
    fn allocates_the_lowest_unused_virtual_button_within_capacity() {
        assert_eq!(next_unused_button(8, &[1, 2, 4]).unwrap(), 3);
        assert_eq!(next_unused_button(8, &[3, 2, 1]).unwrap(), 4);
    }

    #[test]
    fn reports_exhausted_or_empty_virtual_devices() {
        assert!(next_unused_button(2, &[1, 2]).is_err());
        assert!(next_unused_button(0, &[]).is_err());
    }

    #[test]
    fn setup_allocator_reserves_successful_pulses_without_waiting_for_lmu_json() {
        let mut allocator = SetupButtonAllocator::new(4, [1, 3]).unwrap();

        assert_eq!(allocator.next_button(), Some(2));
        let button = allocator.reserve_next().unwrap();
        assert_eq!(button, 2);
        assert!(allocator.reserve_next().is_err());

        allocator.finish_pulse(button, true).unwrap();
        assert_eq!(allocator.next_button(), Some(4));
        assert_eq!(allocator.used_button_count(), 3);
    }

    #[test]
    fn setup_allocator_retries_a_failed_pulse() {
        let mut allocator = SetupButtonAllocator::new(4, [1, 3]).unwrap();
        let button = allocator.reserve_next().unwrap();

        allocator.finish_pulse(button, false).unwrap();

        assert_eq!(allocator.next_button(), Some(2));
        assert_eq!(allocator.used_button_count(), 2);
    }

    #[test]
    fn setup_allocator_validates_capacity_and_exhaustion() {
        assert!(SetupButtonAllocator::new(0, []).is_err());
        assert!(SetupButtonAllocator::new(2, [3]).is_err());

        let mut allocator = SetupButtonAllocator::new(2, [1, 2]).unwrap();
        assert_eq!(allocator.next_button(), None);
        assert!(allocator.reserve_next().is_err());
    }

    #[test]
    fn mock_reports_stable_capabilities() {
        let mut backend = MockVirtualBackend::new(32);
        backend.connect().unwrap();

        let capabilities = backend.capabilities().unwrap();

        assert_eq!(capabilities.button_count, 32);
        assert_eq!(capabilities.identity, "LMUKit mock virtual controller");
    }

    #[test]
    fn mock_tracks_press_release_and_rejects_out_of_range_buttons() {
        let mut backend = MockVirtualBackend::new(2);
        backend.connect().unwrap();

        backend.set_button(1, true).unwrap();
        backend.set_button(1, false).unwrap();

        assert_eq!(backend.button(1), Some(false));
        assert_eq!(backend.events(), [(1, true), (1, false)]);
        assert!(backend.set_button(0, true).is_err());
        assert!(backend.set_button(3, true).is_err());
    }

    #[test]
    fn disconnect_releases_every_held_button() {
        let mut backend = MockVirtualBackend::new(3);
        backend.connect().unwrap();
        backend.set_button(1, true).unwrap();
        backend.set_button(3, true).unwrap();

        backend.disconnect().unwrap();

        assert_eq!(backend.button(1), Some(false));
        assert_eq!(backend.button(3), Some(false));
        assert_eq!(
            backend.events(),
            [(1, true), (3, true), (1, false), (3, false)]
        );
        assert!(backend.set_button(1, true).is_err());
    }

    #[test]
    fn two_button_pipeline_emits_only_physical_state_changes() {
        let mut pipeline = TwoButtonMirror::connect(MockVirtualBackend::new(2)).unwrap();

        pipeline.update([false, false]).unwrap();
        pipeline.update([true, false]).unwrap();
        pipeline.update([true, true]).unwrap();
        pipeline.update([true, true]).unwrap();
        pipeline.update([false, true]).unwrap();

        assert_eq!(
            pipeline.backend().events(),
            [(1, true), (2, true), (1, false)]
        );
    }

    #[test]
    fn two_button_pipeline_releases_held_outputs_on_disconnect() {
        let mut pipeline = TwoButtonMirror::connect(MockVirtualBackend::new(2)).unwrap();
        pipeline.update([true, true]).unwrap();

        pipeline.disconnect().unwrap();

        assert_eq!(
            pipeline.backend().events(),
            [(1, true), (2, true), (1, false), (2, false)]
        );
    }
}

use uuid::Uuid;

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    OpenSwitcher,
    ActivateProfile(Uuid),
}

#[cfg(windows)]
mod platform {
    use super::HotkeyAction;
    use global_hotkey::{
        GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
        hotkey::{Code, HotKey, Modifiers},
    };
    use uuid::Uuid;

    pub struct HotkeyManager {
        _manager: GlobalHotKeyManager,
        registrations: Vec<(u32, HotkeyAction)>,
    }

    impl HotkeyManager {
        pub fn new(profiles: &[(Uuid, u8)]) -> Result<Self, String> {
            let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string())?;
            let mut registrations = Vec::new();
            let switcher = hotkey(Code::Space);
            manager
                .register(switcher)
                .map_err(|error| error.to_string())?;
            registrations.push((switcher.id(), HotkeyAction::OpenSwitcher));

            for &(profile_id, slot) in profiles {
                let Some(code) = slot_code(slot) else {
                    continue;
                };
                let shortcut = hotkey(code);
                manager
                    .register(shortcut)
                    .map_err(|error| format!("could not register Ctrl+Alt+{slot}: {error}"))?;
                registrations.push((shortcut.id(), HotkeyAction::ActivateProfile(profile_id)));
            }
            Ok(Self {
                _manager: manager,
                registrations,
            })
        }

        pub fn poll(&self) -> Option<HotkeyAction> {
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.state == HotKeyState::Pressed
                    && let Some((_, action)) =
                        self.registrations.iter().find(|(id, _)| *id == event.id)
                {
                    return Some(*action);
                }
            }
            None
        }
    }

    fn hotkey(code: Code) -> HotKey {
        HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), code)
    }

    fn slot_code(slot: u8) -> Option<Code> {
        Some(match slot {
            1 => Code::Digit1,
            2 => Code::Digit2,
            3 => Code::Digit3,
            4 => Code::Digit4,
            5 => Code::Digit5,
            6 => Code::Digit6,
            7 => Code::Digit7,
            8 => Code::Digit8,
            9 => Code::Digit9,
            _ => return None,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::HotkeyAction;
    use uuid::Uuid;

    pub struct HotkeyManager;

    impl HotkeyManager {
        pub fn new(_profiles: &[(Uuid, u8)]) -> Result<Self, String> {
            Ok(Self)
        }

        pub fn poll(&self) -> Option<HotkeyAction> {
            None
        }
    }
}

pub use platform::HotkeyManager;

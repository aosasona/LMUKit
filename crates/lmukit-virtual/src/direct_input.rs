//! DirectInput button consumer used by the feasibility probe and desktop router.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    mem::size_of,
    ptr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use windows::{
    Win32::{
        Devices::HumanInterfaceDevice::{
            DI8DEVCLASS_GAMECTRL, DIDATAFORMAT, DIDEVCAPS, DIDEVICEINSTANCEW, DIEDFL_ATTACHEDONLY,
            DIENUM_CONTINUE, DIJOYSTATE2, DIRECTINPUT_VERSION, DISCL_BACKGROUND,
            DISCL_NONEXCLUSIVE, DirectInput8Create, IDirectInput8W, IDirectInputDevice8W,
        },
        Foundation::{BOOL, HINSTANCE},
        System::{Console::GetConsoleWindow, LibraryLoader::GetModuleHandleW},
        UI::WindowsAndMessaging::GetDesktopWindow,
    },
    core::{GUID, Interface},
};

#[link(name = "dinput8", kind = "static")]
unsafe extern "system" {
    static c_dfDIJoystick2: DIDATAFORMAT;
}

#[link(name = "dxguid", kind = "static")]
unsafe extern "system" {}

#[derive(Clone, Debug)]
pub struct DirectInputDeviceSummary {
    pub index: usize,
    pub device_id: String,
    pub name: String,
    pub product_name: String,
    pub button_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectInputButtonEvent {
    pub device_id: String,
    pub device_name: String,
    pub button_id: u32,
    pub pressed: bool,
}

#[derive(Clone)]
struct EnumeratedDevice {
    instance_guid: GUID,
    product_guid: GUID,
    name: String,
    product_name: String,
}

pub struct DirectInputButtonReader {
    device: IDirectInputDevice8W,
    pub summary: DirectInputDeviceSummary,
}

pub struct DirectInputButtonMonitor {
    readers: BTreeMap<String, DirectInputButtonReader>,
    previous: BTreeMap<String, Vec<bool>>,
    pending_events: Vec<DirectInputButtonEvent>,
    last_refresh: Instant,
}

impl DirectInputButtonReader {
    pub fn open(index: usize) -> Result<Self> {
        let direct_input = create_direct_input()?;
        let devices = enumerate_devices(&direct_input)?;
        let selected = devices
            .get(index)
            .with_context(|| format!("DirectInput controller index {index} is not available"))?;
        let mut device = None;
        // SAFETY: The GUID came from this DirectInput instance and the output
        // pointer is a valid Option managed by the windows crate.
        unsafe {
            direct_input
                .CreateDevice(&selected.instance_guid, &mut device, None)
                .context("creating the DirectInput device")?
        };
        let device = device.context("DirectInput did not return a controller interface")?;

        let mut capabilities = DIDEVCAPS {
            dwSize: size_of::<DIDEVCAPS>() as u32,
            ..Default::default()
        };
        // SAFETY: capabilities has the required size and remains live.
        unsafe {
            device
                .GetCapabilities(&mut capabilities)
                .context("reading DirectInput capabilities")?
        };
        ensure!(
            capabilities.dwButtons > 0,
            "DirectInput device has no buttons"
        );

        // SAFETY: c_dfDIJoystick2 is DirectInput's process-lifetime standard
        // joystick format. The API does not mutate it despite its legacy
        // non-const pointer signature.
        unsafe {
            device
                .SetDataFormat(ptr::addr_of!(c_dfDIJoystick2).cast_mut())
                .context("setting the standard DirectInput joystick format")?;
            let console_window = GetConsoleWindow();
            let window = if console_window.0.is_null() {
                GetDesktopWindow()
            } else {
                console_window
            };
            device
                .SetCooperativeLevel(window, DISCL_BACKGROUND | DISCL_NONEXCLUSIVE)
                .context("setting the DirectInput cooperative level")?;
            device
                .Acquire()
                .context("acquiring the DirectInput device")?;
        }

        Ok(Self {
            device,
            summary: DirectInputDeviceSummary {
                index,
                device_id: product_id(&selected.product_guid),
                name: selected.name.clone(),
                product_name: selected.product_name.clone(),
                button_count: capabilities.dwButtons,
            },
        })
    }

    pub fn first_two_buttons(&self) -> Result<[bool; 2]> {
        ensure!(
            self.summary.button_count >= 2,
            "DirectInput device has fewer than two buttons"
        );
        let buttons = self.buttons()?;
        Ok([buttons[0], buttons[1]])
    }

    pub fn buttons(&self) -> Result<Vec<bool>> {
        let mut state = DIJOYSTATE2::default();
        // SAFETY: the device uses c_dfDIJoystick2 and state has the matching
        // layout and writable size.
        unsafe {
            let _ = self.device.Poll();
            if self
                .device
                .GetDeviceState(
                    size_of::<DIJOYSTATE2>() as u32,
                    (&mut state as *mut DIJOYSTATE2).cast(),
                )
                .is_err()
            {
                self.device
                    .Acquire()
                    .context("reacquiring the DirectInput device")?;
                let _ = self.device.Poll();
                self.device
                    .GetDeviceState(
                        size_of::<DIJOYSTATE2>() as u32,
                        (&mut state as *mut DIJOYSTATE2).cast(),
                    )
                    .context("reading DirectInput joystick state")?;
            }
        }
        Ok(
            state.rgbButtons[..self.summary.button_count.min(128) as usize]
                .iter()
                .map(|value| value & 0x80 != 0)
                .collect(),
        )
    }
}

impl DirectInputButtonMonitor {
    pub fn new() -> Result<Self> {
        let mut monitor = Self {
            readers: BTreeMap::new(),
            previous: BTreeMap::new(),
            pending_events: Vec::new(),
            last_refresh: Instant::now() - Duration::from_secs(2),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn poll(&mut self) -> Result<Vec<DirectInputButtonEvent>> {
        if self.last_refresh.elapsed() >= Duration::from_secs(1) {
            self.refresh()?;
        }
        let mut events = std::mem::take(&mut self.pending_events);
        let mut failed = Vec::new();
        for (device_id, reader) in &self.readers {
            let buttons = match reader.buttons() {
                Ok(buttons) => buttons,
                Err(_) => {
                    failed.push(device_id.clone());
                    continue;
                }
            };
            let previous = self
                .previous
                .entry(device_id.clone())
                .or_insert_with(|| buttons.clone());
            for (index, (pressed, was_pressed)) in buttons.iter().zip(previous.iter()).enumerate() {
                if pressed != was_pressed {
                    events.push(DirectInputButtonEvent {
                        device_id: device_id.clone(),
                        device_name: reader.summary.product_name.clone(),
                        button_id: index as u32 + 1,
                        pressed: *pressed,
                    });
                }
            }
            *previous = buttons;
        }
        for device_id in failed {
            self.release_and_remove(&device_id, &mut events);
        }
        Ok(events)
    }

    fn refresh(&mut self) -> Result<()> {
        let devices = list_devices()?;
        let present = devices
            .iter()
            .map(|device| device.device_id.clone())
            .collect::<BTreeSet<_>>();
        let disconnected = self
            .readers
            .keys()
            .filter(|device_id| !present.contains(*device_id))
            .cloned()
            .collect::<Vec<_>>();
        for device_id in disconnected {
            let mut released = Vec::new();
            self.release_and_remove(&device_id, &mut released);
            self.pending_events.extend(released);
        }
        for device in devices.into_iter().filter(|device| device.button_count > 0) {
            if self.readers.contains_key(&device.device_id) {
                continue;
            }
            let reader = DirectInputButtonReader::open(device.index)?;
            let buttons = reader.buttons()?;
            self.previous.insert(device.device_id.clone(), buttons);
            self.readers.insert(device.device_id, reader);
        }
        self.last_refresh = Instant::now();
        Ok(())
    }

    fn release_and_remove(&mut self, device_id: &str, events: &mut Vec<DirectInputButtonEvent>) {
        let name = self
            .readers
            .get(device_id)
            .map(|reader| reader.summary.product_name.clone())
            .unwrap_or_else(|| "Disconnected controller".to_owned());
        if let Some(previous) = self.previous.remove(device_id) {
            for (index, pressed) in previous.into_iter().enumerate() {
                if pressed {
                    events.push(DirectInputButtonEvent {
                        device_id: device_id.to_owned(),
                        device_name: name.clone(),
                        button_id: index as u32 + 1,
                        pressed: false,
                    });
                }
            }
        }
        self.readers.remove(device_id);
    }
}

impl Drop for DirectInputButtonReader {
    fn drop(&mut self) {
        // SAFETY: Unacquire is safe for an acquired or already-lost device.
        let _ = unsafe { self.device.Unacquire() };
    }
}

pub fn list_devices() -> Result<Vec<DirectInputDeviceSummary>> {
    let direct_input = create_direct_input()?;
    enumerate_devices(&direct_input)?
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let mut device = None;
            // SAFETY: The GUID came from this DirectInput enumeration.
            unsafe { direct_input.CreateDevice(&item.instance_guid, &mut device, None)? };
            let device = device.context("DirectInput did not return a controller interface")?;
            let mut capabilities = DIDEVCAPS {
                dwSize: size_of::<DIDEVCAPS>() as u32,
                ..Default::default()
            };
            // SAFETY: capabilities has the required size and remains live.
            unsafe { device.GetCapabilities(&mut capabilities)? };
            Ok(DirectInputDeviceSummary {
                index,
                device_id: product_id(&item.product_guid),
                name: item.name,
                product_name: item.product_name,
                button_count: capabilities.dwButtons,
            })
        })
        .collect()
}

fn product_id(guid: &GUID) -> String {
    let vendor_id = guid.data1 as u16;
    let product_id = (guid.data1 >> 16) as u16;
    format!("hid:{vendor_id:04x}:{product_id:04x}")
}

fn create_direct_input() -> Result<IDirectInput8W> {
    // SAFETY: No borrowed module-name pointer is supplied.
    let module = unsafe { GetModuleHandleW(None)? };
    let mut raw = ptr::null_mut();
    // SAFETY: raw is a valid COM output pointer and IID requests IDirectInput8W.
    unsafe {
        DirectInput8Create(
            HINSTANCE(module.0),
            DIRECTINPUT_VERSION,
            &IDirectInput8W::IID,
            &mut raw,
            None,
        )?;
        ensure!(!raw.is_null(), "DirectInput returned a null interface");
        Ok(IDirectInput8W::from_raw(raw))
    }
}

fn enumerate_devices(direct_input: &IDirectInput8W) -> Result<Vec<EnumeratedDevice>> {
    let mut devices = Vec::<EnumeratedDevice>::new();
    // SAFETY: callback_context points to devices for the duration of the
    // synchronous enumeration and the callback copies every instance.
    unsafe {
        direct_input.EnumDevices(
            DI8DEVCLASS_GAMECTRL,
            Some(enumerate_callback),
            (&mut devices as *mut Vec<EnumeratedDevice>).cast(),
            DIEDFL_ATTACHEDONLY,
        )?;
    }
    Ok(devices)
}

unsafe extern "system" fn enumerate_callback(
    instance: *mut DIDEVICEINSTANCEW,
    context: *mut c_void,
) -> BOOL {
    // SAFETY: DirectInput supplies a valid instance, and enumerate_devices
    // supplies a live Vec as context for this synchronous callback.
    let instance = unsafe { &*instance };
    let devices = unsafe { &mut *context.cast::<Vec<EnumeratedDevice>>() };
    devices.push(EnumeratedDevice {
        instance_guid: instance.guidInstance,
        product_guid: instance.guidProduct,
        name: wide_string(&instance.tszInstanceName),
        product_name: wide_string(&instance.tszProductName),
    });
    BOOL(DIENUM_CONTINUE as i32)
}

fn wide_string(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

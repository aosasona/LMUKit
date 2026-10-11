//! DirectInput consumer used only by the Windows feasibility probe.

use std::{ffi::c_void, mem::size_of, ptr};

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
    pub name: String,
    pub product_name: String,
    pub button_count: u32,
}

#[derive(Clone)]
struct EnumeratedDevice {
    instance_guid: GUID,
    name: String,
    product_name: String,
}

pub struct DirectInputButtonReader {
    device: IDirectInputDevice8W,
    pub summary: DirectInputDeviceSummary,
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
            capabilities.dwButtons >= 2,
            "DirectInput device has fewer than two buttons"
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
                name: selected.name.clone(),
                product_name: selected.product_name.clone(),
                button_count: capabilities.dwButtons,
            },
        })
    }

    pub fn first_two_buttons(&self) -> Result<[bool; 2]> {
        let mut state = DIJOYSTATE2::default();
        // SAFETY: the device uses c_dfDIJoystick2 and state has the matching
        // layout and writable size.
        unsafe {
            let _ = self.device.Poll();
            self.device
                .GetDeviceState(
                    size_of::<DIJOYSTATE2>() as u32,
                    (&mut state as *mut DIJOYSTATE2).cast(),
                )
                .context("reading DirectInput joystick state")?;
        }
        Ok([
            state.rgbButtons[0] & 0x80 != 0,
            state.rgbButtons[1] & 0x80 != 0,
        ])
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
                name: item.name,
                product_name: item.product_name,
                button_count: capabilities.dwButtons,
            })
        })
        .collect()
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

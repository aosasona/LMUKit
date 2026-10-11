use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use libloading::Library;

use crate::{VirtualCapabilities, VirtualInputBackend};

type Bool = i32;
type VJoyEnabled = unsafe extern "C" fn() -> Bool;
type DriverMatch = unsafe extern "C" fn(*mut u16, *mut u16) -> Bool;
type GetVJoyVersion = unsafe extern "C" fn() -> i16;
type GetVjdButtonNumber = unsafe extern "C" fn(u32) -> i32;
type GetVjdStatus = unsafe extern "C" fn(u32) -> i32;
type AcquireVjd = unsafe extern "C" fn(u32) -> Bool;
type RelinquishVjd = unsafe extern "C" fn(u32);
type ResetButtons = unsafe extern "C" fn(u32) -> Bool;
type SetBtn = unsafe extern "C" fn(Bool, u32, u8) -> Bool;

const VJD_STAT_OWN: i32 = 0;
const VJD_STAT_FREE: i32 = 1;
const VJD_STAT_BUSY: i32 = 2;
const VJD_STAT_MISS: i32 = 3;

struct VJoyApi {
    _library: Library,
    enabled: VJoyEnabled,
    driver_match: DriverMatch,
    version: GetVJoyVersion,
    button_count: GetVjdButtonNumber,
    status: GetVjdStatus,
    acquire: AcquireVjd,
    relinquish: RelinquishVjd,
    reset_buttons: ResetButtons,
    set_button: SetBtn,
}

impl VJoyApi {
    fn load() -> Result<Self> {
        let candidates = dll_candidates();
        let mut errors = Vec::new();
        for candidate in candidates {
            // SAFETY: Loading a library is inherently unsafe. Symbols are resolved
            // below using the signatures published by vJoy's public C header.
            match unsafe { Library::new(&candidate) } {
                Ok(library) => {
                    return Self::from_library(library)
                        .with_context(|| format!("loading symbols from {}", candidate.display()));
                }
                Err(error) => errors.push(format!("{}: {error}", candidate.display())),
            }
        }
        bail!(
            "vJoyInterface.dll was not found. Install and configure vJoy explicitly, or set VJOY_INTERFACE_DLL. Tried: {}",
            errors.join("; ")
        )
    }

    fn from_library(library: Library) -> Result<Self> {
        // SAFETY: Each symbol and calling convention matches vjoyinterface.h.
        unsafe {
            Ok(Self {
                enabled: load_symbol(&library, b"vJoyEnabled\0")?,
                driver_match: load_symbol(&library, b"DriverMatch\0")?,
                version: load_symbol(&library, b"GetvJoyVersion\0")?,
                button_count: load_symbol(&library, b"GetVJDButtonNumber\0")?,
                status: load_symbol(&library, b"GetVJDStatus\0")?,
                acquire: load_symbol(&library, b"AcquireVJD\0")?,
                relinquish: load_symbol(&library, b"RelinquishVJD\0")?,
                reset_buttons: load_symbol(&library, b"ResetButtons\0")?,
                set_button: load_symbol(&library, b"SetBtn\0")?,
                _library: library,
            })
        }
    }
}

unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
    // SAFETY: The caller supplies the exact ABI type from vjoyinterface.h. The
    // returned function pointer remains valid because VJoyApi owns the library.
    Ok(*unsafe { library.get::<T>(name) }?)
}

fn dll_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("VJOY_INTERFACE_DLL") {
        candidates.push(path.into());
    }
    if let Ok(executable) = env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.push(directory.join("vJoyInterface.dll"));
    }
    candidates.push(PathBuf::from("vJoyInterface.dll"));
    candidates.push(PathBuf::from(
        r"C:\Program Files\vJoy\x64\vJoyInterface.dll",
    ));
    candidates
}

/// Runtime-loaded vJoy backend. LMUKit does not bundle or install the driver.
pub struct VJoyBackend {
    api: VJoyApi,
    device_id: u32,
    connected: bool,
    capabilities: Option<VirtualCapabilities>,
}

impl VJoyBackend {
    pub fn new(device_id: u32) -> Result<Self> {
        ensure!(
            (1..=16).contains(&device_id),
            "vJoy device ID must be 1..=16"
        );
        Ok(Self {
            api: VJoyApi::load()?,
            device_id,
            connected: false,
            capabilities: None,
        })
    }

    fn ensure_connected(&self) -> Result<()> {
        ensure!(self.connected, "vJoy device is not connected");
        Ok(())
    }
}

impl VirtualInputBackend for VJoyBackend {
    fn connect(&mut self) -> Result<()> {
        ensure!(!self.connected, "vJoy device is already connected");

        // SAFETY: All calls use the signatures and value ranges from vJoy's
        // public interface header. Pointer outputs refer to live stack values.
        unsafe {
            ensure!((self.api.enabled)() != 0, "the vJoy driver is not enabled");
            let mut dll_version = 0_u16;
            let mut driver_version = 0_u16;
            ensure!(
                (self.api.driver_match)(&mut dll_version, &mut driver_version) != 0,
                "vJoy DLL/driver version mismatch (DLL {dll_version:#06x}, driver {driver_version:#06x})"
            );

            match (self.api.status)(self.device_id) {
                VJD_STAT_FREE => ensure!(
                    (self.api.acquire)(self.device_id) != 0,
                    "vJoy device {} could not be acquired",
                    self.device_id
                ),
                VJD_STAT_OWN => {}
                VJD_STAT_BUSY => bail!("vJoy device {} is owned by another feeder", self.device_id),
                VJD_STAT_MISS => bail!("vJoy device {} is not configured", self.device_id),
                status => bail!(
                    "vJoy device {} returned unknown status {status}",
                    self.device_id
                ),
            }

            let button_count = (self.api.button_count)(self.device_id);
            if button_count <= 0 {
                (self.api.relinquish)(self.device_id);
                bail!("vJoy device {} has no configured buttons", self.device_id);
            }
            self.connected = true;
            self.capabilities = Some(VirtualCapabilities {
                button_count: button_count as u32,
                identity: format!(
                    "vJoy Device {} (driver {:#06x})",
                    self.device_id,
                    (self.api.version)() as u16
                ),
            });
        }
        self.release_all()?;
        Ok(())
    }

    fn capabilities(&self) -> Result<VirtualCapabilities> {
        self.ensure_connected()?;
        self.capabilities
            .clone()
            .context("vJoy capabilities were not initialised")
    }

    fn set_button(&mut self, button_id: u32, pressed: bool) -> Result<()> {
        self.ensure_connected()?;
        let count = self.capabilities()?.button_count;
        ensure!(
            (1..=count).contains(&button_id),
            "virtual button {button_id} is outside 1..={count}"
        );
        let button_id = u8::try_from(button_id).context("vJoy button ID exceeds its API range")?;
        // SAFETY: The backend owns the acquired device and validates the button.
        ensure!(
            unsafe { (self.api.set_button)(i32::from(pressed), self.device_id, button_id) } != 0,
            "vJoy rejected button {button_id} state {pressed}"
        );
        Ok(())
    }

    fn release_all(&mut self) -> Result<()> {
        self.ensure_connected()?;
        // SAFETY: The backend owns the acquired device.
        ensure!(
            unsafe { (self.api.reset_buttons)(self.device_id) } != 0,
            "vJoy failed to release all buttons"
        );
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.ensure_connected()?;
        let reset_result = self.release_all();
        // SAFETY: Relinquishing an acquired device is always valid.
        unsafe { (self.api.relinquish)(self.device_id) };
        self.connected = false;
        self.capabilities = None;
        reset_result
    }
}

impl Drop for VJoyBackend {
    fn drop(&mut self) {
        if self.connected {
            let _ = self.disconnect();
        }
    }
}

use std::{
    ptr,
    sync::atomic::{AtomicI32, Ordering},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use lmukit_core::car_context::{LmuCarContext, LmuCarContextProvider};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        Memory::{
            FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
            OpenFileMappingW, UnmapViewOfFile,
        },
        Threading::{
            EVENT_MODIFY_STATE, OpenEventW, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
            WaitForSingleObject,
        },
    },
};

use crate::{
    LMU_SHARED_MEMORY_NAME, LMU_SHARED_MEMORY_PAYLOAD_SIZE, decode_lmu_car_context, utc_timestamp,
};

const LOCK_MAPPING_NAME: &str = "LMU_SharedMemoryLockData";
const LOCK_EVENT_NAME: &str = "LMU_SharedMemoryLockEvent";
const LOCK_SIZE: usize = 8;
const LOCK_SPINS: usize = 4_000;
const LOCK_WAIT_MILLIS: u32 = 100;

pub struct WindowsLmuCarContextProvider {
    data: MappedView,
    lock: SharedMemoryLock,
}

impl WindowsLmuCarContextProvider {
    pub fn connect() -> Result<Self> {
        Ok(Self {
            data: MappedView::open_read(LMU_SHARED_MEMORY_NAME, LMU_SHARED_MEMORY_PAYLOAD_SIZE)
                .context("LMU_Data is unavailable; start LMU and enable Gameplay > Plugins")?,
            lock: SharedMemoryLock::open().context("LMU shared-memory lock is unavailable")?,
        })
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        let _guard = self.lock.acquire()?;
        let mut snapshot = vec![0; LMU_SHARED_MEMORY_PAYLOAD_SIZE];
        // SAFETY: both buffers are valid for the requested payload size. The
        // shared lock follows LMU's shipped reader protocol and keeps the game
        // from changing the source during this copy.
        unsafe {
            ptr::copy_nonoverlapping(
                self.data.address.Value.cast::<u8>(),
                snapshot.as_mut_ptr(),
                snapshot.len(),
            );
        }
        Ok(snapshot)
    }
}

impl LmuCarContextProvider for WindowsLmuCarContextProvider {
    fn current_context(&mut self) -> Result<LmuCarContext> {
        decode_lmu_car_context(&self.snapshot()?, Some(utc_timestamp(SystemTime::now())))
    }
}

struct MappedView {
    handle: HANDLE,
    address: MEMORY_MAPPED_VIEW_ADDRESS,
}

impl MappedView {
    fn open_read(name: &str, size: usize) -> Result<Self> {
        Self::open(name, size, FILE_MAP_READ)
    }

    fn open_read_write(name: &str, size: usize) -> Result<Self> {
        Self::open(name, size, FILE_MAP_READ | FILE_MAP_WRITE)
    }

    fn open(name: &str, size: usize, access: u32) -> Result<Self> {
        let wide_name = wide(name);
        // SAFETY: name is a terminated UTF-16 string and the returned handles
        // are owned by MappedView.
        let handle = unsafe { OpenFileMappingW(access, 0, wide_name.as_ptr()) };
        if handle.is_null() {
            bail!("Windows mapping {name} could not be opened");
        }
        // SAFETY: handle references a file mapping. An exact size makes an
        // incompatible or undersized LMU contract fail instead of reading on.
        let address = unsafe { MapViewOfFile(handle, access, 0, 0, size) };
        if address.Value.is_null() {
            // SAFETY: handle is owned and valid here.
            unsafe { CloseHandle(handle) };
            bail!("Windows mapping view could not be created");
        }
        Ok(Self { handle, address })
    }
}

impl Drop for MappedView {
    fn drop(&mut self) {
        // SAFETY: both resources are owned by this value and released once.
        unsafe {
            UnmapViewOfFile(self.address);
            CloseHandle(self.handle);
        }
    }
}

struct SharedMemoryLock {
    mapping: MappedView,
    event: HANDLE,
}

impl SharedMemoryLock {
    fn open() -> Result<Self> {
        let mapping = MappedView::open_read_write(LOCK_MAPPING_NAME, LOCK_SIZE)?;
        let event_name = wide(LOCK_EVENT_NAME);
        // SAFETY: event_name is terminated and the returned handle is owned.
        let event = unsafe {
            OpenEventW(
                SYNCHRONIZATION_SYNCHRONIZE | EVENT_MODIFY_STATE,
                0,
                event_name.as_ptr(),
            )
        };
        if event.is_null() {
            bail!("Windows event {LOCK_EVENT_NAME} could not be opened");
        }
        Ok(Self { mapping, event })
    }

    fn waiters(&self) -> &AtomicI32 {
        // SAFETY: the official lock mapping is two aligned LONG values and the
        // view stays alive for this reference.
        unsafe { &*self.mapping.address.Value.cast::<AtomicI32>() }
    }

    fn busy(&self) -> &AtomicI32 {
        // SAFETY: byte 4 is the aligned second LONG in the official lock data.
        unsafe {
            &*self
                .mapping
                .address
                .Value
                .cast::<u8>()
                .add(4)
                .cast::<AtomicI32>()
        }
    }

    fn acquire(&self) -> Result<SharedMemoryLockGuard<'_>> {
        for _ in 0..LOCK_SPINS {
            if self
                .busy()
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(SharedMemoryLockGuard(self));
            }
            std::hint::spin_loop();
        }

        self.waiters().fetch_add(1, Ordering::AcqRel);
        let result = loop {
            if self
                .busy()
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                break Ok(SharedMemoryLockGuard(self));
            }
            // SAFETY: event is an owned synchronization event handle.
            match unsafe { WaitForSingleObject(self.event, LOCK_WAIT_MILLIS) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => {
                    break Err(anyhow::anyhow!("timed out waiting for LMU shared memory"));
                }
                value => break Err(anyhow::anyhow!("LMU shared-memory wait failed ({value})")),
            }
        };
        self.waiters().fetch_sub(1, Ordering::AcqRel);
        result
    }

    fn release(&self) {
        self.busy().store(0, Ordering::Release);
        if self.waiters().load(Ordering::Acquire) > 0 {
            // SAFETY: event is an owned synchronization event handle.
            unsafe { SetEvent(self.event) };
        }
    }
}

impl Drop for SharedMemoryLock {
    fn drop(&mut self) {
        // SAFETY: the event handle is owned and released once.
        unsafe { CloseHandle(self.event) };
    }
}

struct SharedMemoryLockGuard<'a>(&'a SharedMemoryLock);

impl Drop for SharedMemoryLockGuard<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

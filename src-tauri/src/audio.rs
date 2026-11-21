use lmukit_core::storage::{AudioDeviceRole, AudioDeviceTarget, MicrophoneOperation, SystemAction};
use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock};

type ChangeNotifier = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDevice {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub muted: Option<bool>,
    pub default: bool,
    pub default_communications: bool,
}

enum Request {
    List(SyncSender<Result<Vec<CaptureDevice>, String>>),
    State(AudioDeviceTarget, SyncSender<Result<bool, String>>),
    Execute(SystemAction, bool, Option<SyncSender<Result<bool, String>>>),
}

#[derive(Clone)]
pub struct AudioService {
    sender: Sender<Request>,
    notifier: ChangeNotifier,
}

impl AudioService {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let notifier: ChangeNotifier = Arc::new(Mutex::new(None));
        let worker_notifier = Arc::clone(&notifier);
        std::thread::Builder::new()
            .name("pivot-audio-service".to_owned())
            .spawn(move || run_worker(receiver, worker_notifier))
            .expect("could not start the Windows audio service");
        Self { sender, notifier }
    }

    pub fn list_capture_devices(&self) -> Result<Vec<CaptureDevice>, String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(Request::List(reply))
            .map_err(service_stopped)?;
        result.recv().map_err(service_stopped)?
    }

    pub fn mute_state(&self, target: AudioDeviceTarget) -> Result<bool, String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(Request::State(target, reply))
            .map_err(service_stopped)?;
        result.recv().map_err(service_stopped)?
    }

    pub fn execute(&self, action: SystemAction, play_feedback_sound: bool) -> Result<bool, String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Execute(action, play_feedback_sound, Some(reply)))
            .map_err(service_stopped)?;
        result.recv().map_err(service_stopped)?
    }

    /// Queue an action without blocking the latency-sensitive input worker.
    pub fn dispatch(&self, action: SystemAction, play_feedback_sound: bool) -> Result<(), String> {
        self.sender
            .send(Request::Execute(action, play_feedback_sound, None))
            .map_err(|error| format!("The audio action queue is unavailable: {error}"))
    }

    pub fn set_change_notifier(&self, notify: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut current) = self.notifier.lock() {
            *current = Some(Arc::new(notify));
        }
    }
}

pub fn service() -> AudioService {
    static SERVICE: OnceLock<AudioService> = OnceLock::new();
    SERVICE.get_or_init(AudioService::start).clone()
}

fn desired_mute_state(current: bool, operation: MicrophoneOperation) -> bool {
    match operation {
        MicrophoneOperation::Toggle => !current,
        MicrophoneOperation::Mute => true,
        MicrophoneOperation::Unmute => false,
    }
}

trait MicrophoneControl {
    fn mute_state(&mut self, target: &AudioDeviceTarget) -> Result<bool, String>;
    fn set_mute(&mut self, target: &AudioDeviceTarget, muted: bool) -> Result<bool, String>;
}

fn execute_microphone_action(
    backend: &mut impl MicrophoneControl,
    action: &SystemAction,
) -> Result<bool, String> {
    let SystemAction::Microphone { operation, device } = action;
    let current = backend.mute_state(device)?;
    backend.set_mute(device, desired_mute_state(current, *operation))
}

fn service_stopped(error: impl std::fmt::Display) -> String {
    format!("The Windows audio service stopped unexpectedly: {error}")
}

fn run_worker(receiver: Receiver<Request>, notifier: ChangeNotifier) {
    #[cfg(target_os = "windows")]
    let mut backend = match windows_backend::WindowsAudio::new(Arc::clone(&notifier)) {
        Ok(backend) => Some(backend),
        Err(error) => {
            crate::diagnostics::error("audio.service.start", &error);
            None
        }
    };
    while let Ok(request) = receiver.recv() {
        #[cfg(target_os = "windows")]
        let result = match &mut backend {
            Some(backend) => match &request {
                Request::List(_) => backend.list().map(AudioResult::Devices),
                Request::State(target, _) => backend.mute_state(target).map(AudioResult::State),
                Request::Execute(action, _, _) => {
                    execute_microphone_action(backend, action).map(AudioResult::State)
                }
            },
            None => Err("Windows Core Audio is unavailable.".to_owned()),
        };
        #[cfg(not(target_os = "windows"))]
        let result: Result<AudioResult, String> =
            Err("Microphone control is only available in the Windows build.".to_owned());

        match request {
            Request::List(reply) => {
                let _ = reply.send(result.and_then(|value| match value {
                    AudioResult::Devices(devices) => Ok(devices),
                    _ => Err("The audio service returned an unexpected response.".to_owned()),
                }));
            }
            Request::State(_, reply) => {
                let _ = reply.send(result.and_then(|value| match value {
                    AudioResult::State(muted) => Ok(muted),
                    _ => Err("The audio service returned an unexpected response.".to_owned()),
                }));
            }
            Request::Execute(_action, play_feedback_sound, reply) => match result {
                Ok(AudioResult::State(muted)) => {
                    if play_feedback_sound {
                        play_state_feedback(muted);
                    }
                    notify_changed(&notifier);
                    crate::diagnostics::info(
                        "audio.microphone.changed",
                        if muted {
                            "Microphone muted"
                        } else {
                            "Microphone unmuted"
                        },
                    );
                    if let Some(reply) = reply {
                        let _ = reply.send(Ok(muted));
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    crate::diagnostics::warn("audio.microphone.failed", &error);
                    if let Some(reply) = reply {
                        let _ = reply.send(Err(error));
                    }
                }
            },
        }
    }
}

#[cfg(target_os = "windows")]
fn play_state_feedback(muted: bool) {
    static FEEDBACK: OnceLock<SyncSender<bool>> = OnceLock::new();
    let sender = FEEDBACK.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("pivot-microphone-feedback".to_owned())
            .spawn(move || run_feedback_worker(receiver))
            .expect("could not start the microphone feedback worker");
        sender
    });
    // Audio feedback is best-effort and must never hold up microphone or input work.
    let _ = sender.try_send(muted);
}

#[cfg(not(target_os = "windows"))]
fn play_state_feedback(_muted: bool) {}

#[cfg(target_os = "windows")]
fn run_feedback_worker(receiver: Receiver<bool>) {
    use windows::{
        Win32::{
            Foundation::HMODULE,
            Media::Audio::{PlaySoundA, SND_MEMORY, SND_NODEFAULT},
        },
        core::PCSTR,
    };

    let muted_wave = feedback_wave(true);
    let active_wave = feedback_wave(false);
    while let Ok(muted) = receiver.recv() {
        let wave = if muted { &muted_wave } else { &active_wave };
        // Synchronous playback is intentional on this isolated worker: the in-memory WAV must
        // remain valid until Windows finishes reading it, while input and Core Audio stay free.
        unsafe {
            let _ = PlaySoundA(
                PCSTR(wave.as_ptr()),
                HMODULE::default(),
                SND_MEMORY | SND_NODEFAULT,
            );
        }
    }
}

fn feedback_wave(muted: bool) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 22_050;
    const NOTE_MS: u32 = 82;
    const GAP_MS: u32 = 8;
    const RELEASE_MS: u32 = 22;
    let frequencies = if muted {
        [784.0_f32, 523.25_f32]
    } else {
        [523.25_f32, 784.0_f32]
    };
    let note_samples = (SAMPLE_RATE * NOTE_MS / 1_000) as usize;
    let gap_samples = (SAMPLE_RATE * GAP_MS / 1_000) as usize;
    let release_samples = (SAMPLE_RATE * RELEASE_MS / 1_000) as usize;
    let attack_samples = (SAMPLE_RATE * 6 / 1_000) as usize;
    let sample_count = note_samples * frequencies.len() + gap_samples;
    let data_size = (sample_count * std::mem::size_of::<i16>()) as u32;
    let mut wave = Vec::with_capacity(44 + data_size as usize);

    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + data_size).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wave.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wave.extend_from_slice(&2_u16.to_le_bytes());
    wave.extend_from_slice(&16_u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&data_size.to_le_bytes());

    for (note_index, frequency) in frequencies.into_iter().enumerate() {
        for sample_index in 0..note_samples {
            let attack = (sample_index as f32 / attack_samples.max(1) as f32).min(1.0);
            let remaining = note_samples - sample_index;
            let release = (remaining as f32 / release_samples.max(1) as f32).min(1.0);
            let envelope = attack * release;
            let time = sample_index as f32 / SAMPLE_RATE as f32;
            let phase = std::f32::consts::TAU * frequency * time;
            let tone = phase.sin() + 0.16 * (phase * 2.0).sin();
            let sample = (tone * envelope * 4_800.0) as i16;
            wave.extend_from_slice(&sample.to_le_bytes());
        }
        if note_index == 0 {
            wave.resize(wave.len() + gap_samples * 2, 0);
        }
    }
    wave
}

fn notify_changed(notifier: &ChangeNotifier) {
    if let Ok(current) = notifier.lock()
        && let Some(notify) = current.as_ref()
    {
        notify();
    }
}

enum AudioResult {
    Devices(Vec<CaptureDevice>),
    State(bool),
}

#[cfg(target_os = "windows")]
mod windows_backend {
    use super::*;
    use windows::{
        Win32::{
            Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
            Media::Audio::{
                AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE,
                DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED, EDataFlow,
                ERole,
                Endpoints::{
                    IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                    IAudioEndpointVolumeCallback_Impl,
                },
                IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
                MMDeviceEnumerator, eCapture, eCommunications, eConsole,
            },
            System::Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                CoUninitialize, STGM_READ,
            },
        },
        core::{PCWSTR, implement},
    };

    #[implement(IMMNotificationClient)]
    struct DeviceNotification {
        notifier: ChangeNotifier,
    }

    impl IMMNotificationClient_Impl for DeviceNotification_Impl {
        fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
        fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
        fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
        fn OnDefaultDeviceChanged(
            &self,
            _: EDataFlow,
            _: ERole,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
        fn OnPropertyValueChanged(
            &self,
            _: &PCWSTR,
            _: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
        ) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
    }

    #[implement(IAudioEndpointVolumeCallback)]
    struct VolumeNotification {
        notifier: ChangeNotifier,
    }

    impl IAudioEndpointVolumeCallback_Impl for VolumeNotification_Impl {
        fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
            notify_changed(&self.notifier);
            Ok(())
        }
    }

    pub struct WindowsAudio {
        enumerator: IMMDeviceEnumerator,
        device_notification: IMMNotificationClient,
        volume_notifications: Vec<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>,
        notifier: ChangeNotifier,
    }

    impl WindowsAudio {
        pub fn new(notifier: ChangeNotifier) -> Result<Self, String> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(format_windows_error)?;
                let result = (|| {
                    let enumerator: IMMDeviceEnumerator =
                        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                            .map_err(format_windows_error)?;
                    let device_notification: IMMNotificationClient = DeviceNotification {
                        notifier: Arc::clone(&notifier),
                    }
                    .into();
                    enumerator
                        .RegisterEndpointNotificationCallback(&device_notification)
                        .map_err(format_windows_error)?;
                    Ok(Self {
                        enumerator,
                        device_notification,
                        volume_notifications: Vec::new(),
                        notifier,
                    })
                })();
                if result.is_err() {
                    CoUninitialize();
                }
                result
            }
        }

        pub fn list(&mut self) -> Result<Vec<CaptureDevice>, String> {
            unsafe {
                for (volume, callback) in self.volume_notifications.drain(..) {
                    let _ = volume.UnregisterControlChangeNotify(&callback);
                }
                let default = self.default_id(eConsole).ok();
                let communications = self.default_id(eCommunications).ok();
                let states = windows::Win32::Media::Audio::DEVICE_STATE(
                    DEVICE_STATE_ACTIVE.0
                        | DEVICE_STATE_DISABLED.0
                        | DEVICE_STATE_NOTPRESENT.0
                        | DEVICE_STATE_UNPLUGGED.0,
                );
                let collection = self
                    .enumerator
                    .EnumAudioEndpoints(eCapture, states)
                    .map_err(format_windows_error)?;
                let mut devices = Vec::new();
                for index in 0..collection.GetCount().map_err(format_windows_error)? {
                    let device = collection.Item(index).map_err(format_windows_error)?;
                    let id = device_id(&device)?;
                    let state = device.GetState().map_err(format_windows_error)?;
                    let active = state == DEVICE_STATE_ACTIVE;
                    let name = device
                        .OpenPropertyStore(STGM_READ)
                        .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
                        .map(|value| value.to_string())
                        .unwrap_or_else(|_| "Unknown microphone".to_owned());
                    let muted = active
                        .then(|| {
                            endpoint_volume(&device).and_then(|volume| {
                                volume
                                    .GetMute()
                                    .map(|value| value.as_bool())
                                    .map_err(format_windows_error)
                            })
                        })
                        .transpose()
                        .ok()
                        .flatten();
                    if active && let Ok(volume) = endpoint_volume(&device) {
                        let callback: IAudioEndpointVolumeCallback = VolumeNotification {
                            notifier: Arc::clone(&self.notifier),
                        }
                        .into();
                        if volume.RegisterControlChangeNotify(&callback).is_ok() {
                            self.volume_notifications.push((volume, callback));
                        }
                    }
                    devices.push(CaptureDevice {
                        default: default.as_deref() == Some(&id),
                        default_communications: communications.as_deref() == Some(&id),
                        id,
                        name,
                        active,
                        muted,
                    });
                }
                devices.sort_by_key(|device| (!device.active, device.name.to_lowercase()));
                Ok(devices)
            }
        }

        fn read_state(&self, target: &AudioDeviceTarget) -> Result<bool, String> {
            unsafe {
                endpoint_volume(&self.resolve(target)?)?
                    .GetMute()
                    .map(|value| value.as_bool())
                    .map_err(format_windows_error)
            }
        }

        fn write_state(&self, target: &AudioDeviceTarget, muted: bool) -> Result<bool, String> {
            unsafe {
                let endpoint = endpoint_volume(&self.resolve(target)?)?;
                endpoint
                    .SetMute(muted, std::ptr::null())
                    .map_err(format_windows_error)?;
                endpoint
                    .GetMute()
                    .map(|value| value.as_bool())
                    .map_err(format_windows_error)
            }
        }

        fn resolve(&self, target: &AudioDeviceTarget) -> Result<IMMDevice, String> {
            unsafe {
                match target {
                    AudioDeviceTarget::Default { role } => self
                        .enumerator
                        .GetDefaultAudioEndpoint(
                            eCapture,
                            match role {
                                AudioDeviceRole::Console => eConsole,
                                AudioDeviceRole::Communications => eCommunications,
                            },
                        )
                        .map_err(format_windows_error),
                    AudioDeviceTarget::Specific { device_id } => {
                        let wide = device_id.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                        self.enumerator
                            .GetDevice(PCWSTR(wide.as_ptr()))
                            .map_err(|_| {
                                "The selected microphone is disconnected or no longer available."
                                    .to_owned()
                            })
                    }
                }
            }
        }

        fn default_id(&self, role: windows::Win32::Media::Audio::ERole) -> Result<String, String> {
            unsafe {
                device_id(
                    &self
                        .enumerator
                        .GetDefaultAudioEndpoint(eCapture, role)
                        .map_err(format_windows_error)?,
                )
            }
        }
    }

    impl MicrophoneControl for WindowsAudio {
        fn mute_state(&mut self, target: &AudioDeviceTarget) -> Result<bool, String> {
            self.read_state(target)
        }

        fn set_mute(&mut self, target: &AudioDeviceTarget, muted: bool) -> Result<bool, String> {
            self.write_state(target, muted)
        }
    }

    impl Drop for WindowsAudio {
        fn drop(&mut self) {
            unsafe {
                for (volume, callback) in self.volume_notifications.drain(..) {
                    let _ = volume.UnregisterControlChangeNotify(&callback);
                }
                let _ = self
                    .enumerator
                    .UnregisterEndpointNotificationCallback(&self.device_notification);
                CoUninitialize();
            }
        }
    }

    fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume, String> {
        unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .map_err(format_windows_error)
        }
    }

    fn device_id(device: &IMMDevice) -> Result<String, String> {
        unsafe {
            let value = device.GetId().map_err(format_windows_error)?;
            let id = value.to_string().map_err(|error| {
                format!("Windows returned an invalid microphone identifier: {error}")
            })?;
            CoTaskMemFree(Some(value.0.cast()));
            Ok(id)
        }
    }

    fn format_windows_error(error: windows::core::Error) -> String {
        format!("Windows Core Audio error: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_cues_are_valid_distinct_pcm_waves() {
        let muted = feedback_wave(true);
        let active = feedback_wave(false);

        assert_eq!(&muted[0..4], b"RIFF");
        assert_eq!(&muted[8..12], b"WAVE");
        assert_eq!(muted.len(), active.len());
        assert_ne!(muted, active);
    }

    #[test]
    fn microphone_operations_are_idempotent_except_for_toggle() {
        assert!(desired_mute_state(false, MicrophoneOperation::Toggle));
        assert!(!desired_mute_state(true, MicrophoneOperation::Toggle));
        assert!(desired_mute_state(false, MicrophoneOperation::Mute));
        assert!(desired_mute_state(true, MicrophoneOperation::Mute));
        assert!(!desired_mute_state(false, MicrophoneOperation::Unmute));
        assert!(!desired_mute_state(true, MicrophoneOperation::Unmute));
    }

    #[derive(Default)]
    struct MockMicrophone {
        muted: bool,
        writes: Vec<(AudioDeviceTarget, bool)>,
        unavailable: bool,
    }

    impl MicrophoneControl for MockMicrophone {
        fn mute_state(&mut self, _: &AudioDeviceTarget) -> Result<bool, String> {
            if self.unavailable {
                Err("device disconnected".into())
            } else {
                Ok(self.muted)
            }
        }

        fn set_mute(&mut self, target: &AudioDeviceTarget, muted: bool) -> Result<bool, String> {
            self.writes.push((target.clone(), muted));
            self.muted = muted;
            Ok(muted)
        }
    }

    #[test]
    fn dispatch_resolves_and_toggles_the_configured_target() {
        let target = AudioDeviceTarget::Default {
            role: AudioDeviceRole::Communications,
        };
        let action = SystemAction::Microphone {
            operation: MicrophoneOperation::Toggle,
            device: target.clone(),
        };
        let mut microphone = MockMicrophone::default();

        assert!(execute_microphone_action(&mut microphone, &action).unwrap());
        assert_eq!(microphone.writes, vec![(target, true)]);
    }

    #[test]
    fn disconnected_microphones_fail_without_reporting_a_successful_write() {
        let mut microphone = MockMicrophone {
            unavailable: true,
            ..Default::default()
        };
        let action = SystemAction::Microphone {
            operation: MicrophoneOperation::Mute,
            device: AudioDeviceTarget::Specific {
                device_id: "missing".into(),
            },
        };

        assert!(execute_microphone_action(&mut microphone, &action).is_err());
        assert!(microphone.writes.is_empty());
    }
}

use lmukit_context::{AcCarContext, AcSessionState};
use lmukit_core::car_context::{
    ConfirmedClassDebouncer, LmuCarContext, LmuCarContextSource, LmuCarContextStatus,
    LmuSessionState, display_existing_class,
};
use lmukit_core::storage::{
    AudioDeviceRole, AudioDeviceTarget, CompanionApp, CompanionAppIssue, InputMode, LaunchMode,
    MicrophoneOperation, Profile, Store, SystemAction, SystemActionBinding,
    VirtualAnalogAxisMapping, VirtualAxisCalibration, VirtualAxisId, VirtualAxisInput,
    VirtualButtonConflictResolution, VirtualButtonMapping, VirtualButtonModifier,
    VirtualClutchInput, VirtualClutchMapping, VirtualEncoderControl, VirtualEncoderGroup,
    VirtualEncoderMode as StoredEncoderMode, VirtualProfileMapping, Wheel, WorkspaceClutchSettings,
    WorkspaceKind, resolve_virtual_button_mappings, validate_assetto_corsa_root,
    validate_companion_apps,
};
use lmukit_virtual::{
    AxisCalibration, ClutchInput, ClutchMode, ClutchProcessor, EncoderGroup, EncoderMode,
    LogicalAction, PhysicalAxis, PhysicalButton, ProfileBinding, ProfileMapping,
    SetupButtonAllocator, VirtualAxis, VirtualLayout,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use uuid::Uuid;

mod audio;
mod diagnostics;
mod lmu_process;

#[cfg(target_os = "windows")]
use lmukit_virtual::{
    MappingEngine, VirtualInputBackend,
    direct_input::{DirectInputButtonMonitor, DirectInputEvent},
    vjoy::VJoyBackend,
};

#[path = "../../src/hotkeys.rs"]
mod hotkeys;
#[path = "../../src/input.rs"]
mod input;

struct AppState(Mutex<Store>);
struct AudioState(audio::AudioService);
struct HotkeyState(std::sync::mpsc::Sender<HotkeyUpdate>);
struct VirtualSetupState(Mutex<VirtualSetupSession>);
struct ManualPulseState(Arc<AtomicU64>);
struct VirtualRoutingState {
    operation: Mutex<()>,
    session: Mutex<Option<VirtualRoutingSession>>,
    status: Arc<Mutex<VirtualRoutingStatus>>,
}

struct CarContextState {
    status: Arc<Mutex<CarContextSummary>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

struct AcContextState {
    status: Arc<Mutex<AcContextSummary>>,
}

struct AutoProfileState(Mutex<AutoProfileRuntime>);

struct TrayMenuState {
    status: MenuItem<tauri::Wry>,
    routing_toggle: MenuItem<tauri::Wry>,
    switch_profile: MenuItem<tauri::Wry>,
    resume_auto: MenuItem<tauri::Wry>,
    presentation: Mutex<Option<TrayMenuPresentation>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrayMenuPresentation {
    status: String,
    routing_toggle_text: &'static str,
    routing_toggle_enabled: bool,
    switch_profile_enabled: bool,
    resume_auto_enabled: bool,
}

struct AutoProfileRuntime {
    paused: bool,
    manual_override_context: Option<String>,
    debouncer: ConfirmedClassDebouncer,
    last_attempted_profile: Option<Uuid>,
    message: String,
}

impl Default for AutoProfileRuntime {
    fn default() -> Self {
        Self {
            paused: false,
            manual_override_context: None,
            debouncer: ConfirmedClassDebouncer::new(2),
            last_attempted_profile: None,
            message: "Automatic profile selection is off.".to_owned(),
        }
    }
}

impl AutoProfileRuntime {
    fn reset(&mut self, message: impl Into<String>) {
        self.debouncer.reset();
        self.last_attempted_profile = None;
        self.message = message.into();
    }

    fn summary(&self, enabled: bool) -> AutoProfileSummary {
        AutoProfileSummary {
            enabled,
            paused: self.paused || self.manual_override_context.is_some(),
            candidate_class: self.debouncer.candidate().map(str::to_owned),
            confirmed_observations: self.debouncer.observations(),
            required_observations: self.debouncer.required_observations(),
            message: self.message.clone(),
        }
    }
}

struct VirtualRoutingSession {
    stop: Arc<std::sync::atomic::AtomicBool>,
    update: std::sync::mpsc::Sender<VirtualRoutingUpdate>,
    worker: std::thread::JoinHandle<()>,
    workspace_id: String,
    device_id: u32,
    has_virtual_backend: bool,
}

struct VirtualRoutingUpdate {
    profile_id: Uuid,
    profile_name: String,
    mapping: ProfileMapping,
    profile_system_actions: Vec<SystemActionBinding>,
    global_system_actions: Vec<SystemActionBinding>,
    clutch: Option<ClutchMode>,
    clutch_axis: Option<VirtualAxis>,
    analog_axes: Vec<RuntimeAnalogAxisMapping>,
    selection_reason: String,
    ready: std::sync::mpsc::SyncSender<Result<(String, Vec<ActiveEncoderModeSummary>), String>>,
}

#[derive(Clone)]
struct RuntimeAnalogAxisMapping {
    name: String,
    input: PhysicalAxis,
    calibration: AxisCalibration,
    output: VirtualAxis,
}

enum HotkeyUpdate {
    Profiles(Vec<(Uuid, u8)>),
    VirtualSetup(bool),
}

struct VirtualSetupSession {
    active: bool,
    device_id: u32,
    identity: String,
    workspace_id: String,
    workspace_kind: WorkspaceKind,
    pending_action: Option<String>,
    allocator: Option<SetupButtonAllocator>,
}

impl Default for VirtualSetupSession {
    fn default() -> Self {
        Self {
            active: false,
            device_id: 1,
            identity: String::new(),
            workspace_id: String::new(),
            workspace_kind: WorkspaceKind::LeMansUltimate,
            pending_action: None,
            allocator: None,
        }
    }
}

static LOOKUP_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualRoutingStatus {
    active: bool,
    profile_id: Option<Uuid>,
    profile_name: Option<String>,
    identity: Option<String>,
    routed_event_count: u64,
    last_event: Option<String>,
    error: Option<String>,
    selection_reason: Option<String>,
    active_encoder_modes: Vec<ActiveEncoderModeSummary>,
    clutch_output_percent: Option<f64>,
    clutch_axis: Option<String>,
    clutch_primary_input_percent: Option<f64>,
    clutch_bite_input_percent: Option<f64>,
    analog_axis_outputs: Vec<AnalogAxisOutputSummary>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalogAxisOutputSummary {
    name: String,
    input_percent: f64,
    output_percent: f64,
    virtual_axis: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActiveEncoderModeSummary {
    encoder_name: String,
    mode_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CarContextSummary {
    connected: bool,
    game_version: Option<i32>,
    vehicle_id: Option<String>,
    vehicle_name: Option<String>,
    class_raw: Option<String>,
    class_id: Option<String>,
    class_label: Option<String>,
    session_state: LmuSessionState,
    source: LmuCarContextSource,
    observed_at: Option<String>,
    status: LmuCarContextStatus,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcContextSummary {
    connected: bool,
    car_id: Option<String>,
    car_name: Option<String>,
    tags: Vec<String>,
    session_state: String,
    observed_at: Option<String>,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoProfileSummary {
    enabled: bool,
    paused: bool,
    candidate_class: Option<String>,
    confirmed_observations: u8,
    required_observations: u8,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LmuRuntimeSummary {
    running: bool,
    process_name: Option<String>,
}

impl Default for CarContextSummary {
    fn default() -> Self {
        Self {
            connected: false,
            game_version: None,
            vehicle_id: None,
            vehicle_name: None,
            class_raw: None,
            class_id: None,
            class_label: None,
            session_state: LmuSessionState::Unknown,
            source: LmuCarContextSource::Unknown,
            observed_at: None,
            status: LmuCarContextStatus::Unknown,
            message: "Waiting for LMU shared memory.".to_owned(),
        }
    }
}

impl Default for AcContextSummary {
    fn default() -> Self {
        Self {
            connected: false,
            car_id: None,
            car_name: None,
            tags: Vec::new(),
            session_state: "unknown".to_owned(),
            observed_at: None,
            message: "Waiting for Assetto Corsa shared memory.".to_owned(),
        }
    }
}

impl AcContextSummary {
    fn observed(context: AcCarContext) -> Self {
        Self {
            connected: true,
            car_id: context.car_id,
            car_name: context.car_name,
            tags: context.tags,
            session_state: match context.session_state {
                AcSessionState::Unknown => "unknown",
                AcSessionState::Replay => "replay",
                AcSessionState::Live => "live",
                AcSessionState::Paused => "paused",
            }
            .to_owned(),
            observed_at: context.observed_at,
            message: "Active car and Content Manager tags confirmed.".to_owned(),
        }
    }

    fn unavailable(previous: &Self) -> Self {
        let mut next = previous.clone();
        next.connected = false;
        next.session_state = "unknown".to_owned();
        next.message = "Assetto Corsa shared memory is unavailable.".to_owned();
        next
    }

    fn metadata_unavailable(previous: &Self) -> Self {
        let mut next = previous.clone();
        next.connected = true;
        next.session_state = "unknown".to_owned();
        next.message = "AC shared memory is connected, but the active car metadata could not be read. Check Settings → Assetto Corsa installation.".to_owned();
        next
    }
}

impl CarContextSummary {
    fn observed(context: LmuCarContext) -> Self {
        let class_label = context
            .class_normalized
            .as_deref()
            .map(display_existing_class)
            .map(str::to_owned);
        let message = if context.status == LmuCarContextStatus::Confirmed {
            "Player car context confirmed. Automatic profile selection is not enabled yet."
                .to_owned()
        } else {
            "LMU is connected, but no player car is currently confirmed.".to_owned()
        };
        Self {
            connected: true,
            game_version: context.game_version,
            vehicle_id: context.vehicle_id,
            vehicle_name: context.vehicle_name,
            class_raw: context.class_raw,
            class_id: context.class_normalized,
            class_label,
            session_state: context.session_state,
            source: context.source,
            observed_at: context.observed_at,
            status: context.status,
            message,
        }
    }

    fn unavailable(previous: &Self) -> Self {
        let mut next = previous.clone();
        next.connected = false;
        next.message =
            "LMU shared memory is unavailable. Start LMU to detect the player car.".to_owned();
        next.status = if next.observed_at.is_some() {
            LmuCarContextStatus::Stale
        } else {
            LmuCarContextStatus::Unknown
        };
        next
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticDeviceSummary {
    name: String,
    product_name: String,
    button_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticStatus {
    app_version: &'static str,
    log_directory: String,
    current_log_size: u64,
    retained_log_files: usize,
    log_retention_days: usize,
    recent_errors: Vec<String>,
    active_profile: Option<String>,
    input_mode: &'static str,
    virtual_health: String,
    routing: VirtualRoutingStatus,
    devices: Vec<DiagnosticDeviceSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupHealth {
    level: &'static str,
    title: String,
    message: String,
    button_count: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualProfileReadiness {
    mapping_count: usize,
    encoder_count: usize,
    missing_devices: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualProfileMappingSummary {
    device_id: String,
    device_name: String,
    physical_button: u32,
    modifiers: Vec<VirtualButtonModifierSummary>,
    logical_action: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualButtonCaptureSummary {
    mapping: VirtualProfileMappingSummary,
    conflicting_actions: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualButtonModifierSummary {
    device_id: String,
    device_name: String,
    physical_button: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemActionBindingSummary {
    scope: String,
    device_id: String,
    device_name: String,
    physical_button: u32,
    modifiers: Vec<VirtualButtonModifierSummary>,
    operation: String,
    target_type: String,
    target_id: Option<String>,
    play_feedback_sound: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalControlCaptureSummary {
    device_id: String,
    device_name: String,
    physical_button: u32,
    modifiers: Vec<VirtualButtonModifierSummary>,
}

#[derive(Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
enum ClutchInputCaptureSummary {
    #[serde(rename = "button")]
    Button {
        device_id: String,
        device_name: String,
        physical_button: u32,
    },
    #[serde(rename = "axis")]
    Axis {
        device_id: String,
        device_name: String,
        physical_axis: u32,
        minimum: i32,
        maximum: i32,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClutchConfigurationSummary {
    mapping: Option<VirtualClutchMapping>,
    analog_axes: Vec<VirtualAnalogAxisMapping>,
    workspace: Option<WorkspaceClutchSettings>,
    available_virtual_axes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualMappingImportSummary {
    mappings: Vec<VirtualProfileMappingSummary>,
    added: usize,
    already_mapped: usize,
    skipped_without_virtual_action: usize,
    skipped_non_button: usize,
    skipped_unknown_device: usize,
    conflicting_controls: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualProfileDefaultSummary {
    class_tag: String,
    profile_id: Uuid,
    profile_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualControlInspection {
    control: String,
    matches: Vec<VirtualControlInspectionMatch>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualControlInspectionMatch {
    logical_action: String,
    virtual_button: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualEncoderControlSummary {
    device_id: String,
    device_name: String,
    physical_button: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualEncoderModeSummary {
    name: String,
    clockwise_action: String,
    counterclockwise_action: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualEncoderGroupSummary {
    name: String,
    clockwise: VirtualEncoderControlSummary,
    counterclockwise: VirtualEncoderControlSummary,
    cycle: Option<VirtualEncoderControlSummary>,
    modes: Vec<VirtualEncoderModeSummary>,
}

#[derive(Clone)]
struct LearnedPhysicalButton {
    device_id: String,
    device_name: String,
    button_id: u32,
}

enum LearnedPhysicalMapping {
    Button(LearnedPhysicalButton),
    Modified {
        modifier: LearnedPhysicalButton,
        trigger: LearnedPhysicalButton,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    profiles: Vec<ProfileSummary>,
    wheels: Vec<WheelSummary>,
    workspaces: Vec<WorkspaceSummary>,
    active_workspace_id: String,
    active_workspace_kind: WorkspaceKind,
    active_profile: Option<Uuid>,
    lmu_config_path: String,
    lmu_settings_path: String,
    assetto_corsa_path: String,
    active_profile_dirty: Option<bool>,
    ui_font_scale: f32,
    input_mode: InputMode,
    auto_start_virtual_profile: bool,
    companion_apps: Vec<CompanionApp>,
    lmu_launch_mode: LaunchMode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSummary {
    id: String,
    name: String,
    kind: WorkspaceKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    id: Uuid,
    name: String,
    hotkey_slot: Option<u8>,
    binding_count: usize,
    devices: Vec<DeviceSummary>,
    has_wheel_image: bool,
    wheel_tags: Vec<String>,
    wheel_brand: Option<String>,
    wheel_name: Option<String>,
    class_tags: Vec<String>,
    custom_tags: Vec<String>,
    differs_from_live: Option<bool>,
    wheel_id: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicatedProfileSummary {
    id: Uuid,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WheelSummary {
    id: Uuid,
    brand: Option<String>,
    name: String,
    has_image: bool,
    profile_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSummary {
    key: String,
    name: String,
    binding_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingSummary {
    action: String,
    device_key: String,
    device_name: String,
    input_id: u64,
    alternate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingLookupResult {
    control: String,
    input_id: u64,
    matches: Vec<BindingLookupMatch>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingLookupMatch {
    profile_name: String,
    action: String,
    alternate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingAssignmentCandidate {
    control: String,
    input_id: u64,
    device_key: String,
    device_name: String,
    conflicts: Vec<BindingConflict>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingConflict {
    action: String,
    alternate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualBindingDevice {
    key: String,
    name: String,
    button_binding_count: usize,
    likely_virtual: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualActionSummary {
    action: String,
    virtual_button: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualButtonPulse {
    virtual_button: u32,
    identity: String,
    next_virtual_button: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualSetupStatus {
    identity: String,
    button_count: u32,
    used_button_count: usize,
    next_virtual_button: Option<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualSetupEvent {
    pulse: Option<VirtualButtonPulse>,
    action: Option<String>,
    error: Option<String>,
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let all_profiles = store.profiles().map_err(|error| error.to_string())?;
    let active_workspace = store.active_workspace().clone();
    let profiles = all_profiles
        .iter()
        .filter(|profile| profile.workspace_id == active_workspace.id)
        .cloned()
        .collect::<Vec<_>>();
    let summaries = profiles
        .iter()
        .map(|profile| profile_summary(&store, profile))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AppSnapshot {
        profiles: summaries,
        workspaces: store
            .workspaces()
            .iter()
            .map(|workspace| WorkspaceSummary {
                id: workspace.id.clone(),
                name: workspace.name.clone(),
                kind: workspace.kind,
            })
            .collect(),
        active_workspace_id: active_workspace.id,
        active_workspace_kind: active_workspace.kind,
        wheels: store
            .wheels()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|wheel| wheel_summary(wheel, &all_profiles))
            .collect(),
        active_profile: (active_workspace.kind == WorkspaceKind::LeMansUltimate)
            .then_some(store.settings.active_profile)
            .flatten(),
        lmu_config_path: store.settings.lmu_config_path.display().to_string(),
        lmu_settings_path: store.settings.lmu_settings_path.display().to_string(),
        assetto_corsa_path: store.settings.assetto_corsa_path.display().to_string(),
        active_profile_dirty: if active_workspace.kind == WorkspaceKind::LeMansUltimate {
            store
                .active_profile_has_unsaved_changes()
                .map_err(|error| error.to_string())?
        } else {
            None
        },
        ui_font_scale: store.settings.ui_font_scale,
        input_mode: store.settings.input_mode,
        auto_start_virtual_profile: store.settings.auto_start_virtual_profile,
        companion_apps: store.settings.companion_apps.clone(),
        lmu_launch_mode: store.settings.lmu_launch_mode,
    })
}

#[tauri::command]
fn create_virtual_profile(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .create_virtual_profile(&name)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_active_workspace(
    workspace_id: String,
    state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    setup_state: State<'_, VirtualSetupState>,
    hotkey_state: State<'_, HotkeyState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<(), String> {
    let _operation = routing_state
        .operation
        .lock()
        .map_err(|error| error.to_string())?;
    stop_virtual_routing_inner(&routing_state)?;
    {
        let mut setup = setup_state.0.lock().map_err(|error| error.to_string())?;
        setup.active = false;
        setup.pending_action = None;
    }
    let profiles = {
        let mut store = state.0.lock().map_err(|error| error.to_string())?;
        store
            .set_active_workspace(workspace_id.trim())
            .map_err(|error| error.to_string())?;
        store
            .active_workspace_profiles()
            .map_err(|error| error.to_string())?
    };
    hotkey_state
        .0
        .send(HotkeyUpdate::Profiles(profile_shortcuts(&profiles)))
        .map_err(|_| "The shortcut worker is not running.".to_owned())?;
    hotkey_state
        .0
        .send(HotkeyUpdate::VirtualSetup(false))
        .map_err(|_| "The shortcut worker is not running.".to_owned())?;
    pause_auto_profile_selection(&auto_state, "workspace changed");
    Ok(())
}

#[tauri::command]
fn set_ui_font_scale(scale: f32, state: State<'_, AppState>) -> Result<(), String> {
    if !(0.85..=1.4).contains(&scale) {
        return Err("Font scale must be between 85% and 140%.".to_owned());
    }
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store.settings.ui_font_scale = scale;
    store.save_settings().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_assetto_corsa_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Choose the Assetto Corsa installation folder.".to_owned());
    }
    let path = std::path::PathBuf::from(path);
    validate_assetto_corsa_root(&path).map_err(|error| error.to_string())?;
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store.settings.assetto_corsa_path = path;
    store.save_settings().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_input_mode(
    input_mode: InputMode,
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<(), String> {
    {
        let mut store = app_state.0.lock().map_err(|error| error.to_string())?;
        store.settings.input_mode = input_mode;
        store.save_settings().map_err(|error| error.to_string())?;
    }
    if input_mode == InputMode::Legacy {
        let _operation = routing_state
            .operation
            .lock()
            .map_err(|error| error.to_string())?;
        stop_virtual_routing_inner(&routing_state)?;
    }
    if let Ok(mut runtime) = auto_state.0.lock() {
        runtime.paused = false;
        runtime.reset(match input_mode {
            InputMode::Legacy => "Automatic profile selection waits for Virtual Input mode.",
            InputMode::Virtual => "Waiting for LMU to confirm a car class.",
        });
    }
    diagnostics::info(
        "input.mode.changed",
        match input_mode {
            InputMode::Legacy => "Input method changed to Legacy JSON",
            InputMode::Virtual => "Input method changed to Virtual Input",
        },
    );
    Ok(())
}

#[tauri::command]
fn set_virtual_auto_start(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store.settings.auto_start_virtual_profile = enabled;
    store.save_settings().map_err(|error| error.to_string())?;
    diagnostics::info(
        "routing.autostart.changed",
        if enabled {
            "Automatic live-profile startup enabled"
        } else {
            "Automatic live-profile startup disabled"
        },
    );
    Ok(())
}

#[tauri::command]
fn set_log_retention_days(retention_days: usize, state: State<'_, AppState>) -> Result<(), String> {
    if !(1..=30).contains(&retention_days) {
        return Err("Log retention must be between 1 and 30 days.".to_owned());
    }
    {
        let mut store = state.0.lock().map_err(|error| error.to_string())?;
        store.settings.log_retention_days = retention_days;
        store.save_settings().map_err(|error| error.to_string())?;
    }
    diagnostics::set_retention_days(retention_days)
        .map_err(|error| format!("Could not remove expired logs: {error}"))?;
    diagnostics::info(
        "diagnostics.retention",
        format!("Log retention changed to {retention_days} days"),
    );
    Ok(())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn prepare_for_update(
    routing_state: State<'_, VirtualRoutingState>,
    setup_state: State<'_, VirtualSetupState>,
    hotkey_state: State<'_, HotkeyState>,
    pulse_state: State<'_, ManualPulseState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<(), String> {
    let mut setup = setup_state.0.lock().map_err(|error| error.to_string())?;
    if setup
        .allocator
        .as_ref()
        .is_some_and(SetupButtonAllocator::is_sending)
    {
        return Err(
            "Wait for the current virtual button test to finish, then install the update again."
                .to_owned(),
        );
    }
    pause_auto_profile_selection(&auto_state, "application update");
    pulse_state.0.fetch_add(1, Ordering::AcqRel);
    setup.active = false;
    setup.pending_action = None;
    drop(setup);
    hotkey_state
        .0
        .send(HotkeyUpdate::VirtualSetup(false))
        .map_err(|_| "The shortcut worker is not running.".to_owned())?;
    let _operation = routing_state
        .operation
        .lock()
        .map_err(|error| error.to_string())?;
    stop_virtual_routing_inner(&routing_state)?;
    diagnostics::info(
        "app.update.prepared",
        "Stopped live remapping and released virtual outputs before update installation",
    );
    Ok(())
}

#[tauri::command]
fn save_app_settings(
    lmu_config_path: String,
    lmu_settings_path: String,
    companion_apps: Vec<CompanionApp>,
    launch_mode: LaunchMode,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let issues = validate_companion_apps(&companion_apps);
    if !issues.is_empty() {
        return Err(format!(
            "Companion app setup is invalid: {}",
            issues
                .iter()
                .map(|issue| format!("app {}: {}", issue.index + 1, issue.message))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store.settings.lmu_config_path = lmu_config_path.trim().into();
    store.settings.lmu_settings_path = lmu_settings_path.trim().into();
    store.settings.companion_apps = companion_apps;
    store.settings.lmu_launch_mode = launch_mode;
    store.save_settings().map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let refreshed = store
        .refresh_companion_launcher_if_exists(&executable)
        .map_err(|error| error.to_string())?;
    if refreshed {
        diagnostics::info(
            "launcher.refreshed",
            "Applied saved launch-mode and companion-app changes to the Steam launcher",
        );
    }
    Ok(())
}

#[tauri::command]
fn set_profile_hotkey(
    profile_id: Uuid,
    slot: Option<u8>,
    state: State<'_, AppState>,
    hotkey_state: State<'_, HotkeyState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .set_profile_hotkey(profile_id, slot)
        .map_err(|error| error.to_string())?;
    let profiles = store
        .active_workspace_profiles()
        .map_err(|error| error.to_string())?;
    hotkey_state
        .0
        .send(HotkeyUpdate::Profiles(profile_shortcuts(&profiles)))
        .map_err(|_| "The shortcut worker is not running.".to_owned())?;
    Ok(())
}

#[tauri::command]
fn companion_launch_option(state: State<'_, AppState>) -> Result<String, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    store
        .companion_launch_option(&executable)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn validate_companion_launch_setup(companion_apps: Vec<CompanionApp>) -> Vec<CompanionAppIssue> {
    validate_companion_apps(&companion_apps)
}

#[tauri::command]
fn pick_file(kind: String, initial_path: Option<String>) -> Result<Option<String>, String> {
    pick_file_dialog(&kind, initial_path.as_deref())
}

#[tauri::command]
fn load_game_settings(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .load_lmu_settings_document()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_game_settings(
    document: serde_json::Value,
    allow_while_lmu_running: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    ensure_lmu_file_write_allowed(allow_while_lmu_running)?;
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .save_lmu_settings_document(&document)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn lmu_runtime_status() -> Result<LmuRuntimeSummary, String> {
    let process_name = lmu_process::running_process()?;
    Ok(LmuRuntimeSummary {
        running: process_name.is_some(),
        process_name,
    })
}

fn ensure_lmu_file_write_allowed(allow_while_lmu_running: bool) -> Result<(), String> {
    let process = lmu_process::running_process()?;
    if process.is_some() && !allow_while_lmu_running {
        return Err(
            "LMU is running. Close it before writing its live files, or explicitly confirm that you want to continue."
                .to_owned(),
        );
    }
    if process.is_some() {
        diagnostics::warn(
            "lmu.write.while-running",
            "User confirmed a live-file write while LMU was running",
        );
    }
    Ok(())
}

#[tauri::command]
fn virtual_binding_devices(
    state: State<'_, AppState>,
) -> Result<Vec<VirtualBindingDevice>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let document = store
        .load_live_bindings_document()
        .map_err(|error| error.to_string())?;
    let bindings = Store::bindings_from_document(&document);
    let mut devices = document
        .get("Devices")
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| {
                    let name = value
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(key)
                        .to_owned();
                    VirtualBindingDevice {
                        key: key.clone(),
                        likely_virtual: name.to_ascii_lowercase().contains("vjoy")
                            || key.to_ascii_lowercase().contains("vjoy"),
                        name,
                        button_binding_count: bindings
                            .iter()
                            .filter(|binding| binding.device == *key && binding.input_id >= 32)
                            .count(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    devices.sort_by_key(|device| (!device.likely_virtual, device.name.to_lowercase()));
    Ok(devices)
}

#[tauri::command]
fn preview_virtual_layout(
    device_key: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualActionSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let document = store
        .load_live_bindings_document()
        .map_err(|error| error.to_string())?;
    let layout = Store::virtual_layout_from_document(&document, &device_key)
        .map_err(|error| error.to_string())?;
    Ok(layout
        .into_iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action,
            virtual_button: entry.virtual_button,
        })
        .collect())
}

#[tauri::command]
fn import_virtual_layout(
    device_key: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualActionSummary>, String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    let document = store
        .load_live_bindings_document()
        .map_err(|error| error.to_string())?;
    let layout = Store::virtual_layout_from_document(&document, &device_key)
        .map_err(|error| error.to_string())?;
    let response = layout
        .iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action.clone(),
            virtual_button: entry.virtual_button,
        })
        .collect();
    store
        .save_virtual_layout(device_key, layout)
        .map_err(|error| error.to_string())?;
    Ok(response)
}

#[tauri::command]
fn virtual_profile_mapping(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileMappingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    Ok(mapping_summaries(profile.virtual_mapping.as_ref()))
}

#[tauri::command]
fn virtual_profile_clutch(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<ClutchConfigurationSummary, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let available_virtual_axes = configured_virtual_axes(store.settings.virtual_device_id);
    Ok(ClutchConfigurationSummary {
        mapping: profile
            .virtual_mapping
            .as_ref()
            .and_then(|mapping| mapping.clutch.clone()),
        analog_axes: profile
            .virtual_mapping
            .as_ref()
            .map(|mapping| mapping.analog_axes.clone())
            .unwrap_or_default(),
        workspace: store
            .workspace_clutch_settings(&profile.workspace_id)
            .cloned(),
        available_virtual_axes,
    })
}

#[tauri::command]
fn save_virtual_profile_clutch(
    profile_id: Uuid,
    mapping: Option<VirtualClutchMapping>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .set_profile_clutch_mapping(profile_id, mapping)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_virtual_profile_analog_axes(
    profile_id: Uuid,
    analog_axes: Vec<VirtualAnalogAxisMapping>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .set_profile_analog_axes(profile_id, analog_axes)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_workspace_clutch_settings(
    profile_id: Uuid,
    virtual_axis: VirtualAxisId,
    shared_bite_point_percent: u8,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store
        .set_workspace_clutch_settings(WorkspaceClutchSettings {
            workspace_id: profile.workspace_id,
            virtual_axis,
            shared_bite_point_percent,
        })
        .map_err(|error| error.to_string())
}

/// Drives the saved clutch output through one complete virtual-axis movement
/// while live routing is stopped. This lets a simulator bind vJoy without also
/// receiving a simultaneous physical paddle movement.
#[tauri::command]
async fn test_virtual_clutch_axis(
    profile_id: Uuid,
    delay_ms: u64,
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
) -> Result<(), String> {
    if routing_state
        .status
        .lock()
        .map_err(|error| error.to_string())?
        .active
    {
        return Err(
            "Disable live remapping before sending a clutch binding test so Pivot can reserve vJoy safely."
                .to_owned(),
        );
    }
    let (device_id, axis) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        let profile = find_profile(&store, profile_id)?;
        let workspace = store
            .workspace_clutch_settings(&profile.workspace_id)
            .ok_or_else(|| "Save a vJoy clutch axis before testing it.".to_owned())?;
        (
            store.settings.virtual_device_id,
            runtime_virtual_axis(workspace.virtual_axis),
        )
    };
    let delay = std::time::Duration::from_millis(delay_ms.min(10_000));
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::spawn(move || pulse_virtual_clutch_axis(device_id, axis, delay))
            .join()
            .map_err(|_| "The virtual clutch test worker stopped unexpectedly.".to_owned())?
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(target_os = "windows")]
fn pulse_virtual_clutch_axis(
    device_id: u32,
    axis: VirtualAxis,
    delay: std::time::Duration,
) -> Result<(), String> {
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    let mut backend = VJoyBackend::new(device_id).map_err(|error| error.to_string())?;
    backend.connect().map_err(|error| {
        format!(
            "Could not reserve vJoy device {device_id} for the clutch test: {error}. If SimHub is configured as a vJoy feeder, it cannot share Pivot's vJoy device."
        )
    })?;
    let result = (|| -> Result<(), String> {
        let range = backend
            .capabilities()
            .map_err(|error| error.to_string())?
            .axes
            .get(&axis)
            .copied()
            .ok_or_else(|| {
                format!("vJoy device {device_id} does not have the selected {axis:?} axis.")
            })?;
        backend
            .set_axis(axis, range.minimum)
            .map_err(|error| error.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(120));
        backend
            .set_axis(axis, range.maximum)
            .map_err(|error| error.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(900));
        backend
            .set_axis(axis, range.minimum)
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    let _ = backend.disconnect();
    result
}

#[cfg(not(target_os = "windows"))]
fn pulse_virtual_clutch_axis(
    _device_id: u32,
    _axis: VirtualAxis,
    _delay: std::time::Duration,
) -> Result<(), String> {
    Err("Virtual clutch tests are only available in the Windows build.".to_owned())
}

#[tauri::command]
async fn learn_clutch_input(allow_button: bool) -> Result<ClutchInputCaptureSummary, String> {
    let generation = LOOKUP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::spawn(move || detect_clutch_input(generation, allow_button))
            .join()
            .map_err(|_| "The clutch input worker stopped unexpectedly.".to_owned())?
    })
    .await
    .map_err(|error| error.to_string())?
}

fn system_action_summary(scope: &str, binding: &SystemActionBinding) -> SystemActionBindingSummary {
    let SystemAction::Microphone { operation, device } = &binding.action;
    let (target_type, target_id) = match device {
        AudioDeviceTarget::Default {
            role: AudioDeviceRole::Console,
        } => ("default", None),
        AudioDeviceTarget::Default {
            role: AudioDeviceRole::Communications,
        } => ("communications", None),
        AudioDeviceTarget::Specific { device_id } => ("specific", Some(device_id.clone())),
    };
    SystemActionBindingSummary {
        scope: scope.to_owned(),
        device_id: binding.device_id.clone(),
        device_name: binding.device_name.clone(),
        physical_button: binding.physical_button,
        modifiers: binding
            .modifiers
            .iter()
            .map(|modifier| VirtualButtonModifierSummary {
                device_id: modifier.device_id.clone(),
                device_name: modifier.device_name.clone(),
                physical_button: modifier.physical_button,
            })
            .collect(),
        operation: match operation {
            MicrophoneOperation::Toggle => "toggle",
            MicrophoneOperation::Mute => "mute",
            MicrophoneOperation::Unmute => "unmute",
        }
        .to_owned(),
        target_type: target_type.to_owned(),
        target_id,
        play_feedback_sound: binding.play_feedback_sound,
    }
}

fn stored_system_action(
    summary: SystemActionBindingSummary,
) -> Result<SystemActionBinding, String> {
    let operation = match summary.operation.as_str() {
        "toggle" => MicrophoneOperation::Toggle,
        "mute" => MicrophoneOperation::Mute,
        "unmute" => MicrophoneOperation::Unmute,
        _ => return Err("Unknown microphone operation.".to_owned()),
    };
    let device = match summary.target_type.as_str() {
        "default" => AudioDeviceTarget::Default {
            role: AudioDeviceRole::Console,
        },
        "communications" => AudioDeviceTarget::Default {
            role: AudioDeviceRole::Communications,
        },
        "specific" => AudioDeviceTarget::Specific {
            device_id: summary
                .target_id
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Choose a specific microphone.".to_owned())?,
        },
        _ => return Err("Unknown microphone target.".to_owned()),
    };
    Ok(SystemActionBinding {
        device_id: summary.device_id,
        device_name: summary.device_name,
        physical_button: summary.physical_button,
        modifiers: summary
            .modifiers
            .into_iter()
            .map(|modifier| VirtualButtonModifier {
                device_id: modifier.device_id,
                device_name: modifier.device_name,
                physical_button: modifier.physical_button,
            })
            .collect(),
        play_feedback_sound: summary.play_feedback_sound,
        action: SystemAction::Microphone { operation, device },
    })
}

#[tauri::command]
fn system_action_bindings(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<Vec<SystemActionBindingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let mut result = store
        .settings
        .global_system_actions
        .iter()
        .map(|binding| system_action_summary("global", binding))
        .collect::<Vec<_>>();
    result.extend(
        profile
            .virtual_mapping
            .as_ref()
            .into_iter()
            .flat_map(|mapping| mapping.system_actions.iter())
            .map(|binding| system_action_summary("profile", binding)),
    );
    Ok(result)
}

#[tauri::command]
async fn capture_system_action_control() -> Result<PhysicalControlCaptureSummary, String> {
    let learned = wait_for_direct_input_mapping().await?;
    let (trigger, modifiers) = match learned {
        LearnedPhysicalMapping::Button(trigger) => (trigger, Vec::new()),
        LearnedPhysicalMapping::Modified { modifier, trigger } => (trigger, vec![modifier]),
    };
    Ok(PhysicalControlCaptureSummary {
        device_id: trigger.device_id,
        device_name: trigger.device_name,
        physical_button: trigger.button_id,
        modifiers: modifiers
            .into_iter()
            .map(|modifier| VirtualButtonModifierSummary {
                device_id: modifier.device_id,
                device_name: modifier.device_name,
                physical_button: modifier.button_id,
            })
            .collect(),
    })
}

#[tauri::command]
fn save_system_action_binding(
    profile_id: Uuid,
    binding: SystemActionBindingSummary,
    replace_existing: bool,
    also_trigger: bool,
    state: State<'_, AppState>,
) -> Result<Vec<SystemActionBindingSummary>, String> {
    let scope = binding.scope.clone();
    let binding = stored_system_action(binding)?;
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    if scope == "global" {
        let profile_mappings = store
            .profiles()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter_map(|profile| profile.virtual_mapping)
            .collect::<Vec<_>>();
        let profile_conflict = profile_mappings.iter().any(|mapping| {
            mapping.buttons.iter().any(|entry| {
                entry.device_id == binding.device_id
                    && entry.physical_button == binding.physical_button
                    && entry.modifiers == binding.modifiers
            })
        });
        if profile_conflict && !also_trigger {
            return Err("That global control already triggers a virtual action in at least one profile. Confirm that it should trigger both actions to continue.".to_owned());
        }
        let global_controls =
            std::iter::once((binding.device_id.as_str(), binding.physical_button))
                .chain(
                    binding
                        .modifiers
                        .iter()
                        .map(|modifier| (modifier.device_id.as_str(), modifier.physical_button)),
                )
                .collect::<std::collections::BTreeSet<_>>();
        let encoder_conflict = profile_mappings.iter().any(|mapping| {
            mapping.encoder_groups.iter().any(|encoder| {
                [&encoder.clockwise, &encoder.counterclockwise]
                    .into_iter()
                    .chain(encoder.cycle.iter())
                    .any(|control| {
                        global_controls
                            .contains(&(control.device_id.as_str(), control.physical_button))
                    })
            })
        });
        if encoder_conflict {
            return Err("That global control is part of a profile's digital encoder and cannot also trigger a system action.".to_owned());
        }
        let mut actions = store.settings.global_system_actions.clone();
        let conflict = actions.iter().any(|entry| {
            entry.device_id == binding.device_id
                && entry.physical_button == binding.physical_button
                && entry.modifiers == binding.modifiers
                && entry.action != binding.action
        });
        if conflict && !replace_existing {
            return Err("That global control already triggers another system action. Confirm replacement to continue.".to_owned());
        }
        actions.retain(|entry| {
            !(entry.device_id == binding.device_id
                && entry.physical_button == binding.physical_button
                && entry.modifiers == binding.modifiers)
        });
        actions.push(binding);
        store
            .set_global_system_actions(actions)
            .map_err(|error| error.to_string())?;
    } else {
        let profile = find_profile(&store, profile_id)?;
        let mapping = profile.virtual_mapping.unwrap_or_default();
        let system_controls =
            std::iter::once((binding.device_id.as_str(), binding.physical_button))
                .chain(
                    binding
                        .modifiers
                        .iter()
                        .map(|modifier| (modifier.device_id.as_str(), modifier.physical_button)),
                )
                .collect::<std::collections::BTreeSet<_>>();
        let conflicts_encoder = mapping.encoder_groups.iter().any(|encoder| {
            [&encoder.clockwise, &encoder.counterclockwise]
                .into_iter()
                .chain(encoder.cycle.iter())
                .any(|control| {
                    system_controls.contains(&(control.device_id.as_str(), control.physical_button))
                })
        });
        if conflicts_encoder {
            return Err(
                "That control is already part of a digital encoder and cannot also trigger a system action."
                    .to_owned(),
            );
        }
        let conflicts_virtual = mapping.buttons.iter().any(|entry| {
            entry.device_id == binding.device_id
                && entry.physical_button == binding.physical_button
                && entry.modifiers == binding.modifiers
        });
        if conflicts_virtual && !also_trigger {
            return Err("That profile control already triggers a virtual action. Confirm that it should trigger both actions to continue.".to_owned());
        }
        let mut actions = mapping.system_actions;
        let conflicts_system = actions.iter().any(|entry| {
            entry.device_id == binding.device_id
                && entry.physical_button == binding.physical_button
                && entry.modifiers == binding.modifiers
                && entry.action != binding.action
        });
        if conflicts_system && !replace_existing {
            return Err("That profile control already triggers another system action. Confirm replacement to continue.".to_owned());
        }
        actions.retain(|entry| {
            !(entry.device_id == binding.device_id
                && entry.physical_button == binding.physical_button
                && entry.modifiers == binding.modifiers)
        });
        actions.push(binding);
        store
            .set_profile_system_actions(profile_id, actions)
            .map_err(|error| error.to_string())?;
    }
    drop(store);
    system_action_bindings(profile_id, state)
}

#[tauri::command]
fn remove_system_action_binding(
    profile_id: Uuid,
    binding: SystemActionBindingSummary,
    state: State<'_, AppState>,
) -> Result<Vec<SystemActionBindingSummary>, String> {
    let stored = stored_system_action(binding.clone())?;
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    if binding.scope == "global" {
        let mut actions = store.settings.global_system_actions.clone();
        actions.retain(|entry| entry != &stored);
        store
            .set_global_system_actions(actions)
            .map_err(|error| error.to_string())?;
    } else {
        let profile = find_profile(&store, profile_id)?;
        let mut actions = profile.virtual_mapping.unwrap_or_default().system_actions;
        actions.retain(|entry| entry != &stored);
        store
            .set_profile_system_actions(profile_id, actions)
            .map_err(|error| error.to_string())?;
    }
    drop(store);
    system_action_bindings(profile_id, state)
}

#[tauri::command]
fn capture_audio_devices(
    audio: State<'_, AudioState>,
) -> Result<Vec<audio::CaptureDevice>, String> {
    audio.0.list_capture_devices()
}

#[tauri::command]
fn microphone_mute_state(
    target_type: String,
    target_id: Option<String>,
    audio: State<'_, AudioState>,
) -> Result<bool, String> {
    let target = stored_system_action(SystemActionBindingSummary {
        scope: String::new(),
        device_id: "query".into(),
        device_name: String::new(),
        physical_button: 1,
        modifiers: vec![],
        operation: "toggle".into(),
        target_type,
        target_id,
        play_feedback_sound: false,
    })?;
    let SystemAction::Microphone { device, .. } = target.action;
    audio.0.mute_state(device)
}

#[tauri::command]
fn test_microphone_action(
    operation: String,
    target_type: String,
    target_id: Option<String>,
    play_feedback_sound: bool,
    audio: State<'_, AudioState>,
) -> Result<bool, String> {
    let binding = stored_system_action(SystemActionBindingSummary {
        scope: String::new(),
        device_id: "test".into(),
        device_name: String::new(),
        physical_button: 1,
        modifiers: vec![],
        operation,
        target_type,
        target_id,
        play_feedback_sound,
    })?;
    audio.0.execute(binding.action, binding.play_feedback_sound)
}

#[tauri::command]
fn saved_virtual_layout(state: State<'_, AppState>) -> Result<Vec<VirtualActionSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    Ok(store
        .virtual_layout()
        .iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action.clone(),
            virtual_button: entry.virtual_button,
        })
        .collect())
}

#[tauri::command]
fn manual_virtual_actions(state: State<'_, AppState>) -> Result<Vec<VirtualActionSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    Ok(store
        .manual_virtual_layout()
        .into_iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action,
            virtual_button: entry.virtual_button,
        })
        .collect())
}

fn ensure_manual_pulse_available(
    routing_state: &VirtualRoutingState,
    setup_state: &VirtualSetupState,
) -> Result<(), String> {
    let active = routing_state
        .status
        .lock()
        .map_err(|error| error.to_string())?
        .active;
    if active
        || routing_state
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .is_some()
    {
        return Err("Disable live remapping before sending a manual virtual button.".to_owned());
    }
    if setup_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .active
    {
        return Err("Stop the guided binding setup before sending a manual action.".to_owned());
    }
    Ok(())
}

#[tauri::command]
async fn create_manual_virtual_action(
    action: String,
    state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    setup_state: State<'_, VirtualSetupState>,
) -> Result<VirtualActionSummary, String> {
    ensure_manual_pulse_available(&routing_state, &setup_state)?;
    let device_id = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .settings
        .virtual_device_id;
    let capabilities =
        tauri::async_runtime::spawn_blocking(move || inspect_virtual_device(device_id))
            .await
            .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))??;
    let virtual_button = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .next_manual_virtual_button(&action, capabilities.button_count)
        .map_err(|error| error.to_string())?;
    state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .add_manual_virtual_action(action.clone(), virtual_button)
        .map_err(|error| error.to_string())?;
    let pulse_result = match tauri::async_runtime::spawn_blocking(move || {
        pulse_virtual_button(device_id, virtual_button)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(format!(
            "Virtual button worker stopped unexpectedly: {error}"
        )),
    };
    if let Err(error) = pulse_result {
        let rollback = state
            .0
            .lock()
            .map_err(|lock_error| lock_error.to_string())?
            .remove_manual_virtual_action(&action);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error} The saved action could not be rolled back: {rollback_error}"
            )),
        };
    }
    diagnostics::info(
        "virtual.manual.created",
        format!("Created manually named virtual action on B{virtual_button}"),
    );
    Ok(VirtualActionSummary {
        action,
        virtual_button,
    })
}

#[tauri::command]
async fn test_manual_virtual_action(
    action: String,
    delay_ms: u64,
    state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    setup_state: State<'_, VirtualSetupState>,
    pulse_state: State<'_, ManualPulseState>,
) -> Result<VirtualButtonPulse, String> {
    if delay_ms != 0 && !(1_000..=15_000).contains(&delay_ms) {
        return Err("A test delay must be zero or between 1 and 15 seconds.".to_owned());
    }
    ensure_manual_pulse_available(&routing_state, &setup_state)?;
    let (device_id, virtual_button) = {
        let store = state.0.lock().map_err(|error| error.to_string())?;
        let entry = store
            .manual_virtual_layout()
            .into_iter()
            .find(|entry| entry.logical_action.eq_ignore_ascii_case(&action))
            .ok_or_else(|| "That manually named action no longer exists.".to_owned())?;
        (store.settings.virtual_device_id, entry.virtual_button)
    };
    let generation = pulse_state.0.fetch_add(1, Ordering::AcqRel) + 1;
    let delay_generation = Arc::clone(&pulse_state.0);
    let completed = tauri::async_runtime::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(delay_ms);
        while std::time::Instant::now() < deadline {
            if delay_generation.load(Ordering::Acquire) != generation {
                return false;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            std::thread::sleep(remaining.min(std::time::Duration::from_millis(50)));
        }
        delay_generation.load(Ordering::Acquire) == generation
    })
    .await
    .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))?;
    if !completed {
        return Err("Delayed virtual button test cancelled.".to_owned());
    }
    ensure_manual_pulse_available(&routing_state, &setup_state)?;
    let pulse_generation = Arc::clone(&pulse_state.0);
    let identity = tauri::async_runtime::spawn_blocking(move || {
        if pulse_generation.load(Ordering::Acquire) != generation {
            return Err("Delayed virtual button test cancelled.".to_owned());
        }
        pulse_virtual_button(device_id, virtual_button)
    })
    .await
    .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))??;
    Ok(VirtualButtonPulse {
        virtual_button,
        identity,
        next_virtual_button: None,
    })
}

#[tauri::command]
fn cancel_manual_virtual_action_test(pulse_state: State<'_, ManualPulseState>) {
    pulse_state.0.fetch_add(1, Ordering::AcqRel);
}

#[tauri::command]
fn remove_manual_virtual_action(
    action: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualActionSummary>, String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .remove_manual_virtual_action(&action)
        .map_err(|error| error.to_string())?;
    Ok(store
        .manual_virtual_layout()
        .into_iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action,
            virtual_button: entry.virtual_button,
        })
        .collect())
}

#[tauri::command]
fn import_profile_virtual_mapping(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<VirtualMappingImportSummary, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let report = store
        .import_profile_virtual_mapping(profile_id)
        .map_err(|error| error.to_string())?;
    Ok(VirtualMappingImportSummary {
        mappings: mapping_summaries(Some(&report.mapping)),
        added: report.added,
        already_mapped: report.already_mapped,
        skipped_without_virtual_action: report.skipped_without_virtual_action,
        skipped_non_button: report.skipped_non_button,
        skipped_unknown_device: report.skipped_unknown_device,
        conflicting_controls: report.conflicting_controls,
    })
}

#[tauri::command]
fn virtual_profile_defaults(
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileDefaultSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    virtual_profile_default_summaries(&store)
}

#[tauri::command]
fn set_virtual_profile_default(
    profile_id: Uuid,
    class_tag: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileDefaultSummary>, String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .set_virtual_profile_default(profile_id, class_tag)
        .map_err(|error| error.to_string())?;
    virtual_profile_default_summaries(&store)
}

#[tauri::command]
fn clear_virtual_profile_default(
    class_tag: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileDefaultSummary>, String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .clear_virtual_profile_default(class_tag)
        .map_err(|error| error.to_string())?;
    virtual_profile_default_summaries(&store)
}

#[tauri::command]
async fn learn_virtual_encoder_control() -> Result<VirtualEncoderControlSummary, String> {
    match wait_for_direct_input_mapping().await? {
        LearnedPhysicalMapping::Button(button) => Ok(VirtualEncoderControlSummary {
            device_id: button.device_id,
            device_name: button.device_name,
            physical_button: button.button_id,
        }),
        LearnedPhysicalMapping::Modified { .. } => {
            Err("Move or press only one encoder control at a time.".to_owned())
        }
    }
}

#[tauri::command]
fn virtual_profile_encoder_groups(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualEncoderGroupSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    Ok(encoder_group_summaries(
        profile.virtual_mapping.unwrap_or_default().encoder_groups,
    ))
}

#[tauri::command]
fn save_virtual_profile_encoder_group(
    profile_id: Uuid,
    original_name: Option<String>,
    encoder: VirtualEncoderGroupSummary,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualEncoderGroupSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let mut mapping = profile.virtual_mapping.unwrap_or_default();
    if let Some(original_name) = original_name {
        mapping
            .encoder_groups
            .retain(|entry| !entry.name.eq_ignore_ascii_case(&original_name));
    }
    mapping.encoder_groups.push(stored_encoder_group(encoder));
    store
        .set_profile_virtual_mapping(profile_id, Some(mapping.clone()))
        .map_err(|error| error.to_string())?;
    Ok(encoder_group_summaries(mapping.encoder_groups))
}

#[tauri::command]
fn remove_virtual_profile_encoder_group(
    profile_id: Uuid,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualEncoderGroupSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let mut mapping = profile.virtual_mapping.unwrap_or_default();
    let before = mapping.encoder_groups.len();
    mapping
        .encoder_groups
        .retain(|entry| !entry.name.eq_ignore_ascii_case(&name));
    if mapping.encoder_groups.len() == before {
        return Err("That encoder no longer exists.".to_owned());
    }
    let saved = (!mapping.buttons.is_empty()
        || !mapping.encoder_groups.is_empty()
        || !mapping.system_actions.is_empty()
        || mapping.clutch.is_some()
        || !mapping.analog_axes.is_empty())
    .then_some(mapping.clone());
    store
        .set_profile_virtual_mapping(profile_id, saved)
        .map_err(|error| error.to_string())?;
    Ok(encoder_group_summaries(mapping.encoder_groups))
}

#[tauri::command]
async fn capture_virtual_profile_button(
    profile_id: Uuid,
    logical_action: String,
    state: State<'_, AppState>,
) -> Result<VirtualButtonCaptureSummary, String> {
    let learned = wait_for_direct_input_mapping().await?;
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    if !store
        .virtual_layout_for_workspace(&profile.workspace_id)
        .iter()
        .any(|entry| entry.logical_action == logical_action)
    {
        return Err("That action is not in this workspace's saved button layout.".to_owned());
    }
    let (pressed, modifiers) = match learned {
        LearnedPhysicalMapping::Button(pressed) => (pressed, Vec::new()),
        LearnedPhysicalMapping::Modified { modifier, trigger } => (
            trigger,
            vec![VirtualButtonModifier {
                device_id: modifier.device_id,
                device_name: modifier.device_name,
                physical_button: modifier.button_id,
            }],
        ),
    };
    let proposed = VirtualButtonMapping {
        device_id: pressed.device_id,
        device_name: pressed.device_name,
        physical_button: pressed.button_id,
        modifiers,
        logical_action,
    };
    let conflicting_actions = store
        .virtual_button_conflicts(profile_id, &proposed)
        .map_err(|error| error.to_string())?;
    Ok(VirtualButtonCaptureSummary {
        mapping: mapping_summary(&proposed),
        conflicting_actions,
    })
}

#[tauri::command]
fn save_virtual_profile_button(
    profile_id: Uuid,
    proposed: VirtualProfileMappingSummary,
    replace_conflict: bool,
    also_trigger: bool,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileMappingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    if !store
        .virtual_layout_for_workspace(&profile.workspace_id)
        .iter()
        .any(|entry| entry.logical_action == proposed.logical_action)
    {
        return Err("That action is not in this workspace's saved button layout.".to_owned());
    }
    let proposed = stored_button_mapping(proposed);
    if replace_conflict && also_trigger {
        return Err("Choose either replace or also trigger, not both.".to_owned());
    }
    let conflict_resolution = if replace_conflict {
        VirtualButtonConflictResolution::Replace
    } else if also_trigger {
        VirtualButtonConflictResolution::AlsoTrigger
    } else {
        VirtualButtonConflictResolution::Reject
    };
    store
        .assign_profile_virtual_button(profile_id, proposed, conflict_resolution)
        .map(|mapping| mapping_summaries(Some(&mapping)))
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn inspect_virtual_profile_button(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<VirtualControlInspection, String> {
    let learned = wait_for_direct_input_mapping().await?;
    let (trigger, modifiers) = match learned {
        LearnedPhysicalMapping::Button(trigger) => (trigger, Vec::new()),
        LearnedPhysicalMapping::Modified { modifier, trigger } => (trigger, vec![modifier]),
    };
    let control = modifiers
        .iter()
        .map(|modifier| format!("{} · Button {}", modifier.device_name, modifier.button_id))
        .chain(std::iter::once(format!(
            "{} · Button {}",
            trigger.device_name, trigger.button_id
        )))
        .collect::<Vec<_>>()
        .join(" + ");
    let held_modifiers = modifiers
        .iter()
        .map(|modifier| (modifier.device_id.clone(), modifier.button_id))
        .collect::<Vec<_>>();
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let matched = profile
        .virtual_mapping
        .as_ref()
        .map_or_else(Vec::new, |mapping| {
            resolve_virtual_button_mappings(
                mapping,
                &trigger.device_id,
                trigger.button_id,
                &held_modifiers,
            )
        });
    let matches = matched
        .into_iter()
        .filter_map(|mapping| {
            store
                .virtual_layout_for_workspace(&profile.workspace_id)
                .iter()
                .find(|entry| entry.logical_action == mapping.logical_action)
                .map(|layout| VirtualControlInspectionMatch {
                    logical_action: mapping.logical_action.clone(),
                    virtual_button: layout.virtual_button,
                })
        })
        .collect();
    Ok(VirtualControlInspection { control, matches })
}

#[tauri::command]
fn remove_virtual_profile_button(
    profile_id: Uuid,
    device_id: String,
    physical_button: u32,
    modifiers: Vec<VirtualButtonModifierSummary>,
    logical_action: String,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileMappingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let mut mapping = profile.virtual_mapping.unwrap_or_default();
    let modifiers = modifiers
        .into_iter()
        .map(|modifier| VirtualButtonModifier {
            device_id: modifier.device_id,
            device_name: modifier.device_name,
            physical_button: modifier.physical_button,
        })
        .collect::<Vec<_>>();
    mapping.buttons.retain(|entry| {
        !(entry.device_id == device_id
            && entry.physical_button == physical_button
            && entry.modifiers == modifiers
            && entry.logical_action == logical_action)
    });
    let saved = (!mapping.buttons.is_empty()
        || !mapping.encoder_groups.is_empty()
        || !mapping.system_actions.is_empty()
        || mapping.clutch.is_some()
        || !mapping.analog_axes.is_empty())
    .then_some(mapping.clone());
    store
        .set_profile_virtual_mapping(profile_id, saved)
        .map_err(|error| error.to_string())?;
    Ok(mapping_summaries(Some(&mapping)))
}

fn mapping_summaries(mapping: Option<&VirtualProfileMapping>) -> Vec<VirtualProfileMappingSummary> {
    mapping
        .into_iter()
        .flat_map(|mapping| mapping.buttons.iter())
        .map(mapping_summary)
        .collect()
}

fn mapping_summary(entry: &VirtualButtonMapping) -> VirtualProfileMappingSummary {
    VirtualProfileMappingSummary {
        device_id: entry.device_id.clone(),
        device_name: entry.device_name.clone(),
        physical_button: entry.physical_button,
        modifiers: entry
            .modifiers
            .iter()
            .map(|modifier| VirtualButtonModifierSummary {
                device_id: modifier.device_id.clone(),
                device_name: modifier.device_name.clone(),
                physical_button: modifier.physical_button,
            })
            .collect(),
        logical_action: entry.logical_action.clone(),
    }
}

fn stored_button_mapping(entry: VirtualProfileMappingSummary) -> VirtualButtonMapping {
    VirtualButtonMapping {
        device_id: entry.device_id,
        device_name: entry.device_name,
        physical_button: entry.physical_button,
        modifiers: entry
            .modifiers
            .into_iter()
            .map(|modifier| VirtualButtonModifier {
                device_id: modifier.device_id,
                device_name: modifier.device_name,
                physical_button: modifier.physical_button,
            })
            .collect(),
        logical_action: entry.logical_action,
    }
}

fn encoder_control_summary(control: VirtualEncoderControl) -> VirtualEncoderControlSummary {
    VirtualEncoderControlSummary {
        device_id: control.device_id,
        device_name: control.device_name,
        physical_button: control.physical_button,
    }
}

fn encoder_group_summaries(groups: Vec<VirtualEncoderGroup>) -> Vec<VirtualEncoderGroupSummary> {
    groups
        .into_iter()
        .map(|group| VirtualEncoderGroupSummary {
            name: group.name,
            clockwise: encoder_control_summary(group.clockwise),
            counterclockwise: encoder_control_summary(group.counterclockwise),
            cycle: group.cycle.map(encoder_control_summary),
            modes: group
                .modes
                .into_iter()
                .map(|mode| VirtualEncoderModeSummary {
                    name: mode.name,
                    clockwise_action: mode.clockwise_action,
                    counterclockwise_action: mode.counterclockwise_action,
                })
                .collect(),
        })
        .collect()
}

fn stored_encoder_control(control: VirtualEncoderControlSummary) -> VirtualEncoderControl {
    VirtualEncoderControl {
        device_id: control.device_id,
        device_name: control.device_name,
        physical_button: control.physical_button,
    }
}

fn stored_encoder_group(group: VirtualEncoderGroupSummary) -> VirtualEncoderGroup {
    VirtualEncoderGroup {
        name: group.name,
        clockwise: stored_encoder_control(group.clockwise),
        counterclockwise: stored_encoder_control(group.counterclockwise),
        cycle: group.cycle.map(stored_encoder_control),
        modes: group
            .modes
            .into_iter()
            .map(|mode| StoredEncoderMode {
                name: mode.name,
                clockwise_action: mode.clockwise_action,
                counterclockwise_action: mode.counterclockwise_action,
            })
            .collect(),
    }
}

fn virtual_profile_default_summaries(
    store: &Store,
) -> Result<Vec<VirtualProfileDefaultSummary>, String> {
    let profiles = store.profiles().map_err(|error| error.to_string())?;
    Ok(store
        .settings
        .class_profile_defaults
        .iter()
        .filter(|entry| entry.workspace_id == store.settings.active_workspace_id)
        .filter_map(|entry| {
            profiles
                .iter()
                .find(|profile| profile.id == entry.profile_id)
                .map(|profile| VirtualProfileDefaultSummary {
                    class_tag: entry.class_tag.clone(),
                    profile_id: entry.profile_id,
                    profile_name: profile.name.clone(),
                })
        })
        .collect())
}

#[tauri::command]
fn virtual_routing_status(
    state: State<'_, VirtualRoutingState>,
) -> Result<VirtualRoutingStatus, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn startup_health(
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
) -> Result<StartupHealth, String> {
    let routing = routing_state
        .status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    if routing.active {
        return Ok(StartupHealth {
            level: "ready",
            title: "Live remapping is ready".to_owned(),
            message: format!(
                "{} is active through {}.",
                routing.profile_name.as_deref().unwrap_or("A profile"),
                routing.identity.as_deref().unwrap_or("vJoy")
            ),
            button_count: None,
        });
    }
    let (device_id, profile_count) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        (
            store.settings.virtual_device_id,
            store
                .active_workspace_profiles()
                .map_err(|error| error.to_string())?
                .len(),
        )
    };
    if profile_count == 0 {
        return Ok(StartupHealth {
            level: "warning",
            title: "Create your first profile".to_owned(),
            message: "This workspace has no profile to route yet.".to_owned(),
            button_count: None,
        });
    }
    let capabilities =
        tauri::async_runtime::spawn_blocking(move || inspect_virtual_device(device_id)).await;
    match capabilities {
        Ok(Ok(capabilities)) if capabilities.button_count >= 128 => Ok(StartupHealth {
            level: "ready",
            title: "Virtual input is ready".to_owned(),
            message: format!(
                "{} is available with {} buttons. Choose a mapped profile to begin.",
                capabilities.identity, capabilities.button_count
            ),
            button_count: Some(capabilities.button_count),
        }),
        Ok(Ok(capabilities)) => Ok(StartupHealth {
            level: "warning",
            title: "vJoy has limited capacity".to_owned(),
            message: format!(
                "{} provides {} buttons; 128 is recommended before registering more actions.",
                capabilities.identity, capabilities.button_count
            ),
            button_count: Some(capabilities.button_count),
        }),
        Ok(Err(error)) => Ok(StartupHealth {
            level: "error",
            title: "vJoy needs attention".to_owned(),
            message: error,
            button_count: None,
        }),
        Err(error) => Ok(StartupHealth {
            level: "error",
            title: "Virtual input check stopped".to_owned(),
            message: error.to_string(),
            button_count: None,
        }),
    }
}

#[tauri::command]
fn virtual_profile_readiness(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<VirtualProfileReadiness, String> {
    let (mapping_count, encoder_count, required_devices) = {
        let store = state.0.lock().map_err(|error| error.to_string())?;
        let profile = find_profile(&store, profile_id)?;
        if profile.workspace_id != store.settings.active_workspace_id {
            return Err("That profile belongs to another workspace.".to_owned());
        }
        let Some(mapping) = &profile.virtual_mapping else {
            return Ok(VirtualProfileReadiness {
                mapping_count: 0,
                encoder_count: 0,
                missing_devices: Vec::new(),
                warnings: vec![
                    "This profile has no Virtual Input mappings. Add at least one physical control before starting it live."
                        .to_owned(),
                ],
            });
        };
        let mut devices = BTreeMap::new();
        let mut remember = |device_id: &str, device_name: &str| {
            devices
                .entry(device_id.to_owned())
                .or_insert_with(|| device_name.to_owned());
        };
        for button in &mapping.buttons {
            remember(&button.device_id, &button.device_name);
            for modifier in &button.modifiers {
                remember(&modifier.device_id, &modifier.device_name);
            }
        }
        for encoder in &mapping.encoder_groups {
            remember(&encoder.clockwise.device_id, &encoder.clockwise.device_name);
            remember(
                &encoder.counterclockwise.device_id,
                &encoder.counterclockwise.device_name,
            );
            if let Some(cycle) = &encoder.cycle {
                remember(&cycle.device_id, &cycle.device_name);
            }
        }
        (mapping.buttons.len(), mapping.encoder_groups.len(), devices)
    };
    let missing_devices = missing_virtual_profile_devices(&required_devices)?;
    let mut warnings = Vec::new();
    if mapping_count == 0 && encoder_count == 0 {
        warnings.push(
            "This profile has no Virtual Input mappings. Add at least one physical control before starting it live."
                .to_owned(),
        );
    }
    if !missing_devices.is_empty() {
        warnings.push(format!(
            "These mapped devices are not currently detected: {}. Their controls will not work until they reconnect.",
            missing_devices.join(", ")
        ));
    }
    Ok(VirtualProfileReadiness {
        mapping_count,
        encoder_count,
        missing_devices,
        warnings,
    })
}

#[cfg(target_os = "windows")]
fn missing_virtual_profile_devices(
    required_devices: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    let available = lmukit_virtual::direct_input::list_devices()
        .map_err(|error| format!("Could not inspect connected controllers: {error}"))?
        .into_iter()
        .map(|device| device.device_id)
        .collect::<std::collections::HashSet<_>>();
    Ok(required_devices
        .iter()
        .filter(|(device_id, _)| !available.contains(*device_id))
        .map(|(_, device_name)| device_name.clone())
        .collect())
}

#[cfg(not(target_os = "windows"))]
fn missing_virtual_profile_devices(
    _required_devices: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

#[tauri::command]
fn lmu_car_context_status(state: State<'_, CarContextState>) -> Result<CarContextSummary, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn ac_car_context_status(state: State<'_, AcContextState>) -> Result<AcContextSummary, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn auto_profile_selection_status(
    app_state: State<'_, AppState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<AutoProfileSummary, String> {
    let enabled = app_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .settings
        .auto_profile_selection;
    auto_state
        .0
        .lock()
        .map(|runtime| runtime.summary(enabled))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_auto_profile_selection(
    enabled: bool,
    app_state: State<'_, AppState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<AutoProfileSummary, String> {
    let workspace_kind = {
        let mut store = app_state.0.lock().map_err(|error| error.to_string())?;
        store.settings.auto_profile_selection = enabled;
        store.save_settings().map_err(|error| error.to_string())?;
        store.active_workspace().kind
    };
    let mut runtime = auto_state.0.lock().map_err(|error| error.to_string())?;
    runtime.paused = false;
    runtime.manual_override_context = None;
    runtime.reset(if enabled {
        if workspace_kind == WorkspaceKind::AssettoCorsa {
            "Waiting for Assetto Corsa to confirm a car."
        } else {
            "Waiting for LMU to confirm a car class."
        }
    } else {
        "Automatic profile selection is off."
    });
    diagnostics::info(
        "routing.auto.changed",
        if enabled {
            "Automatic class profile selection enabled"
        } else {
            "Automatic class profile selection disabled"
        },
    );
    Ok(runtime.summary(enabled))
}

#[tauri::command]
fn resume_auto_profile_selection(
    app_state: State<'_, AppState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<AutoProfileSummary, String> {
    resume_auto_profile_selection_inner(&app_state, &auto_state)
}

fn resume_auto_profile_selection_inner(
    app_state: &AppState,
    auto_state: &AutoProfileState,
) -> Result<AutoProfileSummary, String> {
    let (enabled, workspace_kind) = app_state
        .0
        .lock()
        .map(|store| {
            (
                store.settings.auto_profile_selection,
                store.active_workspace().kind,
            )
        })
        .map_err(|error| error.to_string())?;
    if !enabled {
        return Err("Enable automatic profile selection first.".to_owned());
    }
    let mut runtime = auto_state.0.lock().map_err(|error| error.to_string())?;
    runtime.paused = false;
    runtime.manual_override_context = None;
    runtime.reset(if workspace_kind == WorkspaceKind::AssettoCorsa {
        "Waiting for Assetto Corsa to confirm a car."
    } else {
        "Waiting for LMU to confirm a car class."
    });
    diagnostics::info(
        "routing.auto.resumed",
        "Automatic class profile selection resumed",
    );
    Ok(runtime.summary(true))
}

fn pause_auto_profile_selection(state: &AutoProfileState, reason: &str) {
    if let Ok(mut runtime) = state.0.lock() {
        runtime.paused = true;
        runtime.manual_override_context = None;
        runtime.reset(format!(
            "Paused after {reason}. Select Resume Auto when ready."
        ));
    }
}

fn hold_manual_profile_for_class(state: &AutoProfileState, class_tag: Option<&str>) {
    hold_manual_profile_for_context(
        state,
        class_tag,
        class_tag.map(|tag| display_existing_class(tag).to_owned()),
    );
}

fn hold_manual_profile_for_context(
    state: &AutoProfileState,
    context_key: Option<&str>,
    context_label: Option<String>,
) {
    if let Ok(mut runtime) = state.0.lock() {
        runtime.paused = false;
        runtime.manual_override_context = context_key.map(str::to_owned);
        runtime.reset(match context_label {
            Some(context_label) => format!(
                "Keeping the manually selected profile for {context_label}. Automatic selection will resume when the detected car or class changes."
            ),
            None => "Automatic selection is active and waiting for a confirmed car or class."
                .to_owned(),
        });
    }
}

fn hold_manual_profile_for_active_context(
    app_state: &AppState,
    auto_state: &AutoProfileState,
    car_context: &CarContextState,
    ac_context: &AcContextState,
) -> Result<(), String> {
    let workspace_kind = app_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .active_workspace()
        .kind;
    match workspace_kind {
        WorkspaceKind::AssettoCorsa => {
            let context = ac_context.status.lock().ok();
            let context_key = context.as_ref().and_then(|context| context.car_id.clone());
            let context_label = context
                .as_ref()
                .and_then(|context| context.car_name.clone().or_else(|| context.car_id.clone()));
            hold_manual_profile_for_context(auto_state, context_key.as_deref(), context_label);
        }
        _ => {
            let class_tag = car_context
                .status
                .lock()
                .ok()
                .and_then(|context| confirmed_class(&context).map(str::to_owned));
            hold_manual_profile_for_class(auto_state, class_tag.as_deref());
        }
    }
    Ok(())
}

fn confirmed_class(context: &CarContextSummary) -> Option<&str> {
    (context.connected
        && context.status == LmuCarContextStatus::Confirmed
        && matches!(
            context.session_state,
            LmuSessionState::Session | LmuSessionState::Realtime | LmuSessionState::Replay
        ))
    .then_some(context.class_id.as_deref())
    .flatten()
}

#[tauri::command]
fn start_virtual_routing(
    profile_id: Uuid,
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    auto_state: State<'_, AutoProfileState>,
    car_context: State<'_, CarContextState>,
    ac_context: State<'_, AcContextState>,
) -> Result<VirtualRoutingStatus, String> {
    let status = start_virtual_routing_inner(
        profile_id,
        "Selected manually".to_owned(),
        &app_state,
        &routing_state,
    )?;
    hold_manual_profile_for_active_context(&app_state, &auto_state, &car_context, &ac_context)?;
    Ok(status)
}

#[tauri::command]
fn start_default_virtual_routing(
    class_tag: String,
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<VirtualRoutingStatus, String> {
    let (profile_id, reason) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        let profile = store
            .resolve_virtual_profile(&class_tag)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "No default profile is configured for {}.",
                    display_existing_class(&class_tag)
                )
            })?;
        let reason = format!("{} class default", display_existing_class(&class_tag));
        (profile.id, reason)
    };
    let status = start_virtual_routing_inner(profile_id, reason, &app_state, &routing_state)?;
    hold_manual_profile_for_class(&auto_state, Some(&class_tag));
    Ok(status)
}

fn start_virtual_routing_inner(
    profile_id: Uuid,
    selection_reason: String,
    app_state: &AppState,
    routing_state: &VirtualRoutingState,
) -> Result<VirtualRoutingStatus, String> {
    let _operation = routing_state
        .operation
        .lock()
        .map_err(|error| error.to_string())?;
    diagnostics::info(
        "routing.start.requested",
        format!("Profile {profile_id} · {selection_reason}"),
    );
    let (
        profile_name,
        device_id,
        layout,
        mapping,
        has_virtual_outputs,
        profile_system_actions,
        global_system_actions,
        clutch,
        clutch_axis,
        analog_axes,
        workspace_id,
    ) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        if store.settings.input_mode != InputMode::Virtual {
            return Err(
                "Switch the global input method to Virtual Input before enabling live remapping."
                    .to_owned(),
            );
        }
        let profile = find_profile(&store, profile_id)?;
        if profile.workspace_id != store.settings.active_workspace_id {
            return Err("That profile belongs to another workspace.".to_owned());
        }
        let persisted_mapping = profile.virtual_mapping.clone().unwrap_or_default();
        if persisted_mapping.buttons.is_empty()
            && persisted_mapping.encoder_groups.is_empty()
            && persisted_mapping.system_actions.is_empty()
            && persisted_mapping.clutch.is_none()
            && persisted_mapping.analog_axes.is_empty()
            && store.settings.global_system_actions.is_empty()
        {
            return Err("Learn at least one physical button for this profile first.".to_owned());
        }
        let layout = VirtualLayout::new(
            store
                .virtual_layout()
                .iter()
                .map(|entry| {
                    LogicalAction::new(entry.logical_action.clone())
                        .map(|action| (action, entry.virtual_button))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mapping = ProfileMapping::from_bindings(
            persisted_mapping
                .buttons
                .iter()
                .map(|entry| {
                    let trigger =
                        PhysicalButton::new(entry.device_id.clone(), entry.physical_button)
                            .map_err(|error| error.to_string())?;
                    let modifiers = entry
                        .modifiers
                        .iter()
                        .map(|modifier| {
                            PhysicalButton::new(
                                modifier.device_id.clone(),
                                modifier.physical_button,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let action = LogicalAction::new(entry.logical_action.clone())
                        .map_err(|error| error.to_string())?;
                    ProfileBinding::new(trigger, modifiers, action)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?,
        )
        .map_err(|error| error.to_string())?
        .with_encoder_groups(
            persisted_mapping
                .encoder_groups
                .iter()
                .map(|encoder| {
                    let control = |control: &lmukit_core::storage::VirtualEncoderControl| {
                        PhysicalButton::new(control.device_id.clone(), control.physical_button)
                            .map_err(|error| error.to_string())
                    };
                    EncoderGroup::new(
                        encoder.name.clone(),
                        control(&encoder.clockwise)?,
                        control(&encoder.counterclockwise)?,
                        encoder.cycle.as_ref().map(control).transpose()?,
                        encoder
                            .modes
                            .iter()
                            .map(|mode| {
                                EncoderMode::new(
                                    mode.name.clone(),
                                    LogicalAction::new(mode.clockwise_action.clone())
                                        .map_err(|error| error.to_string())?,
                                    LogicalAction::new(mode.counterclockwise_action.clone())
                                        .map_err(|error| error.to_string())?,
                                )
                                .map_err(|error| error.to_string())
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    )
                    .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?,
        )
        .map_err(|error| error.to_string())?;
        let workspace_clutch = store
            .workspace_clutch_settings(&profile.workspace_id)
            .cloned();
        let (clutch, clutch_axis) = match persisted_mapping.clutch.as_ref() {
            Some(mapping) => {
                let workspace = workspace_clutch.as_ref().ok_or_else(|| {
                    "Select a persistent vJoy clutch axis for this workspace first.".to_owned()
                })?;
                (
                    Some(runtime_clutch_mode(mapping, workspace)?),
                    Some(runtime_virtual_axis(workspace.virtual_axis)),
                )
            }
            None => (None, None),
        };
        let analog_axes = persisted_mapping
            .analog_axes
            .iter()
            .map(runtime_analog_axis)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(clutch_axis) = clutch_axis {
            if analog_axes
                .iter()
                .any(|mapping| mapping.output == clutch_axis)
            {
                return Err(
                    "The clutch and another analog mapping cannot write the same vJoy axis."
                        .to_owned(),
                );
            }
        }
        let has_virtual_outputs = !persisted_mapping.buttons.is_empty()
            || !persisted_mapping.encoder_groups.is_empty()
            || clutch.is_some()
            || !analog_axes.is_empty();
        (
            profile.name,
            store.settings.virtual_device_id,
            layout,
            mapping,
            has_virtual_outputs,
            persisted_mapping.system_actions,
            store.settings.global_system_actions.clone(),
            clutch,
            clutch_axis,
            analog_axes,
            profile.workspace_id,
        )
    };
    if let Some(session) = routing_state
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .filter(|session| {
            session.workspace_id == workspace_id
                && session.device_id == device_id
                && session.has_virtual_backend
        })
    {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        session
            .update
            .send(VirtualRoutingUpdate {
                profile_id,
                profile_name: profile_name.clone(),
                mapping,
                profile_system_actions,
                global_system_actions,
                clutch,
                clutch_axis,
                analog_axes,
                selection_reason: selection_reason.clone(),
                ready: ready_tx,
            })
            .map_err(|_| "The active virtual routing worker is unavailable.".to_owned())?;
        let (identity, active_encoder_modes) = ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| {
                "The active virtual routing worker did not switch profiles in time.".to_owned()
            })??;
        let next = VirtualRoutingStatus {
            active: true,
            profile_id: Some(profile_id),
            profile_name: Some(profile_name),
            identity: Some(identity),
            selection_reason: Some(selection_reason),
            active_encoder_modes,
            clutch_output_percent: clutch_axis.map(|_| 0.0),
            clutch_axis: clutch_axis.map(virtual_axis_name).map(str::to_owned),
            ..VirtualRoutingStatus::default()
        };
        *routing_state
            .status
            .lock()
            .map_err(|error| error.to_string())? = next.clone();
        if let Ok(mut store) = app_state.0.lock() {
            store.settings.last_virtual_profile = Some(profile_id);
            if let Err(error) = store.save_settings() {
                diagnostics::warn("routing.last-profile.save", error.to_string());
            }
        }
        diagnostics::info(
            "routing.profile.changed",
            format!("Profile {profile_id} activated without reacquiring vJoy"),
        );
        return Ok(next);
    }
    stop_virtual_routing_inner(routing_state)?;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let status = Arc::clone(&routing_state.status);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (update_tx, update_rx) = std::sync::mpsc::channel();
    let worker_profile_name = profile_name.clone();
    let worker_selection_reason = selection_reason.clone();
    let worker = std::thread::spawn(move || {
        run_virtual_routing_worker(
            profile_id,
            worker_profile_name,
            device_id,
            layout,
            mapping,
            has_virtual_outputs,
            profile_system_actions,
            global_system_actions,
            clutch,
            clutch_axis,
            analog_axes,
            audio::service(),
            worker_stop,
            status,
            ready_tx,
            worker_selection_reason,
            update_rx,
        );
    });
    let (identity, active_encoder_modes) =
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => {
                let _ = worker.join();
                diagnostics::error("routing.start.failed", &error);
                return Err(error);
            }
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                let _ = worker.join();
                diagnostics::error(
                    "routing.start.timeout",
                    "Virtual routing did not start within five seconds",
                );
                return Err("Virtual routing did not start within five seconds.".to_owned());
            }
        };
    *routing_state
        .session
        .lock()
        .map_err(|error| error.to_string())? = Some(VirtualRoutingSession {
        stop,
        update: update_tx,
        worker,
        workspace_id,
        device_id,
        has_virtual_backend: has_virtual_outputs,
    });
    let next = VirtualRoutingStatus {
        active: true,
        profile_id: Some(profile_id),
        profile_name: Some(profile_name),
        identity: Some(identity),
        selection_reason: Some(selection_reason),
        active_encoder_modes,
        clutch_output_percent: clutch_axis.map(|_| 0.0),
        clutch_axis: clutch_axis.map(virtual_axis_name).map(str::to_owned),
        ..VirtualRoutingStatus::default()
    };
    *routing_state
        .status
        .lock()
        .map_err(|error| error.to_string())? = next.clone();
    diagnostics::info(
        "routing.started",
        format!(
            "{} · {}",
            next.profile_name.as_deref().unwrap_or("Unknown profile"),
            next.identity.as_deref().unwrap_or("Unknown virtual device")
        ),
    );
    match app_state.0.lock() {
        Ok(mut store) => {
            store.settings.last_virtual_profile = Some(profile_id);
            if let Err(error) = store.save_settings() {
                diagnostics::warn("routing.last-profile.save", error.to_string());
            }
        }
        Err(error) => diagnostics::warn("routing.last-profile.lock", error.to_string()),
    }
    Ok(next)
}

#[tauri::command]
fn stop_virtual_routing(
    state: State<'_, VirtualRoutingState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<VirtualRoutingStatus, String> {
    pause_auto_profile_selection(&auto_state, "manual routing stop");
    let _operation = state.operation.lock().map_err(|error| error.to_string())?;
    stop_virtual_routing_inner(&state)?;
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| error.to_string())
}

fn stop_virtual_routing_inner(state: &VirtualRoutingState) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(session) = session {
        diagnostics::info("routing.stop.requested", "Stopping virtual input routing");
        session.stop.store(true, Ordering::SeqCst);
        session
            .worker
            .join()
            .map_err(|_| "The virtual routing worker stopped unexpectedly.".to_owned())?;
        diagnostics::info("routing.stopped", "Virtual input routing stopped cleanly");
    }
    if let Ok(mut status) = state.status.lock() {
        status.active = false;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn run_virtual_routing_worker(
    profile_id: Uuid,
    profile_name: String,
    device_id: u32,
    layout: VirtualLayout,
    mapping: ProfileMapping,
    has_virtual_outputs: bool,
    profile_system_actions: Vec<SystemActionBinding>,
    global_system_actions: Vec<SystemActionBinding>,
    clutch: Option<ClutchMode>,
    mut clutch_axis: Option<VirtualAxis>,
    analog_axes: Vec<RuntimeAnalogAxisMapping>,
    audio_service: audio::AudioService,
    stop: Arc<std::sync::atomic::AtomicBool>,
    status: Arc<Mutex<VirtualRoutingStatus>>,
    ready: std::sync::mpsc::SyncSender<Result<(String, Vec<ActiveEncoderModeSummary>), String>>,
    selection_reason: String,
    updates: std::sync::mpsc::Receiver<VirtualRoutingUpdate>,
) {
    let result = (|| -> Result<(), String> {
        let mut profile_id = profile_id;
        let mut profile_name = profile_name;
        let mut profile_system_actions = profile_system_actions;
        let mut global_system_actions = global_system_actions;
        let mut selection_reason = selection_reason;
        let mut analog_axes = analog_axes;
        let mut listener = DirectInputButtonMonitor::new().map_err(|error| error.to_string())?;
        let mut known_devices = listener
            .device_summaries()
            .into_iter()
            .map(|device| (device.device_id, (device.product_name, device.button_count)))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (name, button_count) in known_devices.values() {
            diagnostics::info(
                "device.connected",
                format!("{name} · {button_count} buttons"),
            );
        }
        let mut engine = if has_virtual_outputs {
            let backend = VJoyBackend::new(device_id).map_err(|error| error.to_string())?;
            Some(
                MappingEngine::connect(backend, layout, mapping)
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let mut clutch = clutch
            .map(ClutchProcessor::new)
            .transpose()
            .map_err(|error| error.to_string())?;
        if let (Some(engine), Some(axis)) = (engine.as_mut(), clutch_axis) {
            engine
                .set_axis_normalized(axis, 0.0)
                .map_err(|error| error.to_string())?;
        }
        if let Some(engine) = engine.as_mut() {
            for mapping in &analog_axes {
                engine
                    .set_axis_normalized(mapping.output, 0.0)
                    .map_err(|error| error.to_string())?;
            }
        }
        let mut held_system_buttons = std::collections::BTreeSet::<(String, u32)>::new();
        let identity = engine
            .as_ref()
            .map(|engine| {
                engine
                    .capabilities()
                    .map(|capabilities| capabilities.identity)
            })
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| "Native system actions".to_owned());
        let encoder_modes = engine
            .as_ref()
            .into_iter()
            .flat_map(|engine| engine.active_encoder_modes())
            .map(|(encoder_name, mode_name)| ActiveEncoderModeSummary {
                encoder_name: encoder_name.to_owned(),
                mode_name: mode_name.to_owned(),
            })
            .collect::<Vec<_>>();
        let _ = ready.send(Ok((identity.clone(), encoder_modes.clone())));
        if let Ok(mut current) = status.lock() {
            *current = VirtualRoutingStatus {
                active: true,
                profile_id: Some(profile_id),
                profile_name: Some(profile_name.clone()),
                identity: Some(identity),
                selection_reason: Some(selection_reason.clone()),
                active_encoder_modes: encoder_modes,
                clutch_output_percent: clutch_axis.map(|_| 0.0),
                clutch_axis: clutch_axis.map(virtual_axis_name).map(str::to_owned),
                ..VirtualRoutingStatus::default()
            };
        }
        while !stop.load(Ordering::SeqCst) {
            while let Ok(update) = updates.try_recv() {
                let switched = (|| -> Result<(String, Vec<ActiveEncoderModeSummary>), String> {
                    let engine = engine.as_mut().ok_or_else(|| {
                        "Start a profile with virtual output before switching in place.".to_owned()
                    })?;
                    if let Some(axis) = clutch_axis {
                        engine
                            .set_axis_normalized(axis, 0.0)
                            .map_err(|error| error.to_string())?;
                    }
                    for mapping in &analog_axes {
                        engine
                            .set_axis_normalized(mapping.output, 0.0)
                            .map_err(|error| error.to_string())?;
                    }
                    engine
                        .set_profile(update.mapping)
                        .map_err(|error| error.to_string())?;
                    clutch = update
                        .clutch
                        .map(ClutchProcessor::new)
                        .transpose()
                        .map_err(|error| error.to_string())?;
                    clutch_axis = update.clutch_axis;
                    analog_axes = update.analog_axes;
                    if let Some(axis) = clutch_axis {
                        engine
                            .set_axis_normalized(axis, 0.0)
                            .map_err(|error| error.to_string())?;
                    }
                    for mapping in &analog_axes {
                        engine
                            .set_axis_normalized(mapping.output, 0.0)
                            .map_err(|error| error.to_string())?;
                    }
                    profile_id = update.profile_id;
                    profile_name = update.profile_name;
                    profile_system_actions = update.profile_system_actions;
                    global_system_actions = update.global_system_actions;
                    selection_reason = update.selection_reason;
                    held_system_buttons.clear();
                    let identity = engine
                        .capabilities()
                        .map_err(|error| error.to_string())?
                        .identity;
                    let modes = engine
                        .active_encoder_modes()
                        .into_iter()
                        .map(|(encoder_name, mode_name)| ActiveEncoderModeSummary {
                            encoder_name: encoder_name.to_owned(),
                            mode_name: mode_name.to_owned(),
                        })
                        .collect::<Vec<_>>();
                    if let Ok(mut current) = status.lock() {
                        *current = VirtualRoutingStatus {
                            active: true,
                            profile_id: Some(profile_id),
                            profile_name: Some(profile_name.clone()),
                            identity: Some(identity.clone()),
                            selection_reason: Some(selection_reason.clone()),
                            active_encoder_modes: modes.clone(),
                            clutch_output_percent: clutch_axis.map(|_| 0.0),
                            clutch_axis: clutch_axis.map(virtual_axis_name).map(str::to_owned),
                            ..VirtualRoutingStatus::default()
                        };
                    }
                    Ok((identity, modes))
                })();
                let _ = update.ready.send(switched);
            }
            for event in listener
                .poll_controls()
                .map_err(|error| error.to_string())?
            {
                let event = match event {
                    DirectInputEvent::Axis(event) => {
                        if let Some(engine) = engine.as_mut() {
                            for mapping in analog_axes.iter().filter(|mapping| {
                                mapping.input.device_id() == event.device_id
                                    && mapping.input.axis_id() == event.axis_id
                            }) {
                                let output = mapping.calibration.normalize(event.value);
                                engine
                                    .set_axis_normalized(mapping.output, output)
                                    .map_err(|error| error.to_string())?;
                                if let Ok(mut current) = status.lock() {
                                    current.routed_event_count += 1;
                                    current.last_event =
                                        Some(format!("{} {:.0}%", mapping.name, output * 100.0));
                                    update_analog_axis_status(&mut current, mapping, output);
                                }
                            }
                        }
                        if let (Some(clutch), Some(engine), Some(axis)) =
                            (clutch.as_mut(), engine.as_mut(), clutch_axis)
                            && let Some(output) = clutch.handle_axis(
                                &PhysicalAxis::new(event.device_id, event.axis_id)
                                    .map_err(|error| error.to_string())?,
                                event.value,
                            )
                        {
                            engine
                                .set_axis_normalized(axis, output)
                                .map_err(|error| error.to_string())?;
                            if let Ok(mut current) = status.lock() {
                                let (primary, bite) = clutch.input_values();
                                current.routed_event_count += 1;
                                current.clutch_output_percent = Some(output * 100.0);
                                current.clutch_primary_input_percent = Some(primary * 100.0);
                                current.clutch_bite_input_percent = bite.map(|value| value * 100.0);
                                current.last_event = Some(format!("Clutch {:.0}%", output * 100.0));
                            }
                        }
                        continue;
                    }
                    DirectInputEvent::Disconnected { device_id, .. } => {
                        if let Some(engine) = engine.as_mut() {
                            for mapping in analog_axes
                                .iter()
                                .filter(|mapping| mapping.input.device_id() == device_id)
                            {
                                engine
                                    .set_axis_normalized(mapping.output, 0.0)
                                    .map_err(|error| error.to_string())?;
                                if let Ok(mut current) = status.lock() {
                                    update_analog_axis_status(&mut current, mapping, 0.0);
                                }
                            }
                        }
                        if let (Some(clutch), Some(engine), Some(axis)) =
                            (clutch.as_mut(), engine.as_mut(), clutch_axis)
                            && let Some(output) = clutch.disconnect_device(&device_id)
                        {
                            engine
                                .set_axis_normalized(axis, output)
                                .map_err(|error| error.to_string())?;
                            if let Ok(mut current) = status.lock() {
                                let (primary, bite) = clutch.input_values();
                                current.clutch_output_percent = Some(output * 100.0);
                                current.clutch_primary_input_percent = Some(primary * 100.0);
                                current.clutch_bite_input_percent = bite.map(|value| value * 100.0);
                            }
                        }
                        continue;
                    }
                    DirectInputEvent::Button(event) => event,
                };
                let system_source = (event.device_id.clone(), event.button_id);
                let changed = update_pressed_system_buttons(
                    &mut held_system_buttons,
                    system_source,
                    event.pressed,
                );
                if changed && event.pressed {
                    for binding in resolve_system_actions(
                        &profile_system_actions,
                        &global_system_actions,
                        &event.device_id,
                        event.button_id,
                        &held_system_buttons,
                    ) {
                        if let Err(error) = audio_service
                            .dispatch(binding.action.clone(), binding.play_feedback_sound)
                        {
                            diagnostics::warn("audio.microphone.dispatch", error);
                        }
                    }
                }
                let source = PhysicalButton::new(event.device_id, event.button_id)
                    .map_err(|error| error.to_string())?;
                if let (Some(clutch), Some(engine), Some(axis)) =
                    (clutch.as_mut(), engine.as_mut(), clutch_axis)
                    && let Some(output) = clutch.handle_button(&source, event.pressed)
                {
                    engine
                        .set_axis_normalized(axis, output)
                        .map_err(|error| error.to_string())?;
                    if let Ok(mut current) = status.lock() {
                        let (primary, bite) = clutch.input_values();
                        current.clutch_output_percent = Some(output * 100.0);
                        current.clutch_primary_input_percent = Some(primary * 100.0);
                        current.clutch_bite_input_percent = bite.map(|value| value * 100.0);
                    }
                }
                let routed = if let Some(engine) = engine.as_mut() {
                    engine
                        .handle_button(&source, event.pressed)
                        .map_err(|error| error.to_string())?
                } else {
                    false
                };
                if routed && let Ok(mut current) = status.lock() {
                    current.routed_event_count += 1;
                    current.last_event = Some(format!(
                        "Button {} {}",
                        event.button_id,
                        if event.pressed { "pressed" } else { "released" }
                    ));
                    current.active_encoder_modes = engine
                        .as_ref()
                        .into_iter()
                        .flat_map(|engine| engine.active_encoder_modes())
                        .map(|(encoder_name, mode_name)| ActiveEncoderModeSummary {
                            encoder_name: encoder_name.to_owned(),
                            mode_name: mode_name.to_owned(),
                        })
                        .collect();
                }
            }
            let current_devices = listener
                .device_summaries()
                .into_iter()
                .map(|device| (device.device_id, (device.product_name, device.button_count)))
                .collect::<std::collections::BTreeMap<_, _>>();
            for (device_id, (name, button_count)) in &current_devices {
                if !known_devices.contains_key(device_id) {
                    diagnostics::info(
                        "device.connected",
                        format!("{name} · {button_count} buttons"),
                    );
                }
            }
            for (device_id, (name, _)) in &known_devices {
                if !current_devices.contains_key(device_id) {
                    diagnostics::warn("device.disconnected", format!("{name} disconnected"));
                }
            }
            known_devices = current_devices;
            if let Some(engine) = engine.as_mut() {
                engine.tick().map_err(|error| error.to_string())?;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if let Some(engine) = engine.as_mut() {
            if let Some(axis) = clutch_axis {
                engine
                    .set_axis_normalized(axis, 0.0)
                    .map_err(|error| error.to_string())?;
            }
            for mapping in &analog_axes {
                engine
                    .set_axis_normalized(mapping.output, 0.0)
                    .map_err(|error| error.to_string())?;
            }
            engine.disconnect().map_err(|error| error.to_string())?;
        }
        Ok(())
    })();

    if let Err(error) = result {
        diagnostics::error("routing.worker.failed", &error);
        let _ = ready.send(Err(error.clone()));
        if let Ok(mut current) = status.lock() {
            current.active = false;
            current.error = Some(error);
        }
    } else if let Ok(mut current) = status.lock() {
        diagnostics::info("routing.worker.stopped", "Routing worker exited cleanly");
        current.active = false;
    }
}

#[cfg(not(target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
fn run_virtual_routing_worker(
    _profile_id: Uuid,
    _profile_name: String,
    _device_id: u32,
    _layout: VirtualLayout,
    _mapping: ProfileMapping,
    _has_virtual_outputs: bool,
    _profile_system_actions: Vec<SystemActionBinding>,
    _global_system_actions: Vec<SystemActionBinding>,
    _clutch: Option<ClutchMode>,
    _clutch_axis: Option<VirtualAxis>,
    _analog_axes: Vec<RuntimeAnalogAxisMapping>,
    _audio_service: audio::AudioService,
    _stop: Arc<std::sync::atomic::AtomicBool>,
    _status: Arc<Mutex<VirtualRoutingStatus>>,
    ready: std::sync::mpsc::SyncSender<Result<(String, Vec<ActiveEncoderModeSummary>), String>>,
    _selection_reason: String,
    _updates: std::sync::mpsc::Receiver<VirtualRoutingUpdate>,
) {
    let _ = ready.send(Err(
        "Virtual routing is only available in the Windows build.".to_owned(),
    ));
}

fn resolve_system_actions<'a>(
    profile: &'a [SystemActionBinding],
    global: &'a [SystemActionBinding],
    device_id: &str,
    physical_button: u32,
    held: &std::collections::BTreeSet<(String, u32)>,
) -> Vec<&'a SystemActionBinding> {
    fn matching<'a>(
        bindings: &'a [SystemActionBinding],
        device_id: &str,
        physical_button: u32,
        held: &std::collections::BTreeSet<(String, u32)>,
    ) -> Vec<&'a SystemActionBinding> {
        let candidates = bindings
            .iter()
            .filter(|binding| {
                binding.device_id == device_id
                    && binding.physical_button == physical_button
                    && binding.modifiers.iter().all(|modifier| {
                        held.contains(&(modifier.device_id.clone(), modifier.physical_button))
                    })
            })
            .collect::<Vec<_>>();
        let Some(specificity) = candidates
            .iter()
            .map(|binding| binding.modifiers.len())
            .max()
        else {
            return Vec::new();
        };
        candidates
            .into_iter()
            .filter(|binding| binding.modifiers.len() == specificity)
            .collect()
    }
    let profile_matches = matching(profile, device_id, physical_button, held);
    if profile_matches.is_empty() {
        matching(global, device_id, physical_button, held)
    } else {
        profile_matches
    }
}

fn update_pressed_system_buttons(
    held: &mut std::collections::BTreeSet<(String, u32)>,
    source: (String, u32),
    pressed: bool,
) -> bool {
    if pressed {
        held.insert(source)
    } else {
        held.remove(&source)
    }
}

#[tauri::command]
async fn start_virtual_binding_setup(
    state: State<'_, AppState>,
    setup_state: State<'_, VirtualSetupState>,
    hotkey_state: State<'_, HotkeyState>,
) -> Result<VirtualSetupStatus, String> {
    diagnostics::info(
        "setup.start.requested",
        "Starting guided virtual binding setup",
    );
    let (device_id, used_buttons, workspace_id, workspace_kind) = {
        let store = state.0.lock().map_err(|error| error.to_string())?;
        let used_buttons = existing_vjoy_buttons(&store)?;
        (
            store.settings.virtual_device_id,
            used_buttons,
            store.active_workspace().id.clone(),
            store.active_workspace().kind,
        )
    };

    let capabilities =
        tauri::async_runtime::spawn_blocking(move || inspect_virtual_device(device_id))
            .await
            .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))??;

    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    if session
        .allocator
        .as_ref()
        .is_some_and(SetupButtonAllocator::is_sending)
    {
        return Err("Wait for the current virtual button pulse to finish.".to_owned());
    }
    let allocator = SetupButtonAllocator::new(capabilities.button_count, used_buttons)
        .map_err(|error| error.to_string())?;
    *session = VirtualSetupSession {
        active: true,
        device_id,
        identity: capabilities.identity.clone(),
        workspace_id,
        workspace_kind,
        pending_action: None,
        allocator: Some(allocator),
    };
    let next_virtual_button = session
        .allocator
        .as_ref()
        .and_then(SetupButtonAllocator::next_button);
    drop(session);

    if hotkey_state
        .0
        .send(HotkeyUpdate::VirtualSetup(true))
        .is_err()
    {
        setup_state
            .0
            .lock()
            .map_err(|error| error.to_string())?
            .active = false;
        return Err("The shortcut worker is not running.".to_owned());
    }

    let session = setup_state.0.lock().map_err(|error| error.to_string())?;
    let allocator = session
        .allocator
        .as_ref()
        .ok_or_else(|| "The virtual binding setup is not initialised.".to_owned())?;
    diagnostics::info(
        "setup.started",
        format!(
            "{} · {} buttons · {} already assigned",
            session.identity,
            allocator.button_count(),
            allocator.used_button_count()
        ),
    );
    Ok(VirtualSetupStatus {
        identity: session.identity.clone(),
        button_count: allocator.button_count(),
        used_button_count: allocator.used_button_count(),
        next_virtual_button,
    })
}

#[tauri::command]
fn stop_virtual_binding_setup(
    setup_state: State<'_, VirtualSetupState>,
    hotkey_state: State<'_, HotkeyState>,
) -> Result<(), String> {
    setup_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .active = false;
    hotkey_state
        .0
        .send(HotkeyUpdate::VirtualSetup(false))
        .map_err(|_| "The shortcut worker is not running.".to_owned())?;
    diagnostics::info("setup.stopped", "Guided virtual binding setup stopped");
    Ok(())
}

#[tauri::command]
fn arm_virtual_binding_action(
    action: String,
    state: State<'_, AppState>,
    setup_state: State<'_, VirtualSetupState>,
) -> Result<(), String> {
    let action = action.trim().to_owned();
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    if !session.active {
        return Err("Start the virtual binding setup first.".to_owned());
    }
    if session.workspace_kind == WorkspaceKind::LeMansUltimate {
        return Err("LMU action names are read from its saved binding file.".to_owned());
    }
    if store.settings.active_workspace_id != session.workspace_id {
        return Err("The active workspace changed. Restart binding setup.".to_owned());
    }
    let button_count = session
        .allocator
        .as_ref()
        .ok_or_else(|| "The virtual binding setup is not initialised.".to_owned())?
        .button_count();
    store
        .next_manual_virtual_button(&action, button_count)
        .map_err(|error| error.to_string())?;
    session.pending_action = Some(action);
    Ok(())
}

#[tauri::command]
async fn send_next_virtual_button(
    state: State<'_, AppState>,
    setup_state: State<'_, VirtualSetupState>,
) -> Result<VirtualButtonPulse, String> {
    let (device_id, virtual_button, action) = reserve_virtual_setup_button(&setup_state)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        pulse_virtual_button(device_id, virtual_button)
    })
    .await
    .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))?;
    let pulse = finish_virtual_setup_pulse(&setup_state, virtual_button, result)?;
    if let Some(action) = action {
        persist_virtual_setup_action(&state, &setup_state, action, virtual_button)?;
    }
    Ok(pulse)
}

fn existing_vjoy_buttons(store: &Store) -> Result<Vec<u32>, String> {
    if store.active_workspace().kind != WorkspaceKind::LeMansUltimate {
        return Ok(store
            .virtual_layout()
            .iter()
            .map(|entry| entry.virtual_button)
            .collect());
    }
    let document = store
        .load_live_bindings_document()
        .map_err(|error| error.to_string())?;
    let devices = document
        .get("Devices")
        .and_then(serde_json::Value::as_object);
    let configured = store
        .settings
        .virtual_device_key
        .as_deref()
        .filter(|key| devices.is_some_and(|entries| entries.contains_key(*key)));
    let detected = devices.and_then(|entries| {
        entries.iter().find_map(|(key, value)| {
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key);
            (name.to_ascii_lowercase().contains("vjoy")
                || key.to_ascii_lowercase().contains("vjoy"))
            .then_some(key.as_str())
        })
    });
    let mut used = match configured.or(detected) {
        Some(device_key) => Store::used_virtual_buttons_from_document(&document, device_key)
            .map_err(|error| error.to_string()),
        None => Ok(Vec::new()),
    }?;
    used.extend(
        store
            .manual_virtual_layout()
            .into_iter()
            .map(|entry| entry.virtual_button),
    );
    used.sort_unstable();
    used.dedup();
    Ok(used)
}

fn reserve_virtual_setup_button(
    setup_state: &State<'_, VirtualSetupState>,
) -> Result<(u32, u32, Option<String>), String> {
    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    if !session.active {
        return Err("Start the virtual binding setup first.".to_owned());
    }
    let action = if session.workspace_kind == WorkspaceKind::LeMansUltimate {
        None
    } else {
        Some(
            session
                .pending_action
                .clone()
                .ok_or_else(|| "Name and arm the action before sending a button.".to_owned())?,
        )
    };
    let virtual_button = session
        .allocator
        .as_mut()
        .ok_or_else(|| "The virtual binding setup is not initialised.".to_owned())?
        .reserve_next()
        .map_err(|error| error.to_string())?;
    Ok((session.device_id, virtual_button, action))
}

fn persist_virtual_setup_action(
    state: &State<'_, AppState>,
    setup_state: &State<'_, VirtualSetupState>,
    action: String,
    virtual_button: u32,
) -> Result<(), String> {
    let workspace_id = setup_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .workspace_id
        .clone();
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    if store.settings.active_workspace_id != workspace_id {
        return Err("The active workspace changed before the action could be saved.".to_owned());
    }
    store
        .add_manual_virtual_action(action.clone(), virtual_button)
        .map_err(|error| error.to_string())?;
    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    if session.pending_action.as_deref() == Some(action.as_str()) {
        session.pending_action = None;
    }
    Ok(())
}

fn finish_virtual_setup_pulse(
    setup_state: &State<'_, VirtualSetupState>,
    virtual_button: u32,
    result: Result<String, String>,
) -> Result<VirtualButtonPulse, String> {
    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    let allocator = session
        .allocator
        .as_mut()
        .ok_or_else(|| "The virtual binding setup is not initialised.".to_owned())?;
    match result {
        Ok(identity) => {
            allocator
                .finish_pulse(virtual_button, true)
                .map_err(|error| error.to_string())?;
            Ok(VirtualButtonPulse {
                virtual_button,
                identity,
                next_virtual_button: allocator.next_button(),
            })
        }
        Err(error) => {
            allocator
                .finish_pulse(virtual_button, false)
                .map_err(|finish_error| finish_error.to_string())?;
            Err(error)
        }
    }
}

#[cfg(target_os = "windows")]
fn inspect_virtual_device(device_id: u32) -> Result<lmukit_virtual::VirtualCapabilities, String> {
    let mut backend = VJoyBackend::new(device_id).map_err(|error| error.to_string())?;
    backend.connect().map_err(|error| error.to_string())?;
    let capabilities = backend.capabilities().map_err(|error| error.to_string())?;
    backend.disconnect().map_err(|error| error.to_string())?;
    Ok(capabilities)
}

#[cfg(not(target_os = "windows"))]
fn inspect_virtual_device(_device_id: u32) -> Result<lmukit_virtual::VirtualCapabilities, String> {
    Err("Virtual button setup is only available on Windows.".to_owned())
}

#[cfg(target_os = "windows")]
fn pulse_virtual_button(device_id: u32, virtual_button: u32) -> Result<String, String> {
    let mut backend = VJoyBackend::new(device_id).map_err(|error| error.to_string())?;
    backend.connect().map_err(|error| error.to_string())?;
    let capabilities = backend.capabilities().map_err(|error| error.to_string())?;
    if virtual_button > capabilities.button_count {
        return Err(format!(
            "vJoy now provides only {} buttons; B{virtual_button} is unavailable.",
            capabilities.button_count
        ));
    }
    backend
        .set_button(virtual_button, true)
        .map_err(|error| error.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    backend
        .set_button(virtual_button, false)
        .map_err(|error| error.to_string())?;
    backend.disconnect().map_err(|error| error.to_string())?;
    Ok(capabilities.identity)
}

#[cfg(not(target_os = "windows"))]
fn pulse_virtual_button(_device_id: u32, _virtual_button: u32) -> Result<String, String> {
    Err("Virtual button setup is only available on Windows.".to_owned())
}

#[cfg(target_os = "windows")]
fn pick_file_dialog(kind: &str, initial_path: Option<&str>) -> Result<Option<String>, String> {
    if kind == "assetto_corsa_folder" {
        let selected_path = initial_path.unwrap_or_default().replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='Choose the Assetto Corsa installation folder'; $d.ShowNewFolderButton=$false; $d.SelectedPath='{selected_path}'; if($d.ShowDialog() -eq 'OK'){{[Console]::Write($d.SelectedPath)}}"
        );
        return run_picker_script(&script);
    }
    let (filter, filename) = match kind {
        "bindings" => ("JSON files (*.json)|*.json", "direct input.json"),
        "settings" => ("JSON files (*.json)|*.json", "Settings.JSON"),
        "executable" => ("Windows applications (*.exe)|*.exe", ""),
        _ => return Err("Unknown file picker type.".to_owned()),
    };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='{filter}'; $d.FileName='{filename}'; if($d.ShowDialog() -eq 'OK'){{[Console]::Write($d.FileName)}}"
    );
    run_picker_script(&script)
}

#[cfg(target_os = "windows")]
fn run_picker_script(script: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!path.is_empty()).then_some(path))
}

#[cfg(not(target_os = "windows"))]
fn pick_file_dialog(_kind: &str, _initial_path: Option<&str>) -> Result<Option<String>, String> {
    Err("File browsing is available in the Windows build.".to_owned())
}

#[tauri::command]
fn activate_profile(
    profile_id: Uuid,
    allow_while_lmu_running: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    ensure_lmu_file_write_allowed(allow_while_lmu_running)?;
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    if store.settings.input_mode != InputMode::Legacy {
        return Err(
            "Switch the global input method to Legacy JSON before preparing LMU's binding file."
                .to_owned(),
        );
    }
    let profile = store
        .profiles()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "That profile no longer exists.".to_owned())?;
    store
        .activate(&profile)
        .map_err(|error| error.to_string())?;
    diagnostics::info(
        "profile.activated",
        format!("Prepared '{}' for the next LMU launch", profile.name),
    );
    Ok(())
}

#[tauri::command]
fn remove_profile_device(
    profile_id: Uuid,
    device_key: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = store
        .profiles()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "That profile no longer exists.".to_owned())?;
    let mut document = store
        .load_profile_document(&profile)
        .map_err(|error| error.to_string())?;
    let removed =
        Store::remove_device(&mut document, &device_key).map_err(|error| error.to_string())?;
    store
        .save_profile_document(&profile, &document)
        .map_err(|error| error.to_string())?;
    Ok(removed)
}

fn find_profile(store: &Store, profile_id: Uuid) -> Result<Profile, String> {
    store
        .profiles()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "That profile no longer exists.".to_owned())
}

#[tauri::command]
fn save_profile_wheel_image(
    profile_id: Uuid,
    image_bytes: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    if let Some(wheel_id) = profile.wheel_id {
        let mut wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
        store
            .save_wheel_image(&mut wheel, &image_bytes)
            .map_err(|error| error.to_string())
    } else {
        let mut profile = profile;
        store
            .save_profile_wheel_image(&mut profile, &image_bytes)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn profile_wheel_image(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<Option<Vec<u8>>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    if let Some(wheel_id) = profile.wheel_id {
        let wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
        store.wheel_image(&wheel).map_err(|error| error.to_string())
    } else {
        store
            .profile_wheel_image(&profile)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn remove_profile_wheel_image(profile_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    if let Some(wheel_id) = profile.wheel_id {
        let mut wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
        store
            .remove_wheel_image(&mut wheel)
            .map_err(|error| error.to_string())
    } else {
        let mut profile = profile;
        store
            .remove_profile_wheel_image(&mut profile)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn create_wheel(
    brand: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> Result<Uuid, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .create_wheel(brand, name)
        .map(|wheel| wheel.id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_wheel(
    wheel_id: Uuid,
    brand: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .update_wheel(wheel_id, brand, name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_wheel(wheel_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .delete_wheel(wheel_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn wheel_image(wheel_id: Uuid, state: State<'_, AppState>) -> Result<Option<Vec<u8>>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
    store.wheel_image(&wheel).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_wheel_library_image(
    wheel_id: Uuid,
    image_bytes: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let mut wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
    store
        .save_wheel_image(&mut wheel, &image_bytes)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_wheel_library_image(wheel_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let mut wheel = store.wheel(wheel_id).map_err(|error| error.to_string())?;
    store
        .remove_wheel_image(&mut wheel)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn assign_profile_wheel(
    profile_id: Uuid,
    wheel_id: Option<Uuid>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .assign_profile_wheel(profile_id, wheel_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn capture_profile(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store.capture(&name).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn import_profile(
    name: String,
    contents: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if contents.len() > 16 * 1024 * 1024 {
        return Err("Preset files must be 16 MB or smaller.".to_owned());
    }
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .import_profile_bytes(&name, &contents)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_profile(profile_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store
        .update_profile(&profile)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn duplicate_profile(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<DuplicatedProfileSummary, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = store
        .duplicate_profile(profile_id)
        .map_err(|error| error.to_string())?;
    Ok(DuplicatedProfileSummary {
        id: profile.id,
        name: profile.name,
    })
}

#[tauri::command]
fn delete_profile(profile_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store.delete(&profile).map_err(|error| error.to_string())
}

#[tauri::command]
fn reveal_profiles(state: State<'_, AppState>) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    reveal_directory(&store.profiles_dir())
}

#[tauri::command]
fn diagnostic_status(
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
) -> Result<DiagnosticStatus, String> {
    build_diagnostic_status(&app_state, &routing_state)
}

#[tauri::command]
fn reveal_logs() -> Result<(), String> {
    let directory = diagnostics::directory()
        .ok_or_else(|| "The logging directory is not available.".to_owned())?;
    reveal_directory(&directory)
}

#[tauri::command]
fn export_diagnostic_report(
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
) -> Result<String, String> {
    let status = build_diagnostic_status(&app_state, &routing_state)?;
    let directory = diagnostics::directory()
        .ok_or_else(|| "The logging directory is not available.".to_owned())?;
    let generated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = directory.join(format!("diagnostic-report-{generated_at}.json"));
    let report = serde_json::json!({
        "generatedAtUnix": generated_at,
        "appVersion": status.app_version,
        "activeProfile": status.active_profile,
        "inputMode": status.input_mode,
        "virtualHealth": status.virtual_health,
        "routing": status.routing,
        "devices": status.devices,
        "recentErrors": status.recent_errors,
        "privacy": "Device instance identifiers and configured filesystem paths are omitted."
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not export the diagnostic report: {error}"))?;
    diagnostics::info(
        "diagnostics.export",
        "Exported a privacy-redacted diagnostic report",
    );
    Ok(path.display().to_string())
}

fn build_diagnostic_status(
    app_state: &AppState,
    routing_state: &VirtualRoutingState,
) -> Result<DiagnosticStatus, String> {
    let (active_profile, input_mode) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        let active_profile = store.settings.active_profile.and_then(|active_id| {
            store
                .profiles()
                .ok()?
                .into_iter()
                .find(|profile| profile.id == active_id)
                .map(|profile| profile.name)
        });
        let input_mode = match store.settings.input_mode {
            lmukit_core::storage::InputMode::Legacy => "Legacy JSON",
            lmukit_core::storage::InputMode::Virtual => "Virtual input",
        };
        (active_profile, input_mode)
    };
    let routing = routing_state
        .status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let virtual_health = if routing.active {
        routing.identity.as_ref().map_or_else(
            || "Connected".to_owned(),
            |identity| format!("Connected · {identity}"),
        )
    } else if let Some(error) = &routing.error {
        format!("Error · {error}")
    } else {
        "Not running".to_owned()
    };
    let log =
        diagnostics::summary().ok_or_else(|| "The logging system is not available.".to_owned())?;
    Ok(DiagnosticStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        log_directory: log.directory,
        current_log_size: log.current_size,
        retained_log_files: log.retained_files,
        log_retention_days: log.retention_days,
        recent_errors: log.recent_errors,
        active_profile,
        input_mode,
        virtual_health,
        routing,
        devices: diagnostic_devices(),
    })
}

#[cfg(target_os = "windows")]
fn diagnostic_devices() -> Vec<DiagnosticDeviceSummary> {
    match lmukit_virtual::direct_input::list_devices() {
        Ok(devices) => devices
            .into_iter()
            .map(|device| DiagnosticDeviceSummary {
                name: device.name,
                product_name: device.product_name,
                button_count: device.button_count,
            })
            .collect(),
        Err(error) => {
            diagnostics::warn("device.enumeration", error.to_string());
            Vec::new()
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn diagnostic_devices() -> Vec<DiagnosticDeviceSummary> {
    Vec::new()
}

#[tauri::command]
fn profile_bindings(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<Vec<BindingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let document = store
        .load_profile_document(&profile)
        .map_err(|error| error.to_string())?;
    Ok(Store::bindings_from_document(&document)
        .into_iter()
        .map(|binding| {
            let device_name = document
                .get("Devices")
                .and_then(|devices| devices.get(&binding.device))
                .and_then(|device| device.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&binding.device)
                .to_owned();
            BindingSummary {
                action: binding.action,
                device_key: binding.device,
                device_name,
                input_id: binding.input_id,
                alternate: binding.alternate,
            }
        })
        .collect())
}

#[tauri::command]
fn load_profile_document(
    profile_id: Uuid,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store
        .load_profile_document(&profile)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_profile_document(
    profile_id: Uuid,
    document: serde_json::Value,
    activate: bool,
    allow_while_lmu_running: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if activate {
        ensure_lmu_file_write_allowed(allow_while_lmu_running)?;
    }
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store
        .save_profile_document(&profile, &document)
        .map_err(|error| error.to_string())?;
    if activate {
        store
            .activate(&profile)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn binding_action_catalogue(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .binding_action_catalogue()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_profile_binding(
    profile_id: Uuid,
    action: String,
    alternate: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let mut document = store
        .load_profile_document(&profile)
        .map_err(|error| error.to_string())?;
    Store::clear_binding(&mut document, &action, alternate).map_err(|error| error.to_string())?;
    store
        .save_profile_document(&profile, &document)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn assign_profile_binding(
    profile_id: Uuid,
    action: String,
    alternate: bool,
    device_key: String,
    input_id: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    store
        .assign_profile_binding(&profile, &action, alternate, &device_key, input_id)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_profile_categories(
    profile_id: Uuid,
    class_tags: Vec<String>,
    custom_tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .set_profile_categories(profile_id, class_tags, custom_tags)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rename_profile(
    profile_id: Uuid,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .rename_profile(profile_id, name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn find_binding_matches(state: State<'_, AppState>) -> Result<BindingLookupResult, String> {
    let pressed = wait_for_controller_input().await?;

    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profiles = store.profiles().map_err(|error| error.to_string())?;
    let matches = store
        .binding_matches(
            &profiles,
            pressed.vendor_id,
            pressed.product_id,
            pressed.input_id,
        )
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|binding| BindingLookupMatch {
            profile_name: binding.profile_name,
            action: binding.action,
            alternate: binding.alternate,
        })
        .collect();
    Ok(BindingLookupResult {
        control: pressed.control,
        input_id: pressed.input_id,
        matches,
    })
}

#[tauri::command]
async fn listen_for_profile_binding(
    profile_id: Uuid,
    action: String,
    alternate: bool,
    state: State<'_, AppState>,
) -> Result<BindingAssignmentCandidate, String> {
    let pressed = wait_for_controller_input().await?;
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = find_profile(&store, profile_id)?;
    let document = store
        .load_profile_document(&profile)
        .map_err(|error| error.to_string())?;
    let device_key = Store::matching_device(&document, pressed.vendor_id, pressed.product_id)
        .ok_or_else(|| {
            "That controller is not registered in this profile. Capture it in LMU first.".to_owned()
        })?;
    let device_name = document
        .get("Devices")
        .and_then(|devices| devices.get(&device_key))
        .and_then(|device| device.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&device_key)
        .to_owned();
    let conflicts = Store::bindings_from_document(&document)
        .into_iter()
        .filter(|binding| {
            binding.device == device_key
                && binding.input_id == pressed.input_id
                && !(binding.action == action && binding.alternate == alternate)
        })
        .map(|binding| BindingConflict {
            action: binding.action,
            alternate: binding.alternate,
        })
        .collect();
    Ok(BindingAssignmentCandidate {
        control: pressed.control,
        input_id: pressed.input_id,
        device_key,
        device_name,
        conflicts,
    })
}

async fn wait_for_controller_input() -> Result<input::PressedInput, String> {
    let generation = LOOKUP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::spawn(move || detect_controller_input(generation))
            .join()
            .map_err(|_| "The controller input worker stopped unexpectedly.".to_owned())?
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn wait_for_direct_input_mapping() -> Result<LearnedPhysicalMapping, String> {
    let generation = LOOKUP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::spawn(move || detect_direct_input_mapping(generation))
            .join()
            .map_err(|_| "The controller input worker stopped unexpectedly.".to_owned())?
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_binding_lookup() {
    LOOKUP_GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn detect_controller_input(generation: u64) -> Result<input::PressedInput, String> {
    let mut listener = input::InputListener::new()?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        if LOOKUP_GENERATION.load(Ordering::SeqCst) != generation {
            return Err("Binding lookup cancelled.".to_owned());
        }
        if let Some(pressed) = listener.poll()? {
            return Ok(pressed);
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    Err("No controller input was detected within 30 seconds.".to_owned())
}

#[cfg(target_os = "windows")]
fn detect_direct_input_mapping(generation: u64) -> Result<LearnedPhysicalMapping, String> {
    let mut listener = DirectInputButtonMonitor::new().map_err(|error| error.to_string())?;
    let mut first = None::<LearnedPhysicalButton>;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        if LOOKUP_GENERATION.load(Ordering::SeqCst) != generation {
            return Err("Button learning cancelled.".to_owned());
        }
        for event in listener.poll().map_err(|error| error.to_string())? {
            if event.device_name.to_lowercase().contains("vjoy") {
                continue;
            }
            if event.pressed {
                if first.as_ref().is_some_and(|button| {
                    button.device_id == event.device_id && button.button_id == event.button_id
                }) {
                    continue;
                }
                let pressed = LearnedPhysicalButton {
                    device_id: event.device_id,
                    device_name: event.device_name,
                    button_id: event.button_id,
                };
                if let Some(modifier) = first.take() {
                    return Ok(LearnedPhysicalMapping::Modified {
                        modifier,
                        trigger: pressed,
                    });
                }
                first = Some(pressed);
            } else if first.as_ref().is_some_and(|button| {
                button.device_id == event.device_id && button.button_id == event.button_id
            }) {
                return Ok(LearnedPhysicalMapping::Button(
                    first.take().expect("the released first button exists"),
                ));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    Err("No controller button was detected within 30 seconds.".to_owned())
}

#[cfg(target_os = "windows")]
fn detect_clutch_input(
    generation: u64,
    allow_button: bool,
) -> Result<ClutchInputCaptureSummary, String> {
    let mut listener = DirectInputButtonMonitor::new().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let mut axis_started = None::<std::time::Instant>;
    let mut axes = std::collections::BTreeMap::<(String, u32), (String, i32, i32)>::new();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        if LOOKUP_GENERATION.load(Ordering::SeqCst) != generation {
            return Err("Clutch input learning cancelled.".to_owned());
        }
        for event in listener
            .poll_controls()
            .map_err(|error| error.to_string())?
        {
            match event {
                DirectInputEvent::Button(event)
                    if allow_button
                        && event.pressed
                        && !event.device_name.to_lowercase().contains("vjoy") =>
                {
                    return Ok(ClutchInputCaptureSummary::Button {
                        device_id: event.device_id,
                        device_name: event.device_name,
                        physical_button: event.button_id,
                    });
                }
                DirectInputEvent::Axis(event)
                    if !event.device_name.to_lowercase().contains("vjoy") =>
                {
                    axis_started.get_or_insert_with(std::time::Instant::now);
                    let entry = axes.entry((event.device_id, event.axis_id)).or_insert((
                        event.device_name,
                        event.value,
                        event.value,
                    ));
                    entry.1 = entry.1.min(event.value);
                    entry.2 = entry.2.max(event.value);
                }
                DirectInputEvent::Button(_)
                | DirectInputEvent::Axis(_)
                | DirectInputEvent::Disconnected { .. } => {}
            }
        }
        if axis_started
            .is_some_and(|started| started.elapsed() >= std::time::Duration::from_secs(3))
        {
            if let Some(((device_id, axis_id), (device_name, minimum, maximum))) = axes
                .iter()
                .max_by_key(|(_, (_, minimum, maximum))| i64::from(*maximum) - i64::from(*minimum))
                .filter(|(_, (_, minimum, maximum))| {
                    i64::from(*maximum) - i64::from(*minimum) >= 32
                })
            {
                return Ok(ClutchInputCaptureSummary::Axis {
                    device_id: device_id.clone(),
                    device_name: device_name.clone(),
                    physical_axis: *axis_id,
                    minimum: *minimum,
                    maximum: *maximum,
                });
            }
            axis_started = None;
            axes.clear();
        }
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    Err("No clutch input was detected within 30 seconds.".to_owned())
}

#[cfg(not(target_os = "windows"))]
fn detect_direct_input_mapping(_generation: u64) -> Result<LearnedPhysicalMapping, String> {
    Err("DirectInput button learning is only available in the Windows build.".to_owned())
}

#[cfg(not(target_os = "windows"))]
fn detect_clutch_input(
    _generation: u64,
    _allow_button: bool,
) -> Result<ClutchInputCaptureSummary, String> {
    Err("Clutch input learning is only available in the Windows build.".to_owned())
}

fn virtual_axis_name(axis: VirtualAxis) -> &'static str {
    match axis {
        VirtualAxis::X => "x",
        VirtualAxis::Y => "y",
        VirtualAxis::Z => "z",
        VirtualAxis::Rx => "rx",
        VirtualAxis::Ry => "ry",
        VirtualAxis::Rz => "rz",
        VirtualAxis::Slider0 => "slider0",
        VirtualAxis::Slider1 => "slider1",
        VirtualAxis::Wheel => "wheel",
    }
}

#[cfg(target_os = "windows")]
fn configured_virtual_axes(device_id: u32) -> Vec<String> {
    VJoyBackend::configured_axes(device_id)
        .unwrap_or_default()
        .keys()
        .copied()
        .map(virtual_axis_name)
        .map(str::to_owned)
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn configured_virtual_axes(_device_id: u32) -> Vec<String> {
    VirtualAxis::ALL
        .into_iter()
        .map(virtual_axis_name)
        .map(str::to_owned)
        .collect()
}

fn runtime_virtual_axis(axis: VirtualAxisId) -> VirtualAxis {
    match axis {
        VirtualAxisId::X => VirtualAxis::X,
        VirtualAxisId::Y => VirtualAxis::Y,
        VirtualAxisId::Z => VirtualAxis::Z,
        VirtualAxisId::Rx => VirtualAxis::Rx,
        VirtualAxisId::Ry => VirtualAxis::Ry,
        VirtualAxisId::Rz => VirtualAxis::Rz,
        VirtualAxisId::Slider0 => VirtualAxis::Slider0,
        VirtualAxisId::Slider1 => VirtualAxis::Slider1,
        VirtualAxisId::Wheel => VirtualAxis::Wheel,
    }
}

fn runtime_analog_axis(
    value: &VirtualAnalogAxisMapping,
) -> Result<RuntimeAnalogAxisMapping, String> {
    Ok(RuntimeAnalogAxisMapping {
        name: value.name.clone(),
        input: PhysicalAxis::new(value.input.device_id.clone(), value.input.physical_axis)
            .map_err(|error| error.to_string())?,
        calibration: AxisCalibration::new(
            value.input.calibration.minimum,
            value.input.calibration.maximum,
            value.input.calibration.inverted,
            f64::from(value.input.calibration.released_deadzone_percent) / 100.0,
            f64::from(value.input.calibration.engaged_deadzone_percent) / 100.0,
        )
        .map_err(|error| error.to_string())?,
        output: runtime_virtual_axis(value.virtual_axis),
    })
}

fn update_analog_axis_status(
    status: &mut VirtualRoutingStatus,
    mapping: &RuntimeAnalogAxisMapping,
    output: f64,
) {
    let summary = AnalogAxisOutputSummary {
        name: mapping.name.clone(),
        input_percent: output * 100.0,
        output_percent: output * 100.0,
        virtual_axis: virtual_axis_name(mapping.output).to_owned(),
    };
    if let Some(current) = status
        .analog_axis_outputs
        .iter_mut()
        .find(|current| current.name == mapping.name)
    {
        *current = summary;
    } else {
        status.analog_axis_outputs.push(summary);
    }
}

fn runtime_clutch_mode(
    mapping: &VirtualClutchMapping,
    workspace: &WorkspaceClutchSettings,
) -> Result<ClutchMode, String> {
    fn calibration(value: &VirtualAxisCalibration) -> Result<AxisCalibration, String> {
        AxisCalibration::new(
            value.minimum,
            value.maximum,
            value.inverted,
            f64::from(value.released_deadzone_percent) / 100.0,
            f64::from(value.engaged_deadzone_percent) / 100.0,
        )
        .map_err(|error| error.to_string())
    }
    fn axis(value: &VirtualAxisInput) -> Result<ClutchInput, String> {
        Ok(ClutchInput::axis(
            PhysicalAxis::new(value.device_id.clone(), value.physical_axis)
                .map_err(|error| error.to_string())?,
            calibration(&value.calibration)?,
        ))
    }
    fn input(value: &VirtualClutchInput) -> Result<ClutchInput, String> {
        match value {
            VirtualClutchInput::Button {
                device_id,
                physical_button,
                ..
            } => Ok(ClutchInput::button(
                PhysicalButton::new(device_id.clone(), *physical_button)
                    .map_err(|error| error.to_string())?,
            )),
            VirtualClutchInput::Axis(value) => axis(value),
        }
    }
    let bite = |profile: u8, shared: bool| {
        f64::from(if shared {
            workspace.shared_bite_point_percent
        } else {
            profile
        }) / 100.0
    };
    match mapping {
        VirtualClutchMapping::Direct { input: value } => Ok(ClutchMode::Direct {
            input: axis(value)?,
        }),
        VirtualClutchMapping::Dual {
            full,
            bite: bite_input,
            bite_point_percent,
            use_workspace_bite_point,
        } => Ok(ClutchMode::Dual {
            full: input(full)?,
            bite: input(bite_input)?,
            bite_point: bite(*bite_point_percent, *use_workspace_bite_point),
        }),
        VirtualClutchMapping::Combined {
            input: value,
            firmware_bite_point_percent,
            bite_point_percent,
            use_workspace_bite_point,
        } => Ok(ClutchMode::Combined {
            input: axis(value)?,
            firmware_bite_point: firmware_bite_point_percent.map(|value| f64::from(value) / 100.0),
            bite_point: bite(*bite_point_percent, *use_workspace_bite_point),
        }),
    }
}

#[cfg(target_os = "windows")]
fn reveal_directory(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open {}: {error}", path.display()))
}

#[cfg(not(target_os = "windows"))]
fn reveal_directory(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open {}: {error}", path.display()))
}

fn profile_summary(store: &Store, profile: &Profile) -> Result<ProfileSummary, String> {
    let document = store
        .load_profile_document(profile)
        .map_err(|error| error.to_string())?;
    let bindings = Store::bindings_from_document(&document);
    let mut devices = document
        .get("Devices")
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| DeviceSummary {
                    key: key.clone(),
                    name: value
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(key)
                        .to_owned(),
                    binding_count: bindings
                        .iter()
                        .filter(|binding| binding.device == *key)
                        .count(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    devices.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    let wheel = profile.wheel_id.and_then(|id| store.wheel(id).ok());
    Ok(ProfileSummary {
        id: profile.id,
        name: profile.name.clone(),
        hotkey_slot: profile.hotkey_slot,
        binding_count: bindings.len(),
        devices,
        has_wheel_image: wheel.as_ref().map_or_else(
            || profile.wheel_image.is_some(),
            |wheel| wheel.image.is_some(),
        ),
        wheel_tags: profile.wheel_tags.clone(),
        wheel_brand: wheel
            .as_ref()
            .and_then(|wheel| wheel.brand.clone())
            .or_else(|| profile.wheel_brand.clone()),
        wheel_name: wheel
            .as_ref()
            .map(|wheel| wheel.name.clone())
            .or_else(|| profile.wheel_name.clone())
            .or_else(|| profile.wheel_tags.first().cloned()),
        class_tags: profile.class_tags.clone(),
        custom_tags: profile.custom_tags.clone(),
        differs_from_live: store
            .profile_has_live_changes(profile)
            .map_err(|error| error.to_string())?,
        wheel_id: profile.wheel_id,
    })
}

fn wheel_summary(wheel: Wheel, profiles: &[Profile]) -> WheelSummary {
    let profile_count = profiles
        .iter()
        .filter(|profile| profile.wheel_id == Some(wheel.id))
        .count();
    WheelSummary {
        id: wheel.id,
        brand: wheel.brand,
        name: wheel.name,
        has_image: wheel.image.is_some(),
        profile_count,
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn request_exit_confirmation(app: &tauri::AppHandle) {
    show_main_window(app);
    let _ = app.emit("request-exit-confirmation", ());
}

fn tray_tooltip(app: &tauri::AppHandle) -> String {
    let workspace = app
        .state::<AppState>()
        .0
        .lock()
        .map(|store| store.active_workspace().name.clone())
        .unwrap_or_else(|_| "Workspace unavailable".to_owned());
    let routing = app
        .state::<VirtualRoutingState>()
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default();
    if routing.active {
        format!(
            "Pivot — {workspace} — {}",
            routing
                .profile_name
                .as_deref()
                .unwrap_or("Remapping active")
        )
    } else {
        format!("Pivot — {workspace} — Remapping off")
    }
}

fn tray_menu_presentation(app: &tauri::AppHandle) -> TrayMenuPresentation {
    let (workspace, input_mode, auto_enabled, can_start, can_switch) = app
        .state::<AppState>()
        .0
        .lock()
        .map(|store| {
            let profiles = store.active_workspace_profiles().unwrap_or_default();
            let can_start = store.settings.input_mode == InputMode::Virtual
                && store
                    .settings
                    .last_virtual_profile
                    .is_some_and(|profile_id| {
                        profiles.iter().any(|profile| profile.id == profile_id)
                    });
            (
                store.active_workspace().name.clone(),
                store.settings.input_mode,
                store.settings.auto_profile_selection,
                can_start,
                !profiles.is_empty(),
            )
        })
        .unwrap_or_else(|_| {
            (
                "Workspace unavailable".to_owned(),
                InputMode::Legacy,
                false,
                false,
                false,
            )
        });
    let routing = app
        .state::<VirtualRoutingState>()
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default();
    let auto_override_active = app
        .state::<AutoProfileState>()
        .0
        .lock()
        .is_ok_and(|runtime| runtime.paused || runtime.manual_override_context.is_some());
    let can_resume_auto =
        routing.active && input_mode == InputMode::Virtual && auto_enabled && auto_override_active;
    build_tray_menu_presentation(
        &workspace,
        input_mode,
        can_start,
        can_switch,
        can_resume_auto,
        &routing,
    )
}

fn build_tray_menu_presentation(
    workspace: &str,
    input_mode: InputMode,
    can_start: bool,
    can_switch: bool,
    can_resume_auto: bool,
    routing: &VirtualRoutingStatus,
) -> TrayMenuPresentation {
    let workspace = workspace.replace('&', "&&");
    TrayMenuPresentation {
        status: if routing.active {
            format!(
                "{workspace} · {}",
                routing
                    .profile_name
                    .as_deref()
                    .unwrap_or("Remapping active")
                    .replace('&', "&&")
            )
        } else if input_mode == InputMode::Virtual {
            format!("{workspace} · Remapping off")
        } else {
            format!("{workspace} · Legacy JSON")
        },
        routing_toggle_text: if routing.active {
            "Stop remapping"
        } else {
            "Start remapping"
        },
        routing_toggle_enabled: routing.active || can_start,
        switch_profile_enabled: can_switch,
        resume_auto_enabled: can_resume_auto,
    }
}

fn refresh_tray_status(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("lmukit") {
        let _ = tray.set_tooltip(Some(tray_tooltip(app)));
    }
    let Some(menu) = app.try_state::<TrayMenuState>() else {
        return;
    };
    let presentation = tray_menu_presentation(app);
    let unchanged = menu
        .presentation
        .lock()
        .is_ok_and(|current| current.as_ref() == Some(&presentation));
    if unchanged {
        return;
    }
    let updated = menu.status.set_text(&presentation.status).is_ok()
        && menu
            .routing_toggle
            .set_text(presentation.routing_toggle_text)
            .is_ok()
        && menu
            .routing_toggle
            .set_enabled(presentation.routing_toggle_enabled)
            .is_ok()
        && menu
            .switch_profile
            .set_enabled(presentation.switch_profile_enabled)
            .is_ok()
        && menu
            .resume_auto
            .set_enabled(presentation.resume_auto_enabled)
            .is_ok();
    if updated && let Ok(mut current) = menu.presentation.lock() {
        *current = Some(presentation);
    }
}

fn stop_routing_from_tray(app: tauri::AppHandle) {
    let result = (|| {
        let auto_state = app.state::<AutoProfileState>();
        pause_auto_profile_selection(&auto_state, "tray routing stop");
        let state = app.state::<VirtualRoutingState>();
        let _operation = state.operation.lock().map_err(|error| error.to_string())?;
        let was_active = state
            .status
            .lock()
            .map_err(|error| error.to_string())?
            .active;
        stop_virtual_routing_inner(&state)?;
        Ok::<_, String>(if was_active {
            "Live remapping stopped from the notification area."
        } else {
            "Live remapping was already stopped."
        })
    })();
    refresh_tray_status(&app);
    let _ = app.emit("tray-routing-result", result);
}

fn start_routing_from_tray(app: tauri::AppHandle) {
    let result = (|| {
        let profile_id = {
            let state = app.state::<AppState>();
            let store = state.0.lock().map_err(|error| error.to_string())?;
            if store.settings.input_mode != InputMode::Virtual {
                return Err("Switch Pivot to Virtual Input before starting remapping.".to_owned());
            }
            let profile_id = store.settings.last_virtual_profile.ok_or_else(|| {
                "Choose a mapped profile in Pivot before starting from the notification area."
                    .to_owned()
            })?;
            store
                .active_workspace_profiles()
                .map_err(|error| error.to_string())?
                .iter()
                .any(|profile| profile.id == profile_id)
                .then_some(profile_id)
                .ok_or_else(|| {
                    "The last live profile belongs to another workspace. Choose a profile first."
                        .to_owned()
                })?
        };
        let app_state = app.state::<AppState>();
        let routing_state = app.state::<VirtualRoutingState>();
        let status = start_virtual_routing_inner(
            profile_id,
            "Started from the notification area".to_owned(),
            &app_state,
            &routing_state,
        )?;
        hold_manual_profile_for_active_context(
            &app_state,
            &app.state::<AutoProfileState>(),
            &app.state::<CarContextState>(),
            &app.state::<AcContextState>(),
        )?;
        Ok::<_, String>(format!(
            "{} is now remapping live.",
            status.profile_name.as_deref().unwrap_or("The last profile")
        ))
    })();
    refresh_tray_status(&app);
    let _ = app.emit("tray-routing-result", result);
}

fn toggle_routing_from_tray(app: tauri::AppHandle) {
    let active = app
        .state::<VirtualRoutingState>()
        .status
        .lock()
        .is_ok_and(|status| status.active);
    if active {
        stop_routing_from_tray(app);
    } else {
        start_routing_from_tray(app);
    }
}

fn resume_auto_from_tray(app: tauri::AppHandle) {
    let result = (|| {
        let routing_active = app
            .state::<VirtualRoutingState>()
            .status
            .lock()
            .map_err(|error| error.to_string())?
            .active;
        if !routing_active {
            return Err("Start remapping before resuming automatic profile selection.".to_owned());
        }
        resume_auto_profile_selection_inner(
            &app.state::<AppState>(),
            &app.state::<AutoProfileState>(),
        )?;
        Ok::<_, String>("Automatic profile selection resumed.".to_owned())
    })();
    refresh_tray_status(&app);
    let _ = app.emit("tray-routing-result", result);
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let status = MenuItem::with_id(
        app,
        "routing-status",
        "Workspace · Remapping off",
        false,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, "show-lmukit", "Show Pivot", true, None::<&str>)?;
    let routing_toggle = MenuItem::with_id(
        app,
        "toggle-routing",
        "Start remapping",
        false,
        None::<&str>,
    )?;
    let switch_profile = MenuItem::with_id(
        app,
        "switch-profile",
        "Switch profile…",
        false,
        None::<&str>,
    )?;
    let resume_auto = MenuItem::with_id(
        app,
        "resume-auto",
        "Resume automatic selection",
        false,
        None::<&str>,
    )?;
    let check_updates = MenuItem::with_id(
        app,
        "check-updates",
        "Check for updates…",
        true,
        None::<&str>,
    )?;
    let exit = MenuItem::with_id(app, "exit-lmukit", "Exit Pivot", true, None::<&str>)?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let separator_three = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &separator_one,
            &show,
            &routing_toggle,
            &switch_profile,
            &resume_auto,
            &separator_two,
            &check_updates,
            &separator_three,
            &exit,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("lmukit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Pivot")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-lmukit" => show_main_window(app),
            "toggle-routing" => {
                let app = app.clone();
                std::thread::spawn(move || toggle_routing_from_tray(app));
            }
            "switch-profile" => {
                show_main_window(app);
                let _ = app.emit("open-profile-switcher", ());
            }
            "resume-auto" => {
                let app = app.clone();
                std::thread::spawn(move || resume_auto_from_tray(app));
            }
            "check-updates" => {
                show_main_window(app);
                let _ = app.emit("open-update-settings", ());
            }
            "exit-lmukit" => request_exit_confirmation(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    app.manage(TrayMenuState {
        status,
        routing_toggle,
        switch_profile,
        resume_auto,
        presentation: Mutex::new(None),
    });
    refresh_tray_status(app.handle());
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Store::open().expect("Pivot data store should open");
    if let Err(error) =
        diagnostics::initialise(&store.data_dir(), store.settings.log_retention_days)
    {
        eprintln!("Pivot logging could not start: {error}");
    }
    match std::env::current_exe() {
        Ok(executable) => match store.refresh_companion_launcher_if_exists(&executable) {
            Ok(true) => diagnostics::info(
                "launcher.refreshed",
                "Refreshed the existing Steam launcher for this Pivot installation",
            ),
            Ok(false) => {}
            Err(error) => diagnostics::warn(
                "launcher.refresh.failed",
                format!("Could not refresh the existing Steam launcher: {error}"),
            ),
        },
        Err(error) => diagnostics::warn(
            "launcher.refresh.failed",
            format!("Could not locate Pivot's executable for the Steam launcher: {error}"),
        ),
    }
    let shortcuts = store
        .active_workspace_profiles()
        .map(|profiles| profile_shortcuts(&profiles))
        .unwrap_or_default();
    let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
    let car_context_status = Arc::new(Mutex::new(CarContextSummary::default()));
    let ac_context_status = Arc::new(Mutex::new(AcContextSummary::default()));
    let car_context_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(target_os = "windows")]
    let context_worker_status = Arc::clone(&car_context_status);
    #[cfg(target_os = "windows")]
    let ac_context_worker_status = Arc::clone(&ac_context_status);
    #[cfg(target_os = "windows")]
    let context_worker_stop = Arc::clone(&car_context_stop);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            diagnostics::info(
                "app.second-instance",
                "A second launch restored the existing Pivot window",
            );
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState(Mutex::new(store)))
        .manage(AudioState(audio::service()))
        .manage(HotkeyState(hotkey_tx))
        .manage(VirtualSetupState(
            Mutex::new(VirtualSetupSession::default()),
        ))
        .manage(ManualPulseState(Arc::new(AtomicU64::new(0))))
        .manage(VirtualRoutingState {
            operation: Mutex::new(()),
            session: Mutex::new(None),
            status: Arc::new(Mutex::new(VirtualRoutingStatus::default())),
        })
        .manage(AutoProfileState(Mutex::new(AutoProfileRuntime::default())))
        .manage(CarContextState {
            status: car_context_status,
            stop: car_context_stop,
        })
        .manage(AcContextState {
            status: ac_context_status,
        })
        .setup(move |app| {
            setup_tray(app)?;
            let audio_handle = app.handle().clone();
            app.state::<AudioState>().0.set_change_notifier(move || {
                let _ = audio_handle.emit("audio-devices-changed", ());
            });
            let handle = app.handle().clone();
            std::thread::spawn(move || poll_hotkeys(handle, hotkey_rx, shortcuts));
            let handle = app.handle().clone();
            let tray_stop = Arc::clone(&handle.state::<CarContextState>().stop);
            std::thread::spawn(move || {
                while !tray_stop.load(Ordering::Acquire) {
                    refresh_tray_status(&handle);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
            #[cfg(target_os = "windows")]
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    poll_game_car_context(
                        handle,
                        context_worker_status,
                        ac_context_worker_status,
                        context_worker_stop,
                    )
                });
            }
            let auto_start_profile = {
                let state = app.state::<AppState>();
                state.0.lock().ok().and_then(|store| {
                    let profile_id = (store.settings.input_mode == InputMode::Virtual
                        && store.settings.auto_start_virtual_profile)
                        .then_some(store.settings.last_virtual_profile)
                        .flatten()?;
                    store
                        .active_workspace_profiles()
                        .ok()?
                        .iter()
                        .any(|profile| profile.id == profile_id)
                        .then_some(profile_id)
                })
            };
            if let Some(profile_id) = auto_start_profile {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let app_state = handle.state::<AppState>();
                    let routing_state = handle.state::<VirtualRoutingState>();
                    match start_virtual_routing_inner(
                        profile_id,
                        "Restored after launch".to_owned(),
                        &app_state,
                        &routing_state,
                    ) {
                        Ok(status) => {
                            let _ = handle.emit(
                                "virtual-autostart-result",
                                format!(
                                    "{} is now remapping live.",
                                    status.profile_name.as_deref().unwrap_or("The last profile")
                                ),
                            );
                        }
                        Err(error) => {
                            diagnostics::warn("routing.autostart.failed", &error);
                            if let Ok(mut status) = routing_state.status.lock() {
                                status.error = Some(format!(
                                    "Could not restore the last live profile: {error}"
                                ));
                            }
                            let _ = handle.emit(
                                "virtual-autostart-result",
                                format!("Could not restore live remapping: {error}"),
                            );
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            set_active_workspace,
            activate_profile,
            remove_profile_device,
            save_profile_wheel_image,
            profile_wheel_image,
            remove_profile_wheel_image,
            create_wheel,
            update_wheel,
            delete_wheel,
            wheel_image,
            save_wheel_library_image,
            remove_wheel_library_image,
            assign_profile_wheel,
            capture_profile,
            create_virtual_profile,
            import_profile,
            update_profile,
            duplicate_profile,
            delete_profile,
            reveal_profiles,
            diagnostic_status,
            reveal_logs,
            export_diagnostic_report,
            profile_bindings,
            load_profile_document,
            save_profile_document,
            binding_action_catalogue,
            clear_profile_binding,
            assign_profile_binding,
            set_profile_categories,
            rename_profile,
            find_binding_matches,
            listen_for_profile_binding,
            cancel_binding_lookup,
            set_ui_font_scale,
            set_assetto_corsa_path,
            set_input_mode,
            set_virtual_auto_start,
            set_log_retention_days,
            exit_app,
            prepare_for_update,
            save_app_settings,
            set_profile_hotkey,
            companion_launch_option,
            validate_companion_launch_setup,
            pick_file,
            load_game_settings,
            save_game_settings,
            lmu_runtime_status,
            virtual_binding_devices,
            preview_virtual_layout,
            import_virtual_layout,
            virtual_profile_mapping,
            virtual_profile_clutch,
            save_virtual_profile_clutch,
            save_virtual_profile_analog_axes,
            save_workspace_clutch_settings,
            test_virtual_clutch_axis,
            learn_clutch_input,
            system_action_bindings,
            capture_system_action_control,
            save_system_action_binding,
            remove_system_action_binding,
            capture_audio_devices,
            microphone_mute_state,
            test_microphone_action,
            saved_virtual_layout,
            import_profile_virtual_mapping,
            virtual_profile_defaults,
            manual_virtual_actions,
            create_manual_virtual_action,
            test_manual_virtual_action,
            cancel_manual_virtual_action_test,
            remove_manual_virtual_action,
            set_virtual_profile_default,
            clear_virtual_profile_default,
            learn_virtual_encoder_control,
            virtual_profile_encoder_groups,
            save_virtual_profile_encoder_group,
            remove_virtual_profile_encoder_group,
            capture_virtual_profile_button,
            save_virtual_profile_button,
            inspect_virtual_profile_button,
            remove_virtual_profile_button,
            virtual_routing_status,
            startup_health,
            virtual_profile_readiness,
            lmu_car_context_status,
            ac_car_context_status,
            auto_profile_selection_status,
            set_auto_profile_selection,
            resume_auto_profile_selection,
            start_virtual_routing,
            start_default_virtual_routing,
            stop_virtual_routing,
            start_virtual_binding_setup,
            stop_virtual_binding_setup,
            arm_virtual_binding_action,
            send_next_virtual_button
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pivot");
    app.run(|app_handle, event| match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window(&label) {
                let _ = window.hide();
            }
        }
        tauri::RunEvent::ExitRequested { .. } => {
            diagnostics::info("app.exit", "Application exit requested");
            let state = app_handle.state::<VirtualRoutingState>();
            if let Ok(_operation) = state.operation.lock() {
                let _ = stop_virtual_routing_inner(&state);
            }
            app_handle
                .state::<CarContextState>()
                .stop
                .store(true, Ordering::Release);
        }
        _ => {}
    });
}

fn profile_shortcuts(profiles: &[Profile]) -> Vec<(Uuid, u8)> {
    profiles
        .iter()
        .filter_map(|profile| profile.hotkey_slot.map(|slot| (profile.id, slot)))
        .collect()
}

#[cfg(target_os = "windows")]
fn poll_game_car_context(
    app: tauri::AppHandle,
    lmu_status: Arc<Mutex<CarContextSummary>>,
    ac_status: Arc<Mutex<AcContextSummary>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    use lmukit_context::windows::{WindowsAcCarContextProvider, WindowsLmuCarContextProvider};
    use lmukit_core::car_context::LmuCarContextProvider;

    while !stop.load(Ordering::Acquire) {
        let workspace = app.state::<AppState>().0.lock().ok().map(|store| {
            (
                store.active_workspace().kind,
                store.settings.assetto_corsa_path.clone(),
            )
        });
        match workspace {
            Some((WorkspaceKind::AssettoCorsa, assetto_corsa_path)) => {
                let observation = WindowsAcCarContextProvider::connect()
                    .map(|provider| provider.current_context(&assetto_corsa_path));
                let summary = if let Ok(current) = ac_status.lock() {
                    match observation {
                        Ok(Ok(context)) => AcContextSummary::observed(context),
                        Ok(Err(_)) => AcContextSummary::metadata_unavailable(&current),
                        Err(_) => AcContextSummary::unavailable(&current),
                    }
                } else {
                    AcContextSummary::default()
                };
                if let Ok(mut current) = ac_status.lock() {
                    *current = summary.clone();
                }
                apply_ac_automatic_profile_selection(&app, &summary);
            }
            Some((WorkspaceKind::LeMansUltimate, _)) => {
                // Reopen the game-owned mapping for every sample so shutdown
                // cannot leave a stale snapshot apparently connected.
                let observation = WindowsLmuCarContextProvider::connect()
                    .and_then(|mut provider| provider.current_context());
                let summary = if let Ok(current) = lmu_status.lock() {
                    match observation {
                        Ok(context) => CarContextSummary::observed(context),
                        Err(_) => CarContextSummary::unavailable(&current),
                    }
                } else {
                    CarContextSummary::default()
                };
                if let Ok(mut current) = lmu_status.lock() {
                    *current = summary.clone();
                }
                apply_automatic_profile_selection(&app, &summary);
            }
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(1_500));
    }
}

#[cfg(target_os = "windows")]
fn apply_ac_automatic_profile_selection(app: &tauri::AppHandle, context: &AcContextSummary) {
    let app_state = app.state::<AppState>();
    let (enabled, input_mode) = match app_state.0.lock() {
        Ok(store) => (
            store.settings.auto_profile_selection,
            store.settings.input_mode,
        ),
        Err(_) => return,
    };
    let auto_state = app.state::<AutoProfileState>();
    let stable_car = {
        let Ok(mut runtime) = auto_state.0.lock() else {
            return;
        };
        if !enabled {
            runtime.reset("Automatic profile selection is off.");
            return;
        }
        if input_mode != InputMode::Virtual {
            runtime.reset("Automatic profile selection waits for Virtual Input mode.");
            return;
        }
        if runtime.paused {
            runtime.debouncer.reset();
            runtime.last_attempted_profile = None;
            return;
        }
        let Some(car_id) = context.car_id.as_deref().filter(|_| context.connected) else {
            if runtime.manual_override_context.is_none() {
                runtime.reset("Holding the current profile until Assetto Corsa confirms a car.");
            }
            return;
        };
        if let Some(override_car) = runtime.manual_override_context.clone() {
            if override_car.eq_ignore_ascii_case(car_id) {
                runtime.debouncer.reset();
                runtime.last_attempted_profile = None;
                return;
            }
            runtime.manual_override_context = None;
            runtime.reset("Assetto Corsa changed cars. Resuming automatic selection.");
        }
        if !runtime
            .debouncer
            .candidate()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(car_id))
        {
            runtime.last_attempted_profile = None;
        }
        let stable = runtime.debouncer.observe(Some(car_id)).map(str::to_owned);
        if stable.is_none() {
            runtime.message = format!(
                "Confirming {} ({}/{})…",
                context.car_name.as_deref().unwrap_or(car_id),
                runtime.debouncer.observations(),
                runtime.debouncer.required_observations()
            );
        }
        stable
    };
    let Some(car_id) = stable_car else {
        return;
    };
    let resolved = (|| -> Result<(Profile, String), String> {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        let profiles = store
            .active_workspace_profiles()
            .map_err(|error| error.to_string())?;
        let mut matches = store
            .settings
            .class_profile_defaults
            .iter()
            .filter(|entry| entry.workspace_id == store.settings.active_workspace_id)
            .filter(|entry| {
                context
                    .tags
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case(&entry.class_tag))
            })
            .filter_map(|entry| {
                profiles
                    .iter()
                    .find(|profile| profile.id == entry.profile_id)
                    .cloned()
                    .map(|profile| (profile, entry.class_tag.clone()))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(profile, _)| profile.id);
        matches.dedup_by_key(|(profile, _)| profile.id);
        match matches.len() {
            0 => Err(format!(
                "No automatic profile tag matches {}.",
                context.car_name.as_deref().unwrap_or(&car_id)
            )),
            1 => Ok(matches.remove(0)),
            _ => Err(format!(
                "{} matches tags assigned to multiple profile defaults. Use a more specific tag.",
                context.car_name.as_deref().unwrap_or(&car_id)
            )),
        }
    })();
    let (profile, matched_tag) = match resolved {
        Ok(resolved) => resolved,
        Err(message) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = message;
            }
            return;
        }
    };
    let routing_state = app.state::<VirtualRoutingState>();
    if routing_state
        .status
        .lock()
        .is_ok_and(|status| status.active && status.profile_id == Some(profile.id))
    {
        if let Ok(mut runtime) = auto_state.0.lock() {
            runtime.last_attempted_profile = Some(profile.id);
            runtime.message = format!(
                "{} is already active for {}.",
                profile.name,
                context.car_name.as_deref().unwrap_or(&car_id)
            );
        }
        return;
    }
    {
        let Ok(mut runtime) = auto_state.0.lock() else {
            return;
        };
        if runtime.paused || runtime.last_attempted_profile == Some(profile.id) {
            return;
        }
        runtime.last_attempted_profile = Some(profile.id);
        runtime.message = format!("Switching to {}…", profile.name);
    }
    let reason = format!("AC tag {matched_tag} detected automatically");
    match start_virtual_routing_inner(profile.id, reason, &app_state, &routing_state) {
        Ok(_) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = format!(
                    "Switched to {} for {}.",
                    profile.name,
                    context.car_name.as_deref().unwrap_or(&car_id)
                );
            }
            diagnostics::info(
                "routing.auto.selected",
                format!(
                    "{} selected for AC car {car_id} via tag {matched_tag}",
                    profile.name
                ),
            );
        }
        Err(error) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message =
                    format!("Could not start {} automatically: {error}", profile.name);
            }
            diagnostics::warn("routing.auto.failed", error);
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_automatic_profile_selection(app: &tauri::AppHandle, context: &CarContextSummary) {
    let app_state = app.state::<AppState>();
    let (enabled, input_mode, workspace_kind) = match app_state.0.lock() {
        Ok(store) => (
            store.settings.auto_profile_selection,
            store.settings.input_mode,
            store.active_workspace().kind,
        ),
        Err(_) => return,
    };
    let auto_state = app.state::<AutoProfileState>();
    if workspace_kind != WorkspaceKind::LeMansUltimate {
        if let Ok(mut runtime) = auto_state.0.lock() {
            runtime.reset("Automatic LMU profile selection is inactive in this workspace.");
        }
        return;
    }
    let confirmed_class = confirmed_class(context);
    let stable_class = {
        let Ok(mut runtime) = auto_state.0.lock() else {
            return;
        };
        if !enabled {
            runtime.reset("Automatic profile selection is off.");
            return;
        }
        if input_mode != InputMode::Virtual {
            runtime.reset("Automatic profile selection waits for Virtual Input mode.");
            return;
        }
        if runtime.paused {
            runtime.debouncer.reset();
            runtime.last_attempted_profile = None;
            return;
        }
        let Some(class_tag) = confirmed_class else {
            if runtime.manual_override_context.is_none() {
                runtime.reset("Holding the current profile until LMU confirms a car class.");
            }
            return;
        };
        if let Some(override_class) = runtime.manual_override_context.clone() {
            if override_class.eq_ignore_ascii_case(class_tag) {
                runtime.debouncer.reset();
                runtime.last_attempted_profile = None;
                return;
            }
            runtime.manual_override_context = None;
            runtime.reset(format!(
                "Detected a change from {} to {}. Resuming automatic selection.",
                display_existing_class(&override_class),
                display_existing_class(class_tag)
            ));
        }
        if !runtime
            .debouncer
            .candidate()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(class_tag))
        {
            runtime.last_attempted_profile = None;
        }
        let stable = runtime
            .debouncer
            .observe(Some(class_tag))
            .map(str::to_owned);
        if stable.is_none() {
            runtime.message = format!(
                "Confirming {} ({}/{})…",
                display_existing_class(class_tag),
                runtime.debouncer.observations(),
                runtime.debouncer.required_observations()
            );
        }
        stable
    };
    let Some(class_tag) = stable_class else {
        return;
    };
    let resolved: Result<Option<Profile>, String> = match app_state.0.lock() {
        Ok(store) => store
            .resolve_virtual_profile(&class_tag)
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let profile = match resolved {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = format!(
                    "No automatic profile default is configured for {}.",
                    display_existing_class(&class_tag)
                );
            }
            return;
        }
        Err(error) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = format!("Could not resolve the class default: {error}");
            }
            return;
        }
    };
    let routing_state = app.state::<VirtualRoutingState>();
    if routing_state
        .status
        .lock()
        .is_ok_and(|status| status.active && status.profile_id == Some(profile.id))
    {
        if let Ok(mut runtime) = auto_state.0.lock() {
            runtime.last_attempted_profile = Some(profile.id);
            runtime.message = format!(
                "{} is already active for {}.",
                profile.name,
                display_existing_class(&class_tag)
            );
        }
        return;
    }
    {
        let Ok(mut runtime) = auto_state.0.lock() else {
            return;
        };
        if runtime.paused {
            return;
        }
        if runtime.last_attempted_profile == Some(profile.id) {
            return;
        }
        runtime.last_attempted_profile = Some(profile.id);
        runtime.message = format!("Switching to {}…", profile.name);
    }
    let reason = format!(
        "{} detected automatically",
        display_existing_class(&class_tag)
    );
    match start_virtual_routing_inner(profile.id, reason, &app_state, &routing_state) {
        Ok(_) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = format!(
                    "Switched to {} for {}.",
                    profile.name,
                    display_existing_class(&class_tag)
                );
            }
            diagnostics::info(
                "routing.auto.selected",
                format!("{} selected for {}", profile.name, class_tag),
            );
        }
        Err(error) => {
            if let Ok(mut runtime) = auto_state.0.lock() {
                runtime.message = format!(
                    "Could not start {} automatically: {error}. Use Resume Auto to retry.",
                    profile.name
                );
            }
            diagnostics::warn("routing.auto.failed", error);
        }
    }
}

fn poll_hotkeys(
    app: tauri::AppHandle,
    updates: std::sync::mpsc::Receiver<HotkeyUpdate>,
    mut shortcuts: Vec<(Uuid, u8)>,
) {
    let mut virtual_setup_enabled = false;
    let mut manager = hotkeys::HotkeyManager::new(&shortcuts, virtual_setup_enabled).ok();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        while let Ok(update) = updates.try_recv() {
            match update {
                HotkeyUpdate::Profiles(next) => shortcuts = next,
                HotkeyUpdate::VirtualSetup(enabled) => virtual_setup_enabled = enabled,
            }
            // Windows will reject a replacement registration while the old
            // manager still owns the same shortcut. Drop it before rebuilding.
            drop(manager.take());
            match hotkeys::HotkeyManager::new(&shortcuts, virtual_setup_enabled) {
                Ok(next) => manager = Some(next),
                Err(error) => {
                    if virtual_setup_enabled {
                        let _ = app.emit(
                            "virtual-setup-result",
                            VirtualSetupEvent {
                                pulse: None,
                                action: None,
                                error: Some(format!(
                                    "The setup hotkey could not be registered: {error}. Use the on-screen button instead."
                                )),
                            },
                        );
                    }
                }
            }
        }
        let action = manager.as_ref().and_then(hotkeys::HotkeyManager::poll);
        match action {
            Some(hotkeys::HotkeyAction::OpenSwitcher) => {
                show_main_window(&app);
                let _ = app.emit("open-profile-switcher", ());
            }
            Some(hotkeys::HotkeyAction::ActivateProfile(profile_id)) => {
                let app_state = app.state::<AppState>();
                let mode = app_state
                    .0
                    .lock()
                    .map(|store| store.settings.input_mode)
                    .map_err(|error| error.to_string());
                let result = mode.and_then(|mode| match mode {
                    InputMode::Legacy => ensure_lmu_file_write_allowed(false).and_then(|_| {
                        app_state
                            .0
                            .lock()
                            .map_err(|error| error.to_string())
                            .and_then(|mut store| {
                                let profile = store
                                    .profiles()
                                    .map_err(|error| error.to_string())?
                                    .into_iter()
                                    .find(|profile| profile.id == profile_id)
                                    .ok_or_else(|| {
                                        "That shortcut's profile no longer exists.".to_owned()
                                    })?;
                                store
                                    .activate(&profile)
                                    .map_err(|error| error.to_string())?;
                                Ok(format!(
                                    "{} is ready for the next LMU launch.",
                                    profile.name
                                ))
                            })
                    }),
                    InputMode::Virtual => {
                        let routing_state = app.state::<VirtualRoutingState>();
                        let auto_state = app.state::<AutoProfileState>();
                        let result = start_virtual_routing_inner(
                            profile_id,
                            "Profile shortcut".to_owned(),
                            &app_state,
                            &routing_state,
                        );
                        if result.is_ok() {
                            let _ = hold_manual_profile_for_active_context(
                                &app_state,
                                &auto_state,
                                &app.state::<CarContextState>(),
                                &app.state::<AcContextState>(),
                            );
                        }
                        result.map(|status| {
                            format!(
                                "{} is now remapping live.",
                                status
                                    .profile_name
                                    .as_deref()
                                    .unwrap_or("The selected profile")
                            )
                        })
                    }
                });
                let _ = app.emit("profile-shortcut-result", result);
            }
            Some(hotkeys::HotkeyAction::SendVirtualSetupButton) => {
                let setup_state = app.state::<VirtualSetupState>();
                let app_state = app.state::<AppState>();
                let result = reserve_virtual_setup_button(&setup_state).and_then(
                    |(device_id, virtual_button, action)| {
                        let result = pulse_virtual_button(device_id, virtual_button);
                        let pulse =
                            finish_virtual_setup_pulse(&setup_state, virtual_button, result)?;
                        if let Some(action) = action.as_ref() {
                            persist_virtual_setup_action(
                                &app_state,
                                &setup_state,
                                action.clone(),
                                virtual_button,
                            )?;
                        }
                        Ok((pulse, action))
                    },
                );
                let event = match result {
                    Ok((pulse, action)) => VirtualSetupEvent {
                        pulse: Some(pulse),
                        action,
                        error: None,
                    },
                    Err(error) => VirtualSetupEvent {
                        pulse: None,
                        action: None,
                        error: Some(error),
                    },
                };
                let _ = app.emit("virtual-setup-result", event);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn microphone_binding(button: u32, modifiers: Vec<u32>) -> SystemActionBinding {
        SystemActionBinding {
            device_id: "wheel".into(),
            device_name: "Wheel".into(),
            physical_button: button,
            modifiers: modifiers
                .into_iter()
                .map(|physical_button| VirtualButtonModifier {
                    device_id: "wheel".into(),
                    device_name: "Wheel".into(),
                    physical_button,
                })
                .collect(),
            play_feedback_sound: true,
            action: SystemAction::Microphone {
                operation: MicrophoneOperation::Toggle,
                device: AudioDeviceTarget::Default {
                    role: AudioDeviceRole::Communications,
                },
            },
        }
    }

    #[test]
    fn profile_system_actions_take_precedence_over_global_bindings() {
        let profile = microphone_binding(12, vec![]);
        let mut global = microphone_binding(12, vec![]);
        global.action = SystemAction::Microphone {
            operation: MicrophoneOperation::Mute,
            device: AudioDeviceTarget::Default {
                role: AudioDeviceRole::Console,
            },
        };
        let held = [("wheel".to_owned(), 12)].into_iter().collect();

        let profile_bindings = [profile.clone()];
        let global_bindings = [global];
        let resolved =
            resolve_system_actions(&profile_bindings, &global_bindings, "wheel", 12, &held);

        assert_eq!(resolved, vec![&profile]);
    }

    #[test]
    fn system_actions_use_the_most_specific_satisfied_modifier_chord() {
        let base = microphone_binding(12, vec![]);
        let mut modified = microphone_binding(12, vec![4]);
        modified.action = SystemAction::Microphone {
            operation: MicrophoneOperation::Unmute,
            device: AudioDeviceTarget::Default {
                role: AudioDeviceRole::Console,
            },
        };
        let held = [("wheel".to_owned(), 4), ("wheel".to_owned(), 12)]
            .into_iter()
            .collect();

        let profile_bindings = [base, modified.clone()];
        let resolved = resolve_system_actions(&profile_bindings, &[], "wheel", 12, &held);

        assert_eq!(resolved, vec![&modified]);
    }

    #[test]
    fn held_system_action_controls_execute_once_per_press_edge() {
        let mut held = std::collections::BTreeSet::new();
        let source = ("wheel".to_owned(), 12);

        assert!(update_pressed_system_buttons(
            &mut held,
            source.clone(),
            true
        ));
        assert!(!update_pressed_system_buttons(
            &mut held,
            source.clone(),
            true
        ));
        assert!(update_pressed_system_buttons(
            &mut held,
            source.clone(),
            false
        ));
        assert!(!update_pressed_system_buttons(
            &mut held,
            source.clone(),
            false
        ));
        assert!(update_pressed_system_buttons(&mut held, source, true));
    }

    #[test]
    fn clutch_capture_payload_uses_frontend_field_names() {
        let payload = serde_json::to_value(ClutchInputCaptureSummary::Axis {
            device_id: "hid:1234:5678".to_owned(),
            device_name: "Test wheel".to_owned(),
            physical_axis: 3,
            minimum: -1_000,
            maximum: 1_000,
        })
        .expect("clutch capture serializes");

        assert_eq!(payload["kind"], "axis");
        assert_eq!(payload["deviceId"], "hid:1234:5678");
        assert_eq!(payload["deviceName"], "Test wheel");
        assert_eq!(payload["physicalAxis"], 3);
        assert!(payload.get("physical_axis").is_none());
    }

    #[test]
    fn tray_menu_describes_legacy_mode_without_offering_live_routing() {
        let presentation = build_tray_menu_presentation(
            "Le Mans Ultimate",
            InputMode::Legacy,
            false,
            true,
            false,
            &VirtualRoutingStatus::default(),
        );

        assert_eq!(presentation.status, "Le Mans Ultimate · Legacy JSON");
        assert_eq!(presentation.routing_toggle_text, "Start remapping");
        assert!(!presentation.routing_toggle_enabled);
        assert!(presentation.switch_profile_enabled);
        assert!(!presentation.resume_auto_enabled);
    }

    #[test]
    fn tray_menu_offers_the_last_virtual_profile_when_available() {
        let presentation = build_tray_menu_presentation(
            "Assetto Corsa",
            InputMode::Virtual,
            true,
            true,
            false,
            &VirtualRoutingStatus::default(),
        );

        assert_eq!(presentation.status, "Assetto Corsa · Remapping off");
        assert_eq!(presentation.routing_toggle_text, "Start remapping");
        assert!(presentation.routing_toggle_enabled);
        assert!(presentation.switch_profile_enabled);
    }

    #[test]
    fn tray_menu_shows_and_can_stop_the_live_profile() {
        let routing = VirtualRoutingStatus {
            active: true,
            profile_name: Some("GT Neo · LMGT3".to_owned()),
            ..VirtualRoutingStatus::default()
        };
        let presentation = build_tray_menu_presentation(
            "Le Mans Ultimate",
            InputMode::Virtual,
            true,
            true,
            true,
            &routing,
        );

        assert_eq!(presentation.status, "Le Mans Ultimate · GT Neo · LMGT3");
        assert_eq!(presentation.routing_toggle_text, "Stop remapping");
        assert!(presentation.routing_toggle_enabled);
        assert!(presentation.resume_auto_enabled);
    }

    #[test]
    fn tray_menu_escapes_windows_menu_mnemonics_in_user_names() {
        let routing = VirtualRoutingStatus {
            active: true,
            profile_name: Some("Wheel & Pedals".to_owned()),
            ..VirtualRoutingStatus::default()
        };
        let presentation = build_tray_menu_presentation(
            "Cars & Sims",
            InputMode::Virtual,
            true,
            true,
            false,
            &routing,
        );

        assert_eq!(presentation.status, "Cars && Sims · Wheel && Pedals");
    }
}

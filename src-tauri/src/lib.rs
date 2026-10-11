use lmukit_core::car_context::{
    ConfirmedClassDebouncer, LmuCarContext, LmuCarContextSource, LmuCarContextStatus,
    LmuSessionState, display_existing_class,
};
use lmukit_core::storage::{
    CompanionApp, InputMode, LaunchMode, Profile, Store, VirtualButtonMapping,
    VirtualButtonModifier, VirtualEncoderControl, VirtualEncoderGroup,
    VirtualEncoderMode as StoredEncoderMode, VirtualProfileMapping, Wheel,
    resolve_virtual_button_mapping,
};
use lmukit_virtual::{
    EncoderGroup, EncoderMode, LogicalAction, PhysicalButton, ProfileBinding, ProfileMapping,
    SetupButtonAllocator, VirtualLayout,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use uuid::Uuid;

mod diagnostics;
mod lmu_process;

#[cfg(target_os = "windows")]
use lmukit_virtual::{
    MappingEngine, VirtualInputBackend, direct_input::DirectInputButtonMonitor, vjoy::VJoyBackend,
};

#[path = "../../src/hotkeys.rs"]
mod hotkeys;
#[path = "../../src/input.rs"]
mod input;

struct AppState(Mutex<Store>);
struct HotkeyState(std::sync::mpsc::Sender<HotkeyUpdate>);
struct VirtualSetupState(Mutex<VirtualSetupSession>);
struct VirtualRoutingState {
    operation: Mutex<()>,
    session: Mutex<Option<VirtualRoutingSession>>,
    status: Arc<Mutex<VirtualRoutingStatus>>,
}

struct CarContextState {
    status: Arc<Mutex<CarContextSummary>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

struct AutoProfileState(Mutex<AutoProfileRuntime>);

struct AutoProfileRuntime {
    paused: bool,
    debouncer: ConfirmedClassDebouncer,
    last_attempted_profile: Option<Uuid>,
    message: String,
}

impl Default for AutoProfileRuntime {
    fn default() -> Self {
        Self {
            paused: false,
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
            paused: self.paused,
            candidate_class: self.debouncer.candidate().map(str::to_owned),
            confirmed_observations: self.debouncer.observations(),
            required_observations: self.debouncer.required_observations(),
            message: self.message.clone(),
        }
    }
}

struct VirtualRoutingSession {
    stop: Arc<std::sync::atomic::AtomicBool>,
    worker: std::thread::JoinHandle<()>,
}

enum HotkeyUpdate {
    Profiles(Vec<(Uuid, u8)>),
    VirtualSetup(bool),
}

struct VirtualSetupSession {
    active: bool,
    device_id: u32,
    identity: String,
    allocator: Option<SetupButtonAllocator>,
}

impl Default for VirtualSetupSession {
    fn default() -> Self {
        Self {
            active: false,
            device_id: 1,
            identity: String::new(),
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
    conflicting_action: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VirtualButtonModifierSummary {
    device_id: String,
    device_name: String,
    physical_button: u32,
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
    active_profile: Option<Uuid>,
    lmu_config_path: String,
    lmu_settings_path: String,
    active_profile_dirty: Option<bool>,
    ui_font_scale: f32,
    input_mode: InputMode,
    auto_start_virtual_profile: bool,
    companion_apps: Vec<CompanionApp>,
    lmu_launch_mode: LaunchMode,
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
    error: Option<String>,
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    let profiles = store.profiles().map_err(|error| error.to_string())?;
    let summaries = profiles
        .iter()
        .map(|profile| profile_summary(&store, profile))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AppSnapshot {
        profiles: summaries,
        wheels: store
            .wheels()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|wheel| wheel_summary(wheel, &profiles))
            .collect(),
        active_profile: store.settings.active_profile,
        lmu_config_path: store.settings.lmu_config_path.display().to_string(),
        lmu_settings_path: store.settings.lmu_settings_path.display().to_string(),
        active_profile_dirty: store
            .active_profile_has_unsaved_changes()
            .map_err(|error| error.to_string())?,
        ui_font_scale: store.settings.ui_font_scale,
        input_mode: store.settings.input_mode,
        auto_start_virtual_profile: store.settings.auto_start_virtual_profile,
        companion_apps: store.settings.companion_apps.clone(),
        lmu_launch_mode: store.settings.lmu_launch_mode,
    })
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
fn save_app_settings(
    lmu_config_path: String,
    lmu_settings_path: String,
    companion_apps: Vec<CompanionApp>,
    launch_mode: LaunchMode,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    store.settings.lmu_config_path = lmu_config_path.trim().into();
    store.settings.lmu_settings_path = lmu_settings_path.trim().into();
    store.settings.companion_apps = companion_apps;
    store.settings.lmu_launch_mode = launch_mode;
    store.save_settings().map_err(|error| error.to_string())
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
    let profiles = store.profiles().map_err(|error| error.to_string())?;
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
fn pick_file(kind: String) -> Result<Option<String>, String> {
    pick_file_dialog(&kind)
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
fn saved_virtual_layout(state: State<'_, AppState>) -> Result<Vec<VirtualActionSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    Ok(store
        .settings
        .virtual_layout
        .iter()
        .map(|entry| VirtualActionSummary {
            action: entry.logical_action.clone(),
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
    let saved = (!mapping.buttons.is_empty() || !mapping.encoder_groups.is_empty())
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
    if !store
        .settings
        .virtual_layout
        .iter()
        .any(|entry| entry.logical_action == logical_action)
    {
        return Err("That action is not in the saved LMU button layout.".to_owned());
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
    let conflicting_action = store
        .virtual_button_conflict(profile_id, &proposed)
        .map_err(|error| error.to_string())?;
    Ok(VirtualButtonCaptureSummary {
        mapping: mapping_summary(&proposed),
        conflicting_action,
    })
}

#[tauri::command]
fn save_virtual_profile_button(
    profile_id: Uuid,
    proposed: VirtualProfileMappingSummary,
    replace_conflict: bool,
    state: State<'_, AppState>,
) -> Result<Vec<VirtualProfileMappingSummary>, String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    if !store
        .settings
        .virtual_layout
        .iter()
        .any(|entry| entry.logical_action == proposed.logical_action)
    {
        return Err("That action is not in the saved LMU button layout.".to_owned());
    }
    let proposed = stored_button_mapping(proposed);
    store
        .assign_profile_virtual_button(profile_id, proposed, replace_conflict)
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
    let matched = profile.virtual_mapping.as_ref().and_then(|mapping| {
        resolve_virtual_button_mapping(
            mapping,
            &trigger.device_id,
            trigger.button_id,
            &held_modifiers,
        )
    });
    let matches = matched
        .and_then(|mapping| {
            store
                .settings
                .virtual_layout
                .iter()
                .find(|entry| entry.logical_action == mapping.logical_action)
                .map(|layout| VirtualControlInspectionMatch {
                    logical_action: mapping.logical_action.clone(),
                    virtual_button: layout.virtual_button,
                })
        })
        .into_iter()
        .collect();
    Ok(VirtualControlInspection { control, matches })
}

#[tauri::command]
fn remove_virtual_profile_button(
    profile_id: Uuid,
    device_id: String,
    physical_button: u32,
    modifiers: Vec<VirtualButtonModifierSummary>,
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
            && entry.modifiers == modifiers)
    });
    let saved = (!mapping.buttons.is_empty() || !mapping.encoder_groups.is_empty())
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
fn lmu_car_context_status(state: State<'_, CarContextState>) -> Result<CarContextSummary, String> {
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
    {
        let mut store = app_state.0.lock().map_err(|error| error.to_string())?;
        store.settings.auto_profile_selection = enabled;
        store.save_settings().map_err(|error| error.to_string())?;
    }
    let mut runtime = auto_state.0.lock().map_err(|error| error.to_string())?;
    runtime.paused = false;
    runtime.reset(if enabled {
        "Waiting for LMU to confirm a car class."
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
    let enabled = app_state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .settings
        .auto_profile_selection;
    if !enabled {
        return Err("Enable automatic profile selection first.".to_owned());
    }
    let mut runtime = auto_state.0.lock().map_err(|error| error.to_string())?;
    runtime.paused = false;
    runtime.reset("Waiting for LMU to confirm a car class.");
    diagnostics::info(
        "routing.auto.resumed",
        "Automatic class profile selection resumed",
    );
    Ok(runtime.summary(true))
}

fn pause_auto_profile_selection(state: &AutoProfileState, reason: &str) {
    if let Ok(mut runtime) = state.0.lock() {
        runtime.paused = true;
        runtime.reset(format!(
            "Paused after {reason}. Select Resume Auto when ready."
        ));
    }
}

#[tauri::command]
fn start_virtual_routing(
    profile_id: Uuid,
    app_state: State<'_, AppState>,
    routing_state: State<'_, VirtualRoutingState>,
    auto_state: State<'_, AutoProfileState>,
) -> Result<VirtualRoutingStatus, String> {
    pause_auto_profile_selection(&auto_state, "manual profile selection");
    start_virtual_routing_inner(
        profile_id,
        "Selected manually".to_owned(),
        &app_state,
        &routing_state,
    )
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
    pause_auto_profile_selection(&auto_state, "manual class selection");
    start_virtual_routing_inner(profile_id, reason, &app_state, &routing_state)
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
    let (profile_name, device_id, layout, mapping) = {
        let store = app_state.0.lock().map_err(|error| error.to_string())?;
        if store.settings.input_mode != InputMode::Virtual {
            return Err(
                "Switch the global input method to Virtual Input before enabling live remapping."
                    .to_owned(),
            );
        }
        let profile = find_profile(&store, profile_id)?;
        let persisted_mapping = profile
            .virtual_mapping
            .as_ref()
            .filter(|mapping| !mapping.buttons.is_empty() || !mapping.encoder_groups.is_empty())
            .ok_or_else(|| {
                "Learn at least one physical button for this profile first.".to_owned()
            })?;
        let layout = VirtualLayout::new(
            store
                .settings
                .virtual_layout
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
        (
            profile.name,
            store.settings.virtual_device_id,
            layout,
            mapping,
        )
    };
    stop_virtual_routing_inner(routing_state)?;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let status = Arc::clone(&routing_state.status);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let worker_profile_name = profile_name.clone();
    let worker_selection_reason = selection_reason.clone();
    let worker = std::thread::spawn(move || {
        run_virtual_routing_worker(
            profile_id,
            worker_profile_name,
            device_id,
            layout,
            mapping,
            worker_stop,
            status,
            ready_tx,
            worker_selection_reason,
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
        .map_err(|error| error.to_string())? = Some(VirtualRoutingSession { stop, worker });
    let next = VirtualRoutingStatus {
        active: true,
        profile_id: Some(profile_id),
        profile_name: Some(profile_name),
        identity: Some(identity),
        selection_reason: Some(selection_reason),
        active_encoder_modes,
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
    stop: Arc<std::sync::atomic::AtomicBool>,
    status: Arc<Mutex<VirtualRoutingStatus>>,
    ready: std::sync::mpsc::SyncSender<Result<(String, Vec<ActiveEncoderModeSummary>), String>>,
    selection_reason: String,
) {
    let result = (|| -> Result<(), String> {
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
        let backend = VJoyBackend::new(device_id).map_err(|error| error.to_string())?;
        let mut engine =
            MappingEngine::connect(backend, layout, mapping).map_err(|error| error.to_string())?;
        let identity = engine
            .capabilities()
            .map_err(|error| error.to_string())?
            .identity;
        let encoder_modes = engine
            .active_encoder_modes()
            .into_iter()
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
                profile_name: Some(profile_name),
                identity: Some(identity),
                selection_reason: Some(selection_reason),
                active_encoder_modes: encoder_modes,
                ..VirtualRoutingStatus::default()
            };
        }
        while !stop.load(Ordering::SeqCst) {
            for event in listener.poll().map_err(|error| error.to_string())? {
                let source = PhysicalButton::new(event.device_id, event.button_id)
                    .map_err(|error| error.to_string())?;
                let routed = engine
                    .handle_button(&source, event.pressed)
                    .map_err(|error| error.to_string())?;
                if routed && let Ok(mut current) = status.lock() {
                    current.routed_event_count += 1;
                    current.last_event = Some(format!(
                        "Button {} {}",
                        event.button_id,
                        if event.pressed { "pressed" } else { "released" }
                    ));
                    current.active_encoder_modes = engine
                        .active_encoder_modes()
                        .into_iter()
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
            engine.tick().map_err(|error| error.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        engine.disconnect().map_err(|error| error.to_string())
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
    _stop: Arc<std::sync::atomic::AtomicBool>,
    _status: Arc<Mutex<VirtualRoutingStatus>>,
    ready: std::sync::mpsc::SyncSender<Result<(String, Vec<ActiveEncoderModeSummary>), String>>,
    _selection_reason: String,
) {
    let _ = ready.send(Err(
        "Virtual routing is only available in the Windows build.".to_owned(),
    ));
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
    let (device_id, used_buttons) = {
        let store = state.0.lock().map_err(|error| error.to_string())?;
        let used_buttons = existing_vjoy_buttons(&store)?;
        (store.settings.virtual_device_id, used_buttons)
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
async fn send_next_virtual_button(
    setup_state: State<'_, VirtualSetupState>,
) -> Result<VirtualButtonPulse, String> {
    let (device_id, virtual_button) = reserve_virtual_setup_button(&setup_state)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        pulse_virtual_button(device_id, virtual_button)
    })
    .await
    .map_err(|error| format!("Virtual button worker stopped unexpectedly: {error}"))?;
    finish_virtual_setup_pulse(&setup_state, virtual_button, result)
}

fn existing_vjoy_buttons(store: &Store) -> Result<Vec<u32>, String> {
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
    match configured.or(detected) {
        Some(device_key) => Store::used_virtual_buttons_from_document(&document, device_key)
            .map_err(|error| error.to_string()),
        None => Ok(Vec::new()),
    }
}

fn reserve_virtual_setup_button(
    setup_state: &State<'_, VirtualSetupState>,
) -> Result<(u32, u32), String> {
    let mut session = setup_state.0.lock().map_err(|error| error.to_string())?;
    if !session.active {
        return Err("Start the virtual binding setup first.".to_owned());
    }
    let virtual_button = session
        .allocator
        .as_mut()
        .ok_or_else(|| "The virtual binding setup is not initialised.".to_owned())?
        .reserve_next()
        .map_err(|error| error.to_string())?;
    Ok((session.device_id, virtual_button))
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
fn pick_file_dialog(kind: &str) -> Result<Option<String>, String> {
    let (filter, filename) = match kind {
        "bindings" => ("JSON files (*.json)|*.json", "direct input.json"),
        "settings" => ("JSON files (*.json)|*.json", "Settings.JSON"),
        "executable" => ("Windows applications (*.exe)|*.exe", ""),
        _ => return Err("Unknown file picker type.".to_owned()),
    };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='{filter}'; $d.FileName='{filename}'; if($d.ShowDialog() -eq 'OK'){{[Console]::Write($d.FileName)}}"
    );
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!path.is_empty()).then_some(path))
}

#[cfg(not(target_os = "windows"))]
fn pick_file_dialog(_kind: &str) -> Result<Option<String>, String> {
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

#[cfg(not(target_os = "windows"))]
fn detect_direct_input_mapping(_generation: u64) -> Result<LearnedPhysicalMapping, String> {
    Err("DirectInput button learning is only available in the Windows build.".to_owned())
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

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show-lmukit", "Show LMUKit", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit-lmukit", "Exit LMUKit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &exit])?;
    let mut tray = TrayIconBuilder::with_id("lmukit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("LMUKit")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-lmukit" => show_main_window(app),
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
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Store::open().expect("LMUKit data store should open");
    if let Err(error) =
        diagnostics::initialise(&store.data_dir(), store.settings.log_retention_days)
    {
        eprintln!("LMUKit logging could not start: {error}");
    }
    let shortcuts = store
        .profiles()
        .map(|profiles| profile_shortcuts(&profiles))
        .unwrap_or_default();
    let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
    let car_context_status = Arc::new(Mutex::new(CarContextSummary::default()));
    let car_context_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(target_os = "windows")]
    let context_worker_status = Arc::clone(&car_context_status);
    #[cfg(target_os = "windows")]
    let context_worker_stop = Arc::clone(&car_context_stop);
    let app = tauri::Builder::default()
        .manage(AppState(Mutex::new(store)))
        .manage(HotkeyState(hotkey_tx))
        .manage(VirtualSetupState(
            Mutex::new(VirtualSetupSession::default()),
        ))
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
        .setup(move |app| {
            setup_tray(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || poll_hotkeys(handle, hotkey_rx, shortcuts));
            #[cfg(target_os = "windows")]
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    poll_lmu_car_context(handle, context_worker_status, context_worker_stop)
                });
            }
            let auto_start_profile = {
                let state = app.state::<AppState>();
                state.0.lock().ok().and_then(|store| {
                    (store.settings.input_mode == InputMode::Virtual
                        && store.settings.auto_start_virtual_profile)
                        .then_some(store.settings.last_virtual_profile)
                        .flatten()
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
            import_profile,
            update_profile,
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
            set_input_mode,
            set_virtual_auto_start,
            set_log_retention_days,
            exit_app,
            save_app_settings,
            set_profile_hotkey,
            companion_launch_option,
            pick_file,
            load_game_settings,
            save_game_settings,
            lmu_runtime_status,
            virtual_binding_devices,
            preview_virtual_layout,
            import_virtual_layout,
            virtual_profile_mapping,
            saved_virtual_layout,
            import_profile_virtual_mapping,
            virtual_profile_defaults,
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
            lmu_car_context_status,
            auto_profile_selection_status,
            set_auto_profile_selection,
            resume_auto_profile_selection,
            start_virtual_routing,
            start_default_virtual_routing,
            stop_virtual_routing,
            start_virtual_binding_setup,
            stop_virtual_binding_setup,
            send_next_virtual_button
        ])
        .build(tauri::generate_context!())
        .expect("error while building LMUKit");
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
fn poll_lmu_car_context(
    app: tauri::AppHandle,
    status: Arc<Mutex<CarContextSummary>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    use lmukit_context::windows::WindowsLmuCarContextProvider;
    use lmukit_core::car_context::LmuCarContextProvider;

    while !stop.load(Ordering::Acquire) {
        // Open and close the LMU handles for each low-frequency sample. If LMU
        // exits, LMUKit therefore cannot keep the game-owned mappings alive and
        // incorrectly present an old snapshot as a current observation.
        let observation = WindowsLmuCarContextProvider::connect()
            .and_then(|mut provider| provider.current_context());
        let summary = if let Ok(current) = status.lock() {
            match observation {
                Ok(context) => CarContextSummary::observed(context),
                Err(_) => CarContextSummary::unavailable(&current),
            }
        } else {
            CarContextSummary::default()
        };
        if let Ok(mut current) = status.lock() {
            *current = summary.clone();
        }
        apply_automatic_profile_selection(&app, &summary);
        std::thread::sleep(std::time::Duration::from_millis(1_500));
    }
}

#[cfg(target_os = "windows")]
fn apply_automatic_profile_selection(app: &tauri::AppHandle, context: &CarContextSummary) {
    let app_state = app.state::<AppState>();
    let (enabled, input_mode) = match app_state.0.lock() {
        Ok(store) => (
            store.settings.auto_profile_selection,
            store.settings.input_mode,
        ),
        Err(_) => return,
    };
    let auto_state = app.state::<AutoProfileState>();
    let confirmed_class = (context.connected
        && context.status == LmuCarContextStatus::Confirmed
        && matches!(
            context.session_state,
            LmuSessionState::Session | LmuSessionState::Realtime | LmuSessionState::Replay
        ))
    .then_some(context.class_id.as_deref())
    .flatten();
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
            runtime.reset("Holding the current profile until LMU confirms a car class.");
            return;
        };
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
                        pause_auto_profile_selection(&auto_state, "profile shortcut");
                        start_virtual_routing_inner(
                            profile_id,
                            "Profile shortcut".to_owned(),
                            &app_state,
                            &routing_state,
                        )
                        .map(|status| {
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
                let result = reserve_virtual_setup_button(&setup_state).and_then(
                    |(device_id, virtual_button)| {
                        let result = pulse_virtual_button(device_id, virtual_button);
                        finish_virtual_setup_pulse(&setup_state, virtual_button, result)
                    },
                );
                let event = match result {
                    Ok(pulse) => VirtualSetupEvent {
                        pulse: Some(pulse),
                        error: None,
                    },
                    Err(error) => VirtualSetupEvent {
                        pulse: None,
                        error: Some(error),
                    },
                };
                let _ = app.emit("virtual-setup-result", event);
            }
            None => {}
        }
    }
}

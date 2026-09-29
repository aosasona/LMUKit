use lmukit_core::storage::{CompanionApp, LaunchMode, Profile, Store, Wheel};
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

#[path = "../../src/hotkeys.rs"]
mod hotkeys;
#[path = "../../src/input.rs"]
mod input;

struct AppState(Mutex<Store>);
struct HotkeyState(std::sync::mpsc::Sender<Vec<(Uuid, u8)>>);

static LOOKUP_GENERATION: AtomicU64 = AtomicU64::new(0);

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
        .send(profile_shortcuts(&profiles))
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
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.0.lock().map_err(|error| error.to_string())?;
    store
        .save_lmu_settings_document(&document)
        .map(|_| ())
        .map_err(|error| error.to_string())
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
fn activate_profile(profile_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.0.lock().map_err(|error| error.to_string())?;
    let profile = store
        .profiles()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "That profile no longer exists.".to_owned())?;
    store
        .activate(&profile)
        .map_err(|error| error.to_string())?;
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
    let store = state.0.lock().map_err(|error| error.to_string())?;
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
    let store = state.0.lock().map_err(|error| error.to_string())?;
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
    let store = state.0.lock().map_err(|error| error.to_string())?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Store::open().expect("LMUKit data store should open");
    let shortcuts = store
        .profiles()
        .map(|profiles| profile_shortcuts(&profiles))
        .unwrap_or_default();
    let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
    tauri::Builder::default()
        .manage(AppState(Mutex::new(store)))
        .manage(HotkeyState(hotkey_tx))
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || poll_hotkeys(handle, hotkey_rx, shortcuts));
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
            profile_bindings,
            clear_profile_binding,
            assign_profile_binding,
            set_profile_categories,
            rename_profile,
            find_binding_matches,
            listen_for_profile_binding,
            cancel_binding_lookup,
            set_ui_font_scale,
            save_app_settings,
            set_profile_hotkey,
            companion_launch_option,
            pick_file,
            load_game_settings,
            save_game_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running LMUKit");
}

fn profile_shortcuts(profiles: &[Profile]) -> Vec<(Uuid, u8)> {
    profiles
        .iter()
        .filter_map(|profile| profile.hotkey_slot.map(|slot| (profile.id, slot)))
        .collect()
}

fn poll_hotkeys(
    app: tauri::AppHandle,
    updates: std::sync::mpsc::Receiver<Vec<(Uuid, u8)>>,
    shortcuts: Vec<(Uuid, u8)>,
) {
    let mut manager = hotkeys::HotkeyManager::new(&shortcuts).ok();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        while let Ok(shortcuts) = updates.try_recv() {
            manager = hotkeys::HotkeyManager::new(&shortcuts).ok();
        }
        let action = manager.as_ref().and_then(hotkeys::HotkeyManager::poll);
        match action {
            Some(hotkeys::HotkeyAction::OpenSwitcher) => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit("open-profile-switcher", ());
            }
            Some(hotkeys::HotkeyAction::ActivateProfile(profile_id)) => {
                let result = app
                    .state::<AppState>()
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
                        Ok(profile.name)
                    });
                let _ = app.emit("profile-shortcut-result", result);
            }
            None => {}
        }
    }
}

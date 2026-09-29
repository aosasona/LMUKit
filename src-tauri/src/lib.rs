use lmukit_core::storage::{Profile, Store};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use uuid::Uuid;

struct AppState(Mutex<Store>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    profiles: Vec<ProfileSummary>,
    active_profile: Option<Uuid>,
    lmu_config_path: String,
    lmu_settings_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    id: Uuid,
    name: String,
    hotkey_slot: Option<u8>,
    binding_count: usize,
    devices: Vec<String>,
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
        active_profile: store.settings.active_profile,
        lmu_config_path: store.settings.lmu_config_path.display().to_string(),
        lmu_settings_path: store.settings.lmu_settings_path.display().to_string(),
    })
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

fn profile_summary(store: &Store, profile: &Profile) -> Result<ProfileSummary, String> {
    let document = store
        .load_profile_document(profile)
        .map_err(|error| error.to_string())?;
    let mut devices = document
        .get("Devices")
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| {
                    value
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(key)
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    devices.sort();
    devices.dedup();
    Ok(ProfileSummary {
        id: profile.id,
        name: profile.name.clone(),
        hotkey_slot: profile.hotkey_slot,
        binding_count: Store::bindings_from_document(&document).len(),
        devices,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Store::open().expect("LMUKit data store should open");
    tauri::Builder::default()
        .manage(AppState(Mutex::new(store)))
        .invoke_handler(tauri::generate_handler![snapshot, activate_profile])
        .run(tauri::generate_context!())
        .expect("error while running LMUKit");
}

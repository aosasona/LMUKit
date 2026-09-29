use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const CONFIG_FILE_NAME: &str = "direct input.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    #[serde(default)]
    pub assignments: Vec<String>,
    #[serde(default)]
    pub hotkey_slot: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub action: String,
    pub device: String,
    pub input_id: u64,
    pub alternate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingMatch {
    pub profile_name: String,
    pub action: String,
    pub alternate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub lmu_config_path: PathBuf,
    #[serde(default)]
    pub lmu_settings_path: PathBuf,
    #[serde(default)]
    pub active_profile: Option<Uuid>,
    #[serde(default, skip_serializing)]
    pub lmuffb_path: PathBuf,
    #[serde(default = "default_companion_apps")]
    pub companion_apps: Vec<CompanionApp>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompanionApp {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lmu_config_path: default_lmu_path(),
            lmu_settings_path: default_lmu_settings_path(),
            active_profile: None,
            lmuffb_path: PathBuf::new(),
            companion_apps: default_companion_apps(),
        }
    }
}

fn default_companion_apps() -> Vec<CompanionApp> {
    vec![
        CompanionApp {
            name: "LMUFFB".into(),
            path: PathBuf::new(),
            enabled: true,
        },
        CompanionApp {
            name: "Crew Chief".into(),
            path: PathBuf::new(),
            enabled: false,
        },
    ]
}

pub struct Store {
    root: PathBuf,
    pub settings: Settings,
}

impl Store {
    pub fn open() -> Result<Self> {
        if let Some(root) = std::env::var_os("LMUKIT_DATA_DIR") {
            return Self::open_at(PathBuf::from(root));
        }
        let dirs = ProjectDirs::from("app", "LMUKit", "LMUKit")
            .context("could not determine LMUKit's application-data directory")?;
        Self::open_at(dirs.data_local_dir().to_path_buf())
    }

    pub fn open_at(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("backups"))?;
        let settings_path = root.join("settings.json");
        let mut settings = if settings_path.exists() {
            serde_json::from_slice(&fs::read(&settings_path)?)
                .context("LMUKit's settings file is invalid")?
        } else {
            Settings::default()
        };
        if settings.lmu_settings_path.as_os_str().is_empty() {
            settings.lmu_settings_path = settings.lmu_config_path.with_file_name("Settings.JSON");
        }
        if !settings.lmuffb_path.as_os_str().is_empty()
            && let Some(lmuffb) = settings
                .companion_apps
                .iter_mut()
                .find(|app| app.name == "LMUFFB")
        {
            lmuffb.path = std::mem::take(&mut settings.lmuffb_path);
            lmuffb.enabled = true;
        }
        Ok(Self { root, settings })
    }

    pub fn save_settings(&self) -> Result<()> {
        write_json(&self.root.join("settings.json"), &self.settings)
    }

    pub fn load_lmu_settings_document(&self) -> Result<serde_json::Value> {
        let path = &self.settings.lmu_settings_path;
        let document: serde_json::Value = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("could not read {}", path.display()))?,
        )
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
        if !document.is_object() {
            bail!("{} does not contain a JSON object", path.display());
        }
        Ok(document)
    }

    pub fn save_lmu_settings_document(&self, document: &serde_json::Value) -> Result<PathBuf> {
        if !document.is_object() {
            bail!("LMU settings must contain a JSON object.");
        }
        let destination = &self.settings.lmu_settings_path;
        if !destination.is_file() {
            bail!(
                "No LMU Settings.JSON was found at {}",
                destination.display()
            );
        }
        let parent = destination
            .parent()
            .context("LMU Settings.JSON path has no parent directory")?;
        let backup = self
            .root
            .join("backups")
            .join(format!("settings-{}.json", unix_time()?));
        fs::copy(destination, &backup).context("could not back up LMU's Settings.JSON")?;

        let staged = parent.join(".lmukit-settings.tmp");
        fs::write(&staged, serde_json::to_vec_pretty(document)?)
            .context("could not stage LMU's edited settings")?;
        if let Err(error) = replace_file(&staged, destination) {
            let _ = fs::copy(&backup, destination);
            return Err(error).context("could not save LMU's Settings.JSON");
        }
        Ok(backup)
    }

    pub fn companion_launch_option(&self, lmukit_exe: &Path) -> Result<String> {
        if !lmukit_exe.is_file() {
            bail!(
                "LMUKit's executable could not be found at {}",
                lmukit_exe.display()
            );
        }
        let launcher = self.root.join("launch-lmu-with-companions.cmd");
        let lmukit = batch_path(lmukit_exe);
        let mut script = format!(
            "@echo off\r\n\
             tasklist /FI \"IMAGENAME eq lmukit.exe\" 2>NUL | find /I \"lmukit.exe\" >NUL\r\n\
             if errorlevel 1 start \"\" \"{lmukit}\"\r\n"
        );
        for app in self
            .settings
            .companion_apps
            .iter()
            .filter(|app| app.enabled && app.path.is_file())
        {
            let executable = app
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .context("a companion app has no usable executable name")?;
            let path = batch_path(&app.path);
            script.push_str(&format!(
                "tasklist /FI \"IMAGENAME eq {executable}\" 2>NUL | find /I \"{executable}\" >NUL\r\n\
                 if errorlevel 1 start \"\" \"{path}\"\r\n"
            ));
        }
        script.push_str("%*\r\n");
        fs::write(&launcher, script).context("could not create the companion launcher")?;
        Ok(format!("cmd /c \"\"{}\" %command%\"", launcher.display()))
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn profiles(&self) -> Result<Vec<Profile>> {
        let mut profiles: Vec<Profile> = Vec::new();
        for entry in fs::read_dir(self.profiles_dir())? {
            let path = entry?.path().join("profile.json");
            if path.is_file() {
                profiles.push(serde_json::from_slice(&fs::read(&path)?)?);
            }
        }
        profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(profiles)
    }

    pub fn save_profile_metadata(&self, profile: &Profile) -> Result<()> {
        write_json(&self.profile_dir(profile.id).join("profile.json"), profile)
            .context("could not save profile metadata")
    }

    pub fn capture(&self, name: &str) -> Result<Profile> {
        let source = &self.settings.lmu_config_path;
        self.store_profile(source, name)
    }

    pub fn import_profile(&self, dropped: &Path) -> Result<Profile> {
        let source = if dropped.is_dir() {
            dropped.join(CONFIG_FILE_NAME)
        } else {
            dropped.to_path_buf()
        };
        let name = imported_profile_name(dropped)?;
        self.store_profile(&source, &name)
    }

    pub fn bindings(&self, profile: &Profile) -> Result<Vec<Binding>> {
        let document = self.profile_document(profile)?;
        Ok(Self::bindings_from_document(&document))
    }

    pub fn bindings_from_document(document: &serde_json::Value) -> Vec<Binding> {
        let mut bindings = Vec::new();
        collect_bindings(document, "Input", false, &mut bindings);
        collect_bindings(document, "Alternative Input", true, &mut bindings);
        bindings.sort_by(|a, b| a.action.to_lowercase().cmp(&b.action.to_lowercase()));
        bindings
    }

    pub fn load_profile_document(&self, profile: &Profile) -> Result<serde_json::Value> {
        self.profile_document(profile)
    }

    pub fn save_profile_document(
        &self,
        profile: &Profile,
        document: &serde_json::Value,
    ) -> Result<PathBuf> {
        if !document.is_object() {
            bail!("A profile must contain a JSON object.");
        }
        let destination = self.profile_dir(profile.id).join(CONFIG_FILE_NAME);
        if !destination.is_file() {
            bail!("The saved profile '{}' could not be found.", profile.name);
        }
        let backup = self.profile_backup_path(profile)?;
        fs::copy(&destination, &backup).context("could not back up the saved profile")?;

        let staged = self
            .profile_dir(profile.id)
            .join(".direct-input-editor.tmp");
        fs::write(&staged, serde_json::to_vec_pretty(document)?)
            .context("could not stage the edited profile")?;
        if let Err(error) = replace_file(&staged, &destination) {
            let _ = fs::copy(&backup, &destination);
            return Err(error).context("could not save the edited profile");
        }
        write_json(&self.profile_dir(profile.id).join("profile.json"), profile)
            .context("could not save the profile's editor metadata")?;
        Ok(backup)
    }

    pub fn matching_device(
        document: &serde_json::Value,
        vendor_id: u16,
        product_id: u16,
    ) -> Option<String> {
        document
            .get("Devices")?
            .as_object()?
            .keys()
            .find(|device| device_matches(document, device, vendor_id, product_id))
            .cloned()
    }

    pub fn set_binding(
        document: &mut serde_json::Value,
        action: &str,
        alternate: bool,
        device: &str,
        input_id: u64,
    ) -> Result<()> {
        let mapping = binding_section_mut(document, alternate)?
            .entry(action.to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let mapping = mapping
            .as_object_mut()
            .context("the selected binding has an invalid JSON shape")?;
        mapping.insert("device".into(), serde_json::Value::String(device.into()));
        mapping.insert("id".into(), serde_json::Value::from(input_id));
        Ok(())
    }

    pub fn clear_binding(
        document: &mut serde_json::Value,
        action: &str,
        alternate: bool,
    ) -> Result<()> {
        binding_section_mut(document, alternate)?.remove(action);
        Ok(())
    }

    pub fn binding_matches(
        &self,
        profiles: &[Profile],
        vendor_id: u16,
        product_id: u16,
        input_id: u64,
    ) -> Result<Vec<BindingMatch>> {
        let mut matches = Vec::new();
        for profile in profiles {
            let document = self.profile_document(profile)?;
            let mut bindings = Vec::new();
            collect_bindings(&document, "Input", false, &mut bindings);
            collect_bindings(&document, "Alternative Input", true, &mut bindings);
            for binding in bindings {
                if binding.input_id == input_id
                    && device_matches(&document, &binding.device, vendor_id, product_id)
                {
                    matches.push(BindingMatch {
                        profile_name: profile.name.clone(),
                        action: binding.action,
                        alternate: binding.alternate,
                    });
                }
            }
        }
        matches.sort_by(|a, b| a.profile_name.cmp(&b.profile_name));
        Ok(matches)
    }

    pub fn active_profile_has_unsaved_changes(&self) -> Result<Option<bool>> {
        let Some(active_id) = self.settings.active_profile else {
            return Ok(None);
        };
        let saved_path = self.profile_dir(active_id).join(CONFIG_FILE_NAME);
        if !saved_path.is_file() || !self.settings.lmu_config_path.is_file() {
            return Ok(None);
        }
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&saved_path)?)
            .with_context(|| format!("{} is not valid JSON", saved_path.display()))?;
        let live: serde_json::Value = serde_json::from_slice(&fs::read(
            &self.settings.lmu_config_path,
        )?)
        .with_context(|| {
            format!(
                "{} is not valid JSON",
                self.settings.lmu_config_path.display()
            )
        })?;
        Ok(Some(saved != live))
    }

    fn profile_document(&self, profile: &Profile) -> Result<serde_json::Value> {
        let path = self.profile_dir(profile.id).join(CONFIG_FILE_NAME);
        serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("{} is not valid JSON", path.display()))
    }

    fn store_profile(&self, source: &Path, name: &str) -> Result<Profile> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Enter a profile name.");
        }
        validate_lmu_file(source)?;

        let profile = Profile {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: unix_time()?,
            assignments: Vec::new(),
            hotkey_slot: None,
        };
        let dir = self.profile_dir(profile.id);
        fs::create_dir_all(&dir)?;
        fs::copy(source, dir.join(CONFIG_FILE_NAME))
            .with_context(|| format!("could not copy {}", source.display()))?;
        write_json(&dir.join("profile.json"), &profile)?;
        Ok(profile)
    }

    pub fn activate(&mut self, profile: &Profile) -> Result<PathBuf> {
        let saved = self.profile_dir(profile.id).join(CONFIG_FILE_NAME);
        validate_lmu_file(&saved)?;

        let destination = self.settings.lmu_config_path.clone();
        let parent = destination
            .parent()
            .context("LMU path has no parent directory")?;
        fs::create_dir_all(parent)?;

        let backup = self
            .root
            .join("backups")
            .join(format!("direct input-{}.json", unix_time()?));
        if destination.exists() {
            fs::copy(&destination, &backup).context("could not create the safety backup")?;
        }

        let staged = parent.join(".lmukit-direct-input.tmp");
        fs::copy(&saved, &staged).context("could not stage the selected profile")?;
        if destination.exists() {
            fs::remove_file(&destination).context("could not replace LMU's current config")?;
        }
        if let Err(error) = fs::rename(&staged, &destination) {
            if backup.exists() {
                let _ = fs::copy(&backup, &destination);
            }
            return Err(error).context("could not install the selected profile");
        }

        self.settings.active_profile = Some(profile.id);
        self.save_settings()?;
        Ok(backup)
    }

    pub fn update_profile(&self, profile: &Profile) -> Result<PathBuf> {
        let source = &self.settings.lmu_config_path;
        validate_lmu_file(source)?;

        let destination = self.profile_dir(profile.id).join(CONFIG_FILE_NAME);
        if !destination.is_file() {
            bail!("The saved profile '{}' could not be found.", profile.name);
        }

        let backup = self.profile_backup_path(profile)?;
        fs::copy(&destination, &backup).context("could not back up the saved profile")?;

        let staged = self
            .profile_dir(profile.id)
            .join(".direct-input-update.tmp");
        fs::copy(source, &staged).context("could not stage LMU's current bindings")?;
        if let Err(error) = replace_file(&staged, &destination) {
            let _ = fs::copy(&backup, &destination);
            return Err(error).context("could not update the saved profile");
        }

        Ok(backup)
    }

    fn profile_backup_path(&self, profile: &Profile) -> Result<PathBuf> {
        Ok(self
            .root
            .join("backups")
            .join(format!("profile-{}-{}.json", profile.id, unix_time()?)))
    }

    pub fn delete(&mut self, profile: &Profile) -> Result<()> {
        fs::remove_dir_all(self.profile_dir(profile.id))?;
        if self.settings.active_profile == Some(profile.id) {
            self.settings.active_profile = None;
            self.save_settings()?;
        }
        Ok(())
    }

    fn profile_dir(&self, id: Uuid) -> PathBuf {
        self.root.join("profiles").join(id.to_string())
    }
}

fn binding_section_mut(
    document: &mut serde_json::Value,
    alternate: bool,
) -> Result<&mut serde_json::Map<String, serde_json::Value>> {
    let section_name = if alternate {
        "Alternative Input"
    } else {
        "Input"
    };
    let root = document
        .as_object_mut()
        .context("the profile root is not a JSON object")?;
    root.entry(section_name)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .with_context(|| format!("{section_name} is not a JSON object"))
}

fn replace_file(staged: &Path, destination: &Path) -> std::io::Result<()> {
    fs::remove_file(destination)?;
    fs::rename(staged, destination)
}

fn batch_path(path: &Path) -> String {
    path.display().to_string().replace('%', "%%")
}

fn device_matches(
    document: &serde_json::Value,
    device: &str,
    vendor_id: u16,
    product_id: u16,
) -> bool {
    let Some(guid) = document
        .get("Devices")
        .and_then(|devices| devices.get(device))
        .and_then(|device| device.get("product guid"))
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    let Some(data) = guid.strip_prefix('{').and_then(|guid| guid.get(..8)) else {
        return false;
    };
    u32::from_str_radix(data, 16)
        .is_ok_and(|id| id as u16 == vendor_id && (id >> 16) as u16 == product_id)
}

fn collect_bindings(
    document: &serde_json::Value,
    section: &str,
    alternate: bool,
    bindings: &mut Vec<Binding>,
) {
    let Some(entries) = document.get(section).and_then(serde_json::Value::as_object) else {
        return;
    };
    for (action, mapping) in entries {
        let Some(device) = mapping.get("device").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(input_id) = mapping.get("id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        bindings.push(Binding {
            action: action.clone(),
            device: device.to_owned(),
            input_id,
            alternate,
        });
    }
}

fn imported_profile_name(path: &Path) -> Result<String> {
    if path.is_dir() {
        let metadata = path.join("profile.json");
        if metadata.is_file()
            && let Ok(profile) = serde_json::from_slice::<Profile>(&fs::read(metadata)?)
        {
            return Ok(profile.name);
        }
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .context("the dropped profile folder has no usable name");
    }

    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("the dropped preset has no usable filename")?;
    if stem.eq_ignore_ascii_case("direct input") {
        return path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .context("place direct input.json in a named folder before importing it");
    }
    Ok(stem.to_owned())
}

pub fn default_lmu_path() -> PathBuf {
    PathBuf::from(
        r"C:\Program Files (x86)\Steam\steamapps\common\Le Mans Ultimate\UserData\player\direct input.json",
    )
}

pub fn default_lmu_settings_path() -> PathBuf {
    default_lmu_path().with_file_name("Settings.JSON")
}

fn validate_lmu_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("No LMU config was found at {}", path.display());
    }
    serde_json::from_slice::<serde_json::Value>(&fs::read(path)?)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let staged = path.with_extension("json.tmp");
    fs::write(&staged, serde_json::to_vec_pretty(value)?)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(staged, path)?;
    Ok(())
}

fn unix_time() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_profile_metadata_without_a_hotkey() {
        let profile: Profile = serde_json::from_value(serde_json::json!({
            "id": Uuid::nil(),
            "name": "Legacy profile",
            "created_at": 1,
            "assignments": []
        }))
        .unwrap();

        assert_eq!(profile.hotkey_slot, None);
    }

    #[test]
    fn captures_and_restores_a_profile() {
        let temp = tempfile::tempdir().unwrap();
        let live = temp.path().join("game").join(CONFIG_FILE_NAME);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, br#"{"wheel":"neo"}"#).unwrap();

        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        store.settings.lmu_config_path = live.clone();
        let profile = store.capture("GT Neo").unwrap();
        fs::write(&live, br#"{"wheel":"other"}"#).unwrap();

        let backup = store.activate(&profile).unwrap();
        assert_eq!(fs::read_to_string(live).unwrap(), r#"{"wheel":"neo"}"#);
        assert_eq!(fs::read_to_string(backup).unwrap(), r#"{"wheel":"other"}"#);
    }

    #[test]
    fn imports_a_named_preset_file() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("Porsche 963.json");
        fs::write(&preset, br#"{"wheel":"formula"}"#).unwrap();

        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();

        assert_eq!(profile.name, "Porsche 963");
        assert_eq!(store.profiles().unwrap().len(), 1);
        assert_eq!(
            fs::read_to_string(store.profile_dir(profile.id).join(CONFIG_FILE_NAME)).unwrap(),
            r#"{"wheel":"formula"}"#
        );
    }

    #[test]
    fn reads_primary_and_alternate_bindings() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("Hypercar.json");
        fs::write(
            &preset,
            br#"{
                "Input":{"Shift Up":{"device":"Wheel-123","id":44}},
                "Alternative Input":{"Shift Up":{"device":"Wheel-123","id":45}}
            }"#,
        )
        .unwrap();

        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();
        let bindings = store.bindings(&profile).unwrap();

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].action, "Shift Up");
        assert!(!bindings[0].alternate);
        assert!(bindings[1].alternate);
    }

    #[test]
    fn matches_a_button_by_device_hardware_id_across_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(
            &preset,
            br#"{
                "Devices":{"Wheel-123":{"product guid":"{05003670-0000-0000-0000-504944564944}"}},
                "Input":{"Shift Up":{"device":"Wheel-123","id":44}}
            }"#,
        )
        .unwrap();

        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();
        let matches = store
            .binding_matches(&[profile], 0x3670, 0x0500, 44)
            .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].profile_name, "GT3");
        assert_eq!(matches[0].action, "Shift Up");
    }

    #[test]
    fn detects_unsaved_changes_to_the_active_profile() {
        let temp = tempfile::tempdir().unwrap();
        let live = temp.path().join("game").join(CONFIG_FILE_NAME);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, br#"{"Input":{"Shift Up":1}}"#).unwrap();

        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        store.settings.lmu_config_path = live.clone();
        let profile = store.capture("Wheel").unwrap();
        store.settings.active_profile = Some(profile.id);

        assert_eq!(
            store.active_profile_has_unsaved_changes().unwrap(),
            Some(false)
        );
        fs::write(&live, br#"{"Input":{"Shift Up":2}}"#).unwrap();
        assert_eq!(
            store.active_profile_has_unsaved_changes().unwrap(),
            Some(true)
        );
    }

    #[test]
    fn updates_a_profile_from_live_bindings_with_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let live = temp.path().join("game").join(CONFIG_FILE_NAME);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, br#"{"Input":{"Shift Up":1}}"#).unwrap();

        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        store.settings.lmu_config_path = live.clone();
        let profile = store.capture("Wheel").unwrap();
        store.settings.active_profile = Some(profile.id);
        fs::write(&live, br#"{"Input":{"Shift Up":2}}"#).unwrap();

        let backup = store.update_profile(&profile).unwrap();

        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            r#"{"Input":{"Shift Up":1}}"#
        );
        assert_eq!(
            fs::read_to_string(store.profile_dir(profile.id).join(CONFIG_FILE_NAME)).unwrap(),
            r#"{"Input":{"Shift Up":2}}"#
        );
        assert_eq!(
            store.active_profile_has_unsaved_changes().unwrap(),
            Some(false)
        );
    }

    #[test]
    fn edits_bindings_and_saves_with_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(
            &preset,
            br#"{
                "Devices":{"Wheel-123":{"product guid":"{05003670-0000-0000-0000-504944564944}"}},
                "Input":{"Shift Up":{"device":"Wheel-123","id":44}},
                "Alternative Input":{"Shift Up":{"device":"Wheel-123","id":45}}
            }"#,
        )
        .unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let mut profile = store.import_profile(&preset).unwrap();
        profile.assignments = vec!["Shift Up".into()];
        let mut document = store.load_profile_document(&profile).unwrap();

        Store::set_binding(&mut document, "Shift Up", false, "Wheel-123", 52).unwrap();
        Store::clear_binding(&mut document, "Shift Up", true).unwrap();
        let backup = store.save_profile_document(&profile, &document).unwrap();

        assert!(fs::read_to_string(backup).unwrap().contains(r#""id":44"#));
        let bindings = store.bindings(&profile).unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].input_id, 52);
        assert!(!bindings[0].alternate);
        assert_eq!(
            Store::matching_device(&document, 0x3670, 0x0500).as_deref(),
            Some("Wheel-123")
        );
        assert_eq!(store.profiles().unwrap()[0].assignments, ["Shift Up"]);
    }

    #[test]
    fn edits_lmu_settings_with_descriptions_and_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let settings_path = temp.path().join("game").join("Settings.JSON");
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            br#"{
                "Visible Vehicles": 20,
                "Visible Vehicles#": "Maximum cars drawn",
                "Fullscreen": true
            }"#,
        )
        .unwrap();
        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        store.settings.lmu_settings_path = settings_path.clone();
        let mut document = store.load_lmu_settings_document().unwrap();
        document["Visible Vehicles"] = serde_json::json!(15);

        let backup = store.save_lmu_settings_document(&document).unwrap();

        let previous: serde_json::Value =
            serde_json::from_slice(&fs::read(backup).unwrap()).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(settings_path).unwrap()).unwrap();
        assert_eq!(previous["Visible Vehicles"], 20);
        assert_eq!(saved["Visible Vehicles"], 15);
        assert_eq!(saved["Visible Vehicles#"], "Maximum cars drawn");
    }

    #[test]
    fn derives_the_lmu_settings_path_for_legacy_lmukit_settings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lmukit");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("settings.json"),
            br#"{
                "lmu_config_path": "D:/LMU/UserData/player/direct input.json",
                "companion_apps": []
            }"#,
        )
        .unwrap();

        let store = Store::open_at(root).unwrap();

        assert_eq!(
            store.settings.lmu_settings_path,
            PathBuf::from("D:/LMU/UserData/player/Settings.JSON")
        );
    }

    #[test]
    fn creates_a_steam_companion_launcher() {
        let temp = tempfile::tempdir().unwrap();
        let lmukit = temp.path().join("lmukit.exe");
        let lmuffb = temp.path().join("LMUFFB.exe");
        fs::write(&lmukit, []).unwrap();
        fs::write(&lmuffb, []).unwrap();
        let mut store = Store::open_at(temp.path().join("data")).unwrap();
        store.settings.companion_apps[0].path = lmuffb;

        let option = store.companion_launch_option(&lmukit).unwrap();
        let launcher = store.root.join("launch-lmu-with-companions.cmd");

        assert!(option.starts_with("cmd /c \"\""));
        assert!(option.ends_with(" %command%\""));
        let script = fs::read_to_string(launcher).unwrap();
        assert!(script.contains("LMUFFB.exe"));
        assert!(script.contains("lmukit.exe"));
        assert!(script.contains("%*"));
    }

    #[test]
    fn excludes_disabled_companion_apps() {
        let temp = tempfile::tempdir().unwrap();
        let lmukit = temp.path().join("lmukit.exe");
        let crew_chief = temp.path().join("CrewChiefV4.exe");
        fs::write(&lmukit, []).unwrap();
        fs::write(&crew_chief, []).unwrap();
        let mut store = Store::open_at(temp.path().join("data")).unwrap();
        store.settings.companion_apps[1].path = crew_chief;
        store.settings.companion_apps[1].enabled = false;

        store.companion_launch_option(&lmukit).unwrap();
        let script = fs::read_to_string(store.root.join("launch-lmu-with-companions.cmd")).unwrap();
        assert!(!script.contains("CrewChiefV4.exe"));
    }
}

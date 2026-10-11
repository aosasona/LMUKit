use anyhow::{Context, Result, bail, ensure};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    #[serde(default)]
    pub wheel_image: Option<String>,
    #[serde(default)]
    pub wheel_tags: Vec<String>,
    #[serde(default)]
    pub wheel_brand: Option<String>,
    #[serde(default)]
    pub wheel_name: Option<String>,
    #[serde(default)]
    pub class_tags: Vec<String>,
    #[serde(default)]
    pub custom_tags: Vec<String>,
    #[serde(default)]
    pub wheel_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtual_mapping: Option<VirtualProfileMapping>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualProfileMapping {
    #[serde(default)]
    pub buttons: Vec<VirtualButtonMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encoder_groups: Vec<VirtualEncoderGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualEncoderGroup {
    pub name: String,
    pub clockwise: VirtualEncoderControl,
    pub counterclockwise: VirtualEncoderControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<VirtualEncoderControl>,
    pub modes: Vec<VirtualEncoderMode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualEncoderControl {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_name: String,
    pub physical_button: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualEncoderMode {
    pub name: String,
    pub clockwise_action: String,
    pub counterclockwise_action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualButtonMapping {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_name: String,
    pub physical_button: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<VirtualButtonModifier>,
    pub logical_action: String,
}

pub fn resolve_virtual_button_mapping<'a>(
    mapping: &'a VirtualProfileMapping,
    device_id: &str,
    physical_button: u32,
    held_modifiers: &[(String, u32)],
) -> Option<&'a VirtualButtonMapping> {
    mapping
        .buttons
        .iter()
        .filter(|entry| {
            entry.device_id == device_id
                && entry.physical_button == physical_button
                && entry.modifiers.iter().all(|modifier| {
                    held_modifiers.iter().any(|(held_device, held_button)| {
                        held_device == &modifier.device_id
                            && *held_button == modifier.physical_button
                    })
                })
        })
        .max_by_key(|entry| entry.modifiers.len())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualButtonModifier {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_name: String,
    pub physical_button: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualMappingImportReport {
    pub mapping: VirtualProfileMapping,
    pub added: usize,
    pub already_mapped: usize,
    pub skipped_without_virtual_action: usize,
    pub skipped_non_button: usize,
    pub skipped_unknown_device: usize,
    pub conflicting_controls: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wheel {
    pub id: Uuid,
    pub brand: Option<String>,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
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
    #[serde(default = "default_ui_font_scale")]
    pub ui_font_scale: f32,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: usize,
    #[serde(default)]
    pub lmu_launch_mode: LaunchMode,
    #[serde(default)]
    pub input_mode: InputMode,
    #[serde(default)]
    pub auto_start_virtual_profile: bool,
    #[serde(default)]
    pub last_virtual_profile: Option<Uuid>,
    #[serde(default)]
    pub virtual_layout: Vec<VirtualLayoutEntry>,
    #[serde(default)]
    pub virtual_device_key: Option<String>,
    #[serde(default = "default_virtual_device_id")]
    pub virtual_device_id: u32,
    #[serde(default)]
    pub virtual_profile_defaults: Vec<VirtualProfileDefault>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualProfileDefault {
    pub wheel_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_tag: Option<String>,
    pub profile_id: Uuid,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    #[default]
    Legacy,
    Virtual,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualLayoutEntry {
    pub logical_action: String,
    pub virtual_button: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    #[default]
    Desktop,
    Vr,
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
            ui_font_scale: default_ui_font_scale(),
            log_retention_days: default_log_retention_days(),
            lmu_launch_mode: LaunchMode::Desktop,
            input_mode: InputMode::Legacy,
            auto_start_virtual_profile: false,
            last_virtual_profile: None,
            virtual_layout: Vec::new(),
            virtual_device_key: None,
            virtual_device_id: default_virtual_device_id(),
            virtual_profile_defaults: Vec::new(),
        }
    }
}

fn default_ui_font_scale() -> f32 {
    1.0
}

fn default_log_retention_days() -> usize {
    5
}

fn default_virtual_device_id() -> u32 {
    1
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
        CompanionApp {
            name: "RaceLab".into(),
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
        fs::create_dir_all(root.join("wheels"))?;
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
        if !(0.85..=1.4).contains(&settings.ui_font_scale) {
            settings.ui_font_scale = default_ui_font_scale();
        }
        if !(1..=30).contains(&settings.log_retention_days) {
            settings.log_retention_days = default_log_retention_days();
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
        let store = Self { root, settings };
        store.migrate_profile_wheels()?;
        Ok(store)
    }

    pub fn save_settings(&self) -> Result<()> {
        validate_virtual_layout(&self.settings.virtual_layout)?;
        self.validate_virtual_profile_defaults()?;
        ensure!(
            (1..=16).contains(&self.settings.virtual_device_id),
            "Virtual device ID must be between 1 and 16."
        );
        ensure!(
            (1..=30).contains(&self.settings.log_retention_days),
            "Log retention must be between 1 and 30 days."
        );
        write_json(&self.root.join("settings.json"), &self.settings)
    }

    pub fn set_virtual_profile_default(
        &mut self,
        profile_id: Uuid,
        class_tag: Option<String>,
    ) -> Result<()> {
        let profile = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        let wheel_id = profile
            .wheel_id
            .context("Assign this profile to a wheel before making it a default.")?;
        let class_tag = match class_tag {
            Some(requested) => {
                let requested = requested.trim();
                profile
                    .class_tags
                    .iter()
                    .find(|tag| tag.eq_ignore_ascii_case(requested))
                    .cloned()
                    .with_context(|| {
                        format!("This profile is not tagged for the {requested} class.")
                    })?
                    .into()
            }
            None => None,
        };
        self.settings.virtual_profile_defaults.retain(|entry| {
            entry.wheel_id != wheel_id || !same_optional_tag(&entry.class_tag, &class_tag)
        });
        self.settings
            .virtual_profile_defaults
            .push(VirtualProfileDefault {
                wheel_id,
                class_tag,
                profile_id,
            });
        self.sort_virtual_profile_defaults();
        self.save_settings()
    }

    pub fn clear_virtual_profile_default(
        &mut self,
        wheel_id: Uuid,
        class_tag: Option<String>,
    ) -> Result<()> {
        let before = self.settings.virtual_profile_defaults.len();
        self.settings.virtual_profile_defaults.retain(|entry| {
            entry.wheel_id != wheel_id || !same_optional_tag(&entry.class_tag, &class_tag)
        });
        ensure!(
            self.settings.virtual_profile_defaults.len() != before,
            "That default assignment no longer exists."
        );
        self.save_settings()
    }

    pub fn resolve_virtual_profile(
        &self,
        wheel_id: Uuid,
        class_tag: Option<&str>,
    ) -> Result<Option<(Profile, bool)>> {
        let profiles = self.profiles()?;
        let exact = class_tag.and_then(|class_tag| {
            self.settings.virtual_profile_defaults.iter().find(|entry| {
                entry.wheel_id == wheel_id
                    && entry
                        .class_tag
                        .as_deref()
                        .is_some_and(|tag| tag.eq_ignore_ascii_case(class_tag))
            })
        });
        let selected = exact.or_else(|| {
            self.settings
                .virtual_profile_defaults
                .iter()
                .find(|entry| entry.wheel_id == wheel_id && entry.class_tag.is_none())
        });
        Ok(selected.and_then(|entry| {
            profiles
                .iter()
                .find(|profile| profile.id == entry.profile_id)
                .cloned()
                .map(|profile| (profile, exact.is_some()))
        }))
    }

    fn validate_virtual_profile_defaults(&self) -> Result<()> {
        let profiles = self.profiles()?;
        let wheels = self.wheels()?;
        let mut scopes = BTreeSet::new();
        for entry in &self.settings.virtual_profile_defaults {
            ensure!(
                wheels.iter().any(|wheel| wheel.id == entry.wheel_id),
                "A virtual profile default references a missing wheel."
            );
            let profile = profiles
                .iter()
                .find(|profile| profile.id == entry.profile_id)
                .context("A virtual profile default references a missing profile.")?;
            ensure!(
                profile.wheel_id == Some(entry.wheel_id),
                "A virtual profile default references a profile assigned to another wheel."
            );
            if let Some(class_tag) = &entry.class_tag {
                ensure!(
                    profile
                        .class_tags
                        .iter()
                        .any(|tag| tag.eq_ignore_ascii_case(class_tag)),
                    "A virtual profile default references a class missing from its profile."
                );
            }
            ensure!(
                scopes.insert((
                    entry.wheel_id,
                    entry.class_tag.as_deref().map(str::to_lowercase)
                )),
                "Only one default profile may be assigned per wheel and class."
            );
        }
        Ok(())
    }

    fn sort_virtual_profile_defaults(&mut self) {
        self.settings
            .virtual_profile_defaults
            .sort_by(|left, right| {
                left.wheel_id.cmp(&right.wheel_id).then_with(|| {
                    left.class_tag
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .cmp(&right.class_tag.as_deref().unwrap_or("").to_lowercase())
                })
            });
    }

    pub fn set_profile_virtual_mapping(
        &self,
        profile_id: Uuid,
        mapping: Option<VirtualProfileMapping>,
    ) -> Result<()> {
        if let Some(mapping) = &mapping {
            validate_virtual_mapping(mapping, &self.settings.virtual_layout)?;
        }
        let mut profile = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        profile.virtual_mapping = mapping;
        self.save_profile_metadata(&profile)
    }

    pub fn import_profile_virtual_mapping(
        &self,
        profile_id: Uuid,
    ) -> Result<VirtualMappingImportReport> {
        let profile = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        let document = self.load_profile_document(&profile)?;
        let layout = self
            .settings
            .virtual_layout
            .iter()
            .map(|entry| {
                (
                    entry.logical_action.to_lowercase(),
                    entry.logical_action.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        ensure!(
            !layout.is_empty(),
            "Set up and save the LMU virtual-button layout before importing profile buttons."
        );

        let mut report = VirtualMappingImportReport {
            mapping: profile.virtual_mapping.unwrap_or_default(),
            ..Default::default()
        };
        let existing_sources = report
            .mapping
            .buttons
            .iter()
            .map(|entry| {
                (
                    entry.device_id.clone(),
                    entry.physical_button,
                    entry
                        .modifiers
                        .iter()
                        .map(|modifier| (modifier.device_id.clone(), modifier.physical_button))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<BTreeSet<_>>();
        let mut candidates = BTreeMap::<(String, String, u32), BTreeSet<String>>::new();

        for binding in Self::bindings_from_document(&document) {
            if binding.input_id < 32 {
                report.skipped_non_button += 1;
                continue;
            }
            let Ok(physical_button) = u32::try_from(binding.input_id - 31) else {
                report.skipped_non_button += 1;
                continue;
            };
            if physical_button > 128 {
                report.skipped_non_button += 1;
                continue;
            }
            let Some(logical_action) = layout.get(&binding.action.to_lowercase()).cloned() else {
                report.skipped_without_virtual_action += 1;
                continue;
            };
            if self.settings.virtual_device_key.as_deref() == Some(binding.device.as_str()) {
                continue;
            }
            let Some((device_id, device_name)) =
                device_hardware_identity(&document, &binding.device)
            else {
                report.skipped_unknown_device += 1;
                continue;
            };
            if device_name.to_lowercase().contains("vjoy") {
                continue;
            }
            candidates
                .entry((device_id, device_name, physical_button))
                .or_default()
                .insert(logical_action);
        }

        for ((device_id, device_name, physical_button), actions) in candidates {
            if existing_sources.contains(&(device_id.clone(), physical_button, Vec::new())) {
                report.already_mapped += 1;
                continue;
            }
            if actions.len() != 1 {
                report.conflicting_controls += 1;
                continue;
            }
            report.mapping.buttons.push(VirtualButtonMapping {
                device_id,
                device_name,
                physical_button,
                modifiers: Vec::new(),
                logical_action: actions.into_iter().next().expect("one action"),
            });
            report.added += 1;
        }
        report.mapping.buttons.sort_by(|left, right| {
            left.logical_action
                .to_lowercase()
                .cmp(&right.logical_action.to_lowercase())
                .then_with(|| left.device_id.cmp(&right.device_id))
                .then_with(|| left.physical_button.cmp(&right.physical_button))
        });
        let saved = (!report.mapping.buttons.is_empty()).then_some(report.mapping.clone());
        self.set_profile_virtual_mapping(profile_id, saved)?;
        Ok(report)
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
        script.push_str(match self.settings.lmu_launch_mode {
            LaunchMode::Desktop => "%*\r\n",
            LaunchMode::Vr => "%* +XR\r\n",
        });
        fs::write(&launcher, script).context("could not create the companion launcher")?;
        Ok(format!("cmd /c \"\"{}\" %command%\"", launcher.display()))
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn wheels(&self) -> Result<Vec<Wheel>> {
        let mut wheels: Vec<Wheel> = Vec::new();
        for entry in fs::read_dir(self.root.join("wheels"))? {
            let path = entry?.path().join("wheel.json");
            if path.is_file() {
                wheels.push(serde_json::from_slice(&fs::read(path)?)?);
            }
        }
        wheels.sort_by_key(|wheel| {
            format!("{} {}", wheel.brand.as_deref().unwrap_or(""), wheel.name).to_lowercase()
        });
        Ok(wheels)
    }

    pub fn create_wheel(&self, brand: Option<String>, name: String) -> Result<Wheel> {
        let name = normalize_optional_label(Some(name))?.context("Enter a wheel name.")?;
        let brand = normalize_optional_label(brand)?;
        if self.wheels()?.iter().any(|wheel| {
            wheel.name.eq_ignore_ascii_case(&name)
                && wheel
                    .brand
                    .as_deref()
                    .unwrap_or("")
                    .eq_ignore_ascii_case(brand.as_deref().unwrap_or(""))
        }) {
            bail!("That wheel already exists in the library.");
        }
        let wheel = Wheel {
            id: Uuid::new_v4(),
            brand,
            name,
            image: None,
        };
        self.save_wheel(&wheel)?;
        Ok(wheel)
    }

    pub fn update_wheel(&self, id: Uuid, brand: Option<String>, name: String) -> Result<()> {
        let name = normalize_optional_label(Some(name))?.context("Enter a wheel name.")?;
        let brand = normalize_optional_label(brand)?;
        if self.wheels()?.iter().any(|wheel| {
            wheel.id != id
                && wheel.name.eq_ignore_ascii_case(&name)
                && wheel
                    .brand
                    .as_deref()
                    .unwrap_or("")
                    .eq_ignore_ascii_case(brand.as_deref().unwrap_or(""))
        }) {
            bail!("That wheel already exists in the library.");
        }
        let mut wheel = self.wheel(id)?;
        wheel.brand = brand;
        wheel.name = name;
        self.save_wheel(&wheel)
    }

    pub fn delete_wheel(&mut self, id: Uuid) -> Result<()> {
        if self
            .profiles()?
            .iter()
            .any(|profile| profile.wheel_id == Some(id))
        {
            bail!("This wheel is still assigned to one or more profiles.");
        }
        let directory = self.wheel_dir(id);
        if !directory.is_dir() {
            bail!("That wheel no longer exists.");
        }
        fs::remove_dir_all(directory).context("could not delete the wheel")?;
        self.settings
            .virtual_profile_defaults
            .retain(|entry| entry.wheel_id != id);
        self.save_settings()
    }

    pub fn assign_profile_wheel(&mut self, profile_id: Uuid, wheel_id: Option<Uuid>) -> Result<()> {
        if let Some(id) = wheel_id
            && !self.wheels()?.iter().any(|wheel| wheel.id == id)
        {
            bail!("That wheel no longer exists.");
        }
        let mut profile = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        if profile.wheel_id != wheel_id {
            self.settings
                .virtual_profile_defaults
                .retain(|entry| entry.profile_id != profile_id);
        }
        profile.wheel_id = wheel_id;
        self.save_profile_metadata(&profile)?;
        self.save_settings()
    }

    pub fn wheel(&self, id: Uuid) -> Result<Wheel> {
        self.wheels()?
            .into_iter()
            .find(|wheel| wheel.id == id)
            .context("That wheel no longer exists.")
    }

    pub fn save_wheel_image(&self, wheel: &mut Wheel, bytes: &[u8]) -> Result<()> {
        validate_image_bytes(bytes)?;
        let extension = image_extension(bytes).expect("validated wheel image");
        let filename = format!("wheel-image.{extension}");
        let directory = self.wheel_dir(wheel.id);
        fs::create_dir_all(&directory)?;
        let destination = directory.join(&filename);
        let staged = directory.join(format!(".wheel-image.{extension}.tmp"));
        fs::write(&staged, bytes).context("could not stage the wheel image")?;
        replace_file(&staged, &destination).context("could not save the wheel image")?;
        let previous = wheel.image.replace(filename);
        self.save_wheel(wheel)?;
        if let Some(previous) = previous
            && previous != wheel.image.as_deref().unwrap_or_default()
        {
            let path = self.safe_wheel_asset_path(wheel.id, &previous)?;
            if path.is_file() {
                fs::remove_file(path).context("could not remove the previous wheel image")?;
            }
        }
        Ok(())
    }

    pub fn wheel_image(&self, wheel: &Wheel) -> Result<Option<Vec<u8>>> {
        let Some(filename) = &wheel.image else {
            return Ok(None);
        };
        let path = self.safe_wheel_asset_path(wheel.id, filename)?;
        Ok(path.is_file().then(|| fs::read(path)).transpose()?)
    }

    pub fn remove_wheel_image(&self, wheel: &mut Wheel) -> Result<()> {
        if let Some(filename) = wheel.image.take() {
            let path = self.safe_wheel_asset_path(wheel.id, &filename)?;
            if path.is_file() {
                fs::remove_file(path).context("could not remove the wheel image")?;
            }
            self.save_wheel(wheel)?;
        }
        Ok(())
    }

    fn save_wheel(&self, wheel: &Wheel) -> Result<()> {
        fs::create_dir_all(self.wheel_dir(wheel.id))?;
        write_json(&self.wheel_dir(wheel.id).join("wheel.json"), wheel)
            .context("could not save wheel metadata")
    }

    fn wheel_dir(&self, id: Uuid) -> PathBuf {
        self.root.join("wheels").join(id.to_string())
    }

    fn safe_wheel_asset_path(&self, id: Uuid, filename: &str) -> Result<PathBuf> {
        let path = Path::new(filename);
        if path.file_name().and_then(|name| name.to_str()) != Some(filename) {
            bail!("the wheel contains an invalid image filename");
        }
        Ok(self.wheel_dir(id).join(path))
    }

    fn migrate_profile_wheels(&self) -> Result<()> {
        let mut wheels = self.wheels()?;
        for mut profile in self.profiles()? {
            if profile.wheel_id.is_some() {
                continue;
            }
            let Some(name) = profile
                .wheel_name
                .clone()
                .or_else(|| profile.wheel_tags.first().cloned())
            else {
                continue;
            };
            let brand = profile.wheel_brand.clone();
            let wheel = if let Some(existing) = wheels.iter().find(|wheel| {
                wheel.name.eq_ignore_ascii_case(&name)
                    && wheel
                        .brand
                        .as_deref()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(brand.as_deref().unwrap_or(""))
            }) {
                existing.clone()
            } else {
                let mut wheel = self.create_wheel(brand, name)?;
                if let Ok(Some(bytes)) = self.profile_wheel_image(&profile) {
                    self.save_wheel_image(&mut wheel, &bytes)?;
                }
                wheels.push(wheel.clone());
                wheel
            };
            profile.wheel_id = Some(wheel.id);
            self.save_profile_metadata(&profile)?;
        }
        Ok(())
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

    pub fn set_profile_hotkey(&self, profile_id: Uuid, slot: Option<u8>) -> Result<()> {
        if slot.is_some_and(|slot| !(1..=9).contains(&slot)) {
            bail!("Profile shortcuts must use a number from 1 to 9.");
        }
        let mut profiles = self.profiles()?;
        if !profiles.iter().any(|profile| profile.id == profile_id) {
            bail!("That profile no longer exists.");
        }
        for profile in &mut profiles {
            let next = if profile.id == profile_id {
                slot
            } else if profile.hotkey_slot == slot {
                None
            } else {
                profile.hotkey_slot
            };
            if next != profile.hotkey_slot {
                profile.hotkey_slot = next;
                self.save_profile_metadata(profile)?;
            }
        }
        Ok(())
    }

    pub fn rename_profile(&self, profile_id: Uuid, name: String) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Enter a profile name.");
        }
        if name.chars().count() > 100 {
            bail!("Profile names must be 100 characters or shorter.");
        }
        let profiles = self.profiles()?;
        if profiles
            .iter()
            .any(|profile| profile.id != profile_id && profile.name.eq_ignore_ascii_case(name))
        {
            bail!("A profile with that name already exists.");
        }
        let mut profile = profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        profile.name = name.to_owned();
        self.save_profile_metadata(&profile)
    }

    pub fn set_profile_categories(
        &mut self,
        profile_id: Uuid,
        class_tags: Vec<String>,
        custom_tags: Vec<String>,
    ) -> Result<()> {
        let mut profile = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .context("That profile no longer exists.")?;
        profile.class_tags = normalize_tags(class_tags)?;
        profile.custom_tags = normalize_tags(custom_tags)?;
        profile.wheel_tags.clear();
        self.settings.virtual_profile_defaults.retain(|entry| {
            entry.profile_id != profile_id
                || entry.class_tag.as_ref().is_none_or(|class_tag| {
                    profile
                        .class_tags
                        .iter()
                        .any(|tag| tag.eq_ignore_ascii_case(class_tag))
                })
        });
        self.save_profile_metadata(&profile)?;
        self.save_settings()
    }

    pub fn save_profile_wheel_image(&self, profile: &mut Profile, bytes: &[u8]) -> Result<()> {
        validate_image_bytes(bytes)?;
        let extension = image_extension(bytes).expect("validated wheel image");
        if let Some(existing) = &profile.wheel_image {
            let existing = self.safe_profile_asset_path(profile.id, existing)?;
            if existing.is_file() {
                fs::remove_file(existing).context("could not replace the previous wheel image")?;
            }
        }
        let filename = format!("wheel-image.{extension}");
        fs::write(self.profile_dir(profile.id).join(&filename), bytes)
            .context("could not save the wheel image")?;
        profile.wheel_image = Some(filename);
        self.save_profile_metadata(profile)
    }

    pub fn profile_wheel_image(&self, profile: &Profile) -> Result<Option<Vec<u8>>> {
        let Some(filename) = &profile.wheel_image else {
            return Ok(None);
        };
        let path = self.safe_profile_asset_path(profile.id, filename)?;
        Ok(path.is_file().then(|| fs::read(path)).transpose()?)
    }

    pub fn remove_profile_wheel_image(&self, profile: &mut Profile) -> Result<()> {
        if let Some(filename) = profile.wheel_image.take() {
            let path = self.safe_profile_asset_path(profile.id, &filename)?;
            if path.is_file() {
                fs::remove_file(path).context("could not remove the wheel image")?;
            }
            self.save_profile_metadata(profile)?;
        }
        Ok(())
    }

    fn safe_profile_asset_path(&self, profile_id: Uuid, filename: &str) -> Result<PathBuf> {
        let path = Path::new(filename);
        if path.file_name().and_then(|name| name.to_str()) != Some(filename) {
            bail!("the profile contains an invalid image filename");
        }
        Ok(self.profile_dir(profile_id).join(path))
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

    pub fn import_profile_bytes(&self, name: &str, bytes: &[u8]) -> Result<Profile> {
        validate_lmu_bytes(bytes, name)?;
        let name = Path::new(name.trim())
            .file_stem()
            .and_then(|name| name.to_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .context("The imported preset has no usable name.")?;
        let profile = Profile {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: unix_time()?,
            assignments: Vec::new(),
            hotkey_slot: None,
            wheel_image: None,
            wheel_tags: Vec::new(),
            wheel_brand: None,
            wheel_name: None,
            class_tags: Vec::new(),
            custom_tags: Vec::new(),
            wheel_id: None,
            virtual_mapping: None,
        };
        let dir = self.profile_dir(profile.id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(CONFIG_FILE_NAME), bytes)
            .context("could not store the imported preset")?;
        write_json(&dir.join("profile.json"), &profile)?;
        Ok(profile)
    }

    pub fn bindings(&self, profile: &Profile) -> Result<Vec<Binding>> {
        let document = self.profile_document(profile)?;
        Ok(Self::bindings_from_document(&document))
    }

    pub fn load_live_bindings_document(&self) -> Result<serde_json::Value> {
        let path = &self.settings.lmu_config_path;
        let document: serde_json::Value = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("could not read {}", path.display()))?,
        )
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
        ensure!(
            document.is_object(),
            "{} does not contain a JSON object",
            path.display()
        );
        Ok(document)
    }

    pub fn save_virtual_layout(
        &mut self,
        device_key: String,
        layout: Vec<VirtualLayoutEntry>,
    ) -> Result<()> {
        validate_virtual_label(&device_key, "Virtual device key")?;
        validate_virtual_layout(&layout)?;
        ensure!(
            !layout.is_empty(),
            "No virtual button bindings were found for that LMU device."
        );
        self.settings.virtual_device_key = Some(device_key);
        self.settings.virtual_layout = layout;
        self.save_settings()
    }

    pub fn used_virtual_buttons_from_document(
        document: &serde_json::Value,
        virtual_device: &str,
    ) -> Result<Vec<u32>> {
        ensure!(
            document
                .get("Devices")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|devices| devices.contains_key(virtual_device)),
            "The selected virtual device is not registered in this LMU binding file."
        );
        let mut buttons = BTreeSet::new();
        for binding in Self::bindings_from_document(document)
            .into_iter()
            .filter(|binding| binding.device == virtual_device && binding.input_id >= 32)
        {
            let virtual_button = u32::try_from(binding.input_id - 31)
                .context("LMU virtual button ID is too large")?;
            ensure!(
                virtual_button <= 128,
                "LMU action '{}' uses virtual button B{virtual_button}, outside the supported B1..B128 range.",
                binding.action
            );
            buttons.insert(virtual_button);
        }
        Ok(buttons.into_iter().collect())
    }

    pub fn bindings_from_document(document: &serde_json::Value) -> Vec<Binding> {
        let mut bindings = Vec::new();
        collect_bindings(document, "Input", false, &mut bindings);
        collect_bindings(document, "Alternative Input", true, &mut bindings);
        bindings.sort_by(|a, b| a.action.to_lowercase().cmp(&b.action.to_lowercase()));
        bindings
    }

    pub fn binding_actions_from_document(document: &serde_json::Value) -> Vec<String> {
        let mut actions = BTreeSet::new();
        for section in ["Input", "Alternative Input"] {
            if let Some(entries) = document.get(section).and_then(serde_json::Value::as_object) {
                actions.extend(entries.keys().cloned());
            }
        }
        actions.into_iter().collect()
    }

    pub fn virtual_layout_from_document(
        document: &serde_json::Value,
        virtual_device: &str,
    ) -> Result<Vec<VirtualLayoutEntry>> {
        ensure!(
            document
                .get("Devices")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|devices| devices.contains_key(virtual_device)),
            "The selected virtual device is not registered in this LMU binding file."
        );

        let mut actions = BTreeMap::<String, u32>::new();
        let mut buttons = BTreeMap::<u32, String>::new();
        for binding in Self::bindings_from_document(document)
            .into_iter()
            .filter(|binding| binding.device == virtual_device && binding.input_id >= 32)
        {
            let virtual_button = u32::try_from(binding.input_id - 31)
                .context("LMU virtual button ID is too large")?;
            ensure!(
                virtual_button <= 128,
                "LMU action '{}' uses virtual button B{virtual_button}, outside the supported B1..B128 range.",
                binding.action
            );
            if let Some(existing_button) = actions.get(&binding.action) {
                ensure!(
                    *existing_button == virtual_button,
                    "LMU action '{}' is bound to both B{existing_button} and B{virtual_button} on the virtual device.",
                    binding.action
                );
            }
            if let Some(existing_action) = buttons.get(&virtual_button) {
                ensure!(
                    existing_action == &binding.action,
                    "Virtual button B{virtual_button} is bound to both '{}' and '{}' in LMU.",
                    existing_action,
                    binding.action
                );
            }
            actions.insert(binding.action.clone(), virtual_button);
            buttons.insert(virtual_button, binding.action);
        }

        let layout = buttons
            .into_iter()
            .map(|(virtual_button, logical_action)| VirtualLayoutEntry {
                logical_action,
                virtual_button,
            })
            .collect::<Vec<_>>();
        validate_virtual_layout(&layout)?;
        Ok(layout)
    }

    pub fn binding_action_catalogue(&self) -> Result<Vec<String>> {
        let mut actions = BTreeSet::new();
        for profile in self.profiles()? {
            actions.extend(Self::binding_actions_from_document(
                &self.load_profile_document(&profile)?,
            ));
        }
        if self.settings.lmu_config_path.is_file()
            && let Ok(bytes) = fs::read(&self.settings.lmu_config_path)
            && let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            actions.extend(Self::binding_actions_from_document(&document));
        }
        Ok(actions.into_iter().collect())
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

    pub fn assign_profile_binding(
        &self,
        profile: &Profile,
        action: &str,
        alternate: bool,
        device: &str,
        input_id: u64,
    ) -> Result<PathBuf> {
        let mut document = self.load_profile_document(profile)?;
        let action = action.trim();
        if action.is_empty() || action.len() > 128 || action.chars().any(char::is_control) {
            bail!("enter a valid LMU action name of 128 characters or fewer");
        }
        let device_exists = document
            .get("Devices")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|devices| devices.contains_key(device));
        if !device_exists {
            bail!("the detected device is not registered in this profile");
        }
        Self::set_binding(&mut document, action, alternate, device, input_id)?;
        self.save_profile_document(profile, &document)
    }

    pub fn clear_binding(
        document: &mut serde_json::Value,
        action: &str,
        alternate: bool,
    ) -> Result<()> {
        binding_section_mut(document, alternate)?.remove(action);
        Ok(())
    }

    pub fn remove_device(document: &mut serde_json::Value, device: &str) -> Result<usize> {
        let devices = document
            .get_mut("Devices")
            .and_then(serde_json::Value::as_object_mut)
            .context("the profile does not contain a valid Devices object")?;
        if devices.remove(device).is_none() {
            bail!("the selected device no longer exists in this profile");
        }

        let mut removed = 0;
        for section in ["Input", "Alternative Input"] {
            let Some(bindings) = document
                .get_mut(section)
                .and_then(serde_json::Value::as_object_mut)
            else {
                continue;
            };
            bindings.retain(|_, mapping| {
                let belongs_to_device =
                    mapping.get("device").and_then(serde_json::Value::as_str) == Some(device);
                removed += usize::from(belongs_to_device);
                !belongs_to_device
            });
        }
        Ok(removed)
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
        let Some(profile) = self
            .profiles()?
            .into_iter()
            .find(|profile| profile.id == active_id)
        else {
            return Ok(None);
        };
        self.profile_has_live_changes(&profile)
    }

    pub fn profile_has_live_changes(&self, profile: &Profile) -> Result<Option<bool>> {
        let saved_path = self.profile_dir(profile.id).join(CONFIG_FILE_NAME);
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
            wheel_image: None,
            wheel_tags: Vec::new(),
            wheel_brand: None,
            wheel_name: None,
            class_tags: Vec::new(),
            custom_tags: Vec::new(),
            wheel_id: None,
            virtual_mapping: None,
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
        let defaults_changed = self
            .settings
            .virtual_profile_defaults
            .iter()
            .any(|entry| entry.profile_id == profile.id);
        self.settings
            .virtual_profile_defaults
            .retain(|entry| entry.profile_id != profile.id);
        let active_changed = self.settings.active_profile == Some(profile.id);
        let last_virtual_changed = self.settings.last_virtual_profile == Some(profile.id);
        if active_changed {
            self.settings.active_profile = None;
        }
        if last_virtual_changed {
            self.settings.last_virtual_profile = None;
        }
        if active_changed || last_virtual_changed || defaults_changed {
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
    if destination.exists() {
        fs::remove_file(destination)?;
    }
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
    device_usb_ids(document, device).is_some_and(|ids| ids == (vendor_id, product_id))
}

fn device_usb_ids(document: &serde_json::Value, device: &str) -> Option<(u16, u16)> {
    let guid = document
        .get("Devices")
        .and_then(|devices| devices.get(device))
        .and_then(|device| device.get("product guid"))
        .and_then(serde_json::Value::as_str)?;
    let data = guid.strip_prefix('{').and_then(|guid| guid.get(..8))?;
    let id = u32::from_str_radix(data, 16).ok()?;
    Some((id as u16, (id >> 16) as u16))
}

fn device_hardware_identity(
    document: &serde_json::Value,
    device: &str,
) -> Option<(String, String)> {
    let (vendor_id, product_id) = device_usb_ids(document, device)?;
    let name = document
        .get("Devices")
        .and_then(|devices| devices.get(device))
        .and_then(|device| device.get("name"))
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(device)
        .to_owned();
    Some((format!("hid:{vendor_id:04x}:{product_id:04x}"), name))
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
    validate_lmu_bytes(&fs::read(path)?, &path.display().to_string())
}

fn validate_lmu_bytes(bytes: &[u8], label: &str) -> Result<()> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).with_context(|| format!("{label} is not valid JSON"))?;
    if !document.is_object() {
        bail!("{label} does not contain a JSON object");
    }
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

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn validate_image_bytes(bytes: &[u8]) -> Result<()> {
    const MAX_IMAGE_SIZE: usize = 8 * 1024 * 1024;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_SIZE {
        bail!("Wheel images must be between 1 byte and 8 MB.");
    }
    if image_extension(bytes).is_none() {
        bail!("Wheel images must be PNG, JPEG, or WebP files.");
    }
    Ok(())
}

fn validate_virtual_layout(layout: &[VirtualLayoutEntry]) -> Result<()> {
    ensure!(
        layout.len() <= 128,
        "Virtual layout cannot contain more than 128 actions."
    );
    let mut actions = BTreeSet::new();
    let mut buttons = BTreeSet::new();
    for entry in layout {
        validate_virtual_label(&entry.logical_action, "Logical action")?;
        ensure!(
            actions.insert(entry.logical_action.to_lowercase()),
            "Logical action '{}' appears more than once.",
            entry.logical_action
        );
        ensure!(
            (1..=128).contains(&entry.virtual_button),
            "Virtual button {} is outside the supported range 1..=128.",
            entry.virtual_button
        );
        ensure!(
            buttons.insert(entry.virtual_button),
            "Virtual button {} is assigned more than once.",
            entry.virtual_button
        );
    }
    Ok(())
}

fn validate_virtual_mapping(
    mapping: &VirtualProfileMapping,
    layout: &[VirtualLayoutEntry],
) -> Result<()> {
    validate_virtual_layout(layout)?;
    ensure!(
        mapping.buttons.len() <= 512,
        "A virtual profile cannot contain more than 512 button mappings."
    );
    let actions = layout
        .iter()
        .map(|entry| entry.logical_action.as_str())
        .collect::<BTreeSet<_>>();
    let mut sources = BTreeSet::new();
    let mut occupied_controls = BTreeSet::new();
    for entry in &mapping.buttons {
        validate_virtual_label(&entry.device_id, "Input device ID")?;
        validate_virtual_label(&entry.logical_action, "Logical action")?;
        ensure!(
            entry.physical_button > 0,
            "Physical button IDs must be greater than zero."
        );
        occupied_controls.insert((entry.device_id.clone(), entry.physical_button));
        ensure!(
            entry.modifiers.len() <= 4,
            "A button mapping cannot have more than four modifiers."
        );
        let mut modifiers = BTreeSet::new();
        for modifier in &entry.modifiers {
            validate_virtual_label(&modifier.device_id, "Modifier device ID")?;
            ensure!(
                modifier.physical_button > 0,
                "Modifier button IDs must be greater than zero."
            );
            occupied_controls.insert((modifier.device_id.clone(), modifier.physical_button));
            ensure!(
                !(modifier.device_id == entry.device_id
                    && modifier.physical_button == entry.physical_button),
                "A physical button cannot modify itself."
            );
            ensure!(
                modifiers.insert((modifier.device_id.clone(), modifier.physical_button)),
                "The same modifier button cannot appear twice."
            );
        }
        ensure!(
            sources.insert((
                entry.device_id.clone(),
                entry.physical_button,
                modifiers.into_iter().collect::<Vec<_>>()
            )),
            "The same physical button chord is mapped more than once."
        );
        ensure!(
            actions.contains(entry.logical_action.as_str()),
            "Logical action '{}' is missing from the shared virtual layout.",
            entry.logical_action
        );
    }
    ensure!(
        mapping.encoder_groups.len() <= 8,
        "A virtual profile cannot contain more than eight encoders."
    );
    let mut encoder_names = BTreeSet::new();
    for encoder in &mapping.encoder_groups {
        validate_virtual_label(&encoder.name, "Encoder name")?;
        ensure!(
            encoder_names.insert(encoder.name.to_lowercase()),
            "Encoder names must be unique."
        );
        ensure!(
            !encoder.modes.is_empty() && encoder.modes.len() <= 8,
            "An encoder must contain between one and eight modes."
        );
        ensure!(
            encoder.modes.len() == 1 || encoder.cycle.is_some(),
            "An encoder with multiple modes requires a mode control."
        );
        for control in [&encoder.clockwise, &encoder.counterclockwise]
            .into_iter()
            .chain(encoder.cycle.iter())
        {
            validate_virtual_label(&control.device_id, "Encoder device ID")?;
            ensure!(
                control.physical_button > 0,
                "Encoder button IDs must be greater than zero."
            );
            ensure!(
                occupied_controls.insert((control.device_id.clone(), control.physical_button)),
                "Encoder controls cannot be shared with another profile control."
            );
        }
        let mut mode_names = BTreeSet::new();
        for mode in &encoder.modes {
            validate_virtual_label(&mode.name, "Encoder mode name")?;
            ensure!(
                mode_names.insert(mode.name.to_lowercase()),
                "Encoder mode names must be unique within an encoder."
            );
            for action in [&mode.clockwise_action, &mode.counterclockwise_action] {
                validate_virtual_label(action, "Encoder logical action")?;
                ensure!(
                    actions.contains(action.as_str()),
                    "Encoder logical action '{action}' is missing from the shared virtual layout."
                );
            }
        }
    }
    Ok(())
}

fn validate_virtual_label(value: &str, label: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{label} must not be empty.");
    ensure!(value == value.trim(), "{label} must not have outer spaces.");
    ensure!(
        value.chars().count() <= 120,
        "{label} must be 120 characters or shorter."
    );
    Ok(())
}

fn same_optional_tag(left: &Option<String>, right: &Option<String>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        (None, None) => true,
        _ => false,
    }
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 40 {
            bail!("Category names must be 40 characters or shorter.");
        }
        if !normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(tag))
        {
            normalized.push(tag.to_owned());
        }
    }
    if normalized.len() > 12 {
        bail!("A profile can have at most 12 categories of each type.");
    }
    normalized.sort_by_key(|tag| tag.to_lowercase());
    Ok(normalized)
}

fn normalize_optional_label(value: Option<String>) -> Result<Option<String>> {
    let value = value.unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 60 {
        bail!("Wheel brand and name must be 60 characters or shorter.");
    }
    Ok(Some(value.to_owned()))
}

fn unix_time() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_most_specific_virtual_button_mapping() {
        let mapping = VirtualProfileMapping {
            buttons: vec![
                VirtualButtonMapping {
                    device_id: "wheel".into(),
                    device_name: "Test wheel".into(),
                    physical_button: 5,
                    modifiers: Vec::new(),
                    logical_action: "pit_limiter".into(),
                },
                VirtualButtonMapping {
                    device_id: "wheel".into(),
                    device_name: "Test wheel".into(),
                    physical_button: 5,
                    modifiers: vec![VirtualButtonModifier {
                        device_id: "wheel".into(),
                        device_name: "Test wheel".into(),
                        physical_button: 1,
                    }],
                    logical_action: "recenter_vr".into(),
                },
            ],
            encoder_groups: Vec::new(),
        };

        let base = resolve_virtual_button_mapping(&mapping, "wheel", 5, &[]).unwrap();
        assert_eq!(base.logical_action, "pit_limiter");

        let modified =
            resolve_virtual_button_mapping(&mapping, "wheel", 5, &[("wheel".into(), 1)]).unwrap();
        assert_eq!(modified.logical_action, "recenter_vr");

        let unrelated =
            resolve_virtual_button_mapping(&mapping, "wheel", 5, &[("wheel".into(), 9)]).unwrap();
        assert_eq!(unrelated.logical_action, "pit_limiter");
        assert!(resolve_virtual_button_mapping(&mapping, "wheel", 8, &[]).is_none());
    }

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
        assert!(profile.wheel_tags.is_empty());
        assert_eq!(profile.wheel_brand, None);
        assert_eq!(profile.wheel_name, None);
        assert!(profile.class_tags.is_empty());
        assert!(profile.custom_tags.is_empty());
        assert_eq!(profile.virtual_mapping, None);
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
    fn imports_uploaded_profile_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store
            .import_profile_bytes("First.json", br#"{"Input":{}}"#)
            .unwrap();
        assert_eq!(profile.name, "First");
        assert_eq!(store.profiles().unwrap().len(), 1);
        assert!(store.import_profile_bytes("bad", b"[]").is_err());
    }

    #[test]
    fn saves_normalized_profile_categories() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store
            .import_profile_bytes("GT3", br#"{"Input":{}}"#)
            .unwrap();

        let wheel = store
            .create_wheel(Some(" Simagic ".into()), " GT Neo ".into())
            .unwrap();
        store
            .assign_profile_wheel(profile.id, Some(wheel.id))
            .unwrap();
        store
            .set_profile_categories(
                profile.id,
                vec!["GT3".into(), "Hypercar".into()],
                vec!["Formula".into(), "formula".into()],
            )
            .unwrap();

        let saved = store.profiles().unwrap().remove(0);
        assert_eq!(saved.wheel_id, Some(wheel.id));
        assert_eq!(
            store.wheel(wheel.id).unwrap().brand.as_deref(),
            Some("Simagic")
        );
        assert_eq!(store.wheel(wheel.id).unwrap().name, "GT Neo");
        assert_eq!(saved.class_tags, ["GT3", "Hypercar"]);
        assert_eq!(saved.custom_tags, ["Formula"]);
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
    fn derives_named_virtual_actions_from_lmu_button_bindings() {
        let document = serde_json::json!({
            "Devices": {
                "vJoy-1": {"name": "vJoy Device"},
                "Wheel-1": {"name": "SIMAGIC Alpha EVO Wheelbase"}
            },
            "Input": {
                "Pit Limiter": {"device": "vJoy-1", "id": 32},
                "Headlights": {"device": "vJoy-1", "id": 33},
                "Steer": {"device": "vJoy-1", "id": 0},
                "Ignition": {"device": "Wheel-1", "id": 34}
            },
            "Alternative Input": {
                "Wipers": {"device": "vJoy-1", "id": 35}
            }
        });

        let layout = Store::virtual_layout_from_document(&document, "vJoy-1").unwrap();
        let used = Store::used_virtual_buttons_from_document(&document, "vJoy-1").unwrap();

        assert_eq!(
            layout,
            vec![
                VirtualLayoutEntry {
                    logical_action: "Pit Limiter".into(),
                    virtual_button: 1,
                },
                VirtualLayoutEntry {
                    logical_action: "Headlights".into(),
                    virtual_button: 2,
                },
                VirtualLayoutEntry {
                    logical_action: "Wipers".into(),
                    virtual_button: 4,
                },
            ]
        );
        assert_eq!(used, [1, 2, 4]);
    }

    #[test]
    fn rejects_ambiguous_virtual_bindings_from_lmu() {
        let shared_button = serde_json::json!({
            "Devices": {"vJoy-1": {}},
            "Input": {
                "Pit Limiter": {"device": "vJoy-1", "id": 32},
                "Headlights": {"device": "vJoy-1", "id": 32}
            }
        });
        assert!(Store::virtual_layout_from_document(&shared_button, "vJoy-1").is_err());

        let shared_action = serde_json::json!({
            "Devices": {"vJoy-1": {}},
            "Input": {"Pit Limiter": {"device": "vJoy-1", "id": 32}},
            "Alternative Input": {"Pit Limiter": {"device": "vJoy-1", "id": 33}}
        });
        assert!(Store::virtual_layout_from_document(&shared_action, "vJoy-1").is_err());
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
    fn assigns_an_alternate_binding_with_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(
            &preset,
            br#"{
                "Devices":{"Wheel-123":{"name":"GT Wheel"}},
                "Input":{"Shift Up":{"device":"Wheel-123","id":44}},
                "Alternative Input":{},
                "Unknown":{"preserved":true}
            }"#,
        )
        .unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();

        let backup = store
            .assign_profile_binding(&profile, "Shift Up", true, "Wheel-123", 52)
            .unwrap();
        let document = store.load_profile_document(&profile).unwrap();

        assert_eq!(document["Alternative Input"]["Shift Up"]["id"], 52);
        assert_eq!(
            document["Alternative Input"]["Shift Up"]["device"],
            "Wheel-123"
        );
        assert_eq!(document["Unknown"]["preserved"], true);
        assert!(fs::read_to_string(backup).unwrap().contains(r#""id":44"#));
    }

    #[test]
    fn creates_a_binding_for_a_new_action() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(
            &preset,
            br#"{
                "Devices":{"Wheel-123":{"name":"GT Wheel"}},
                "Input":{"Shift Up":{"device":"Wheel-123","id":44}},
                "Alternative Input":{}
            }"#,
        )
        .unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();

        store
            .assign_profile_binding(&profile, "Pit Request", false, "Wheel-123", 52)
            .unwrap();

        let document = store.load_profile_document(&profile).unwrap();
        assert_eq!(document["Input"]["Pit Request"]["id"], 52);
        assert_eq!(
            Store::binding_actions_from_document(&document),
            ["Pit Request", "Shift Up"]
        );
    }

    #[test]
    fn rejects_an_invalid_new_action_name() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(&preset, br#"{"Devices":{"Wheel-123":{}},"Input":{}}"#).unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store.import_profile(&preset).unwrap();

        assert!(
            store
                .assign_profile_binding(&profile, "  ", false, "Wheel-123", 52)
                .is_err()
        );
    }

    #[test]
    fn removes_a_device_and_its_bindings_without_touching_unknown_data() {
        let mut document = serde_json::json!({
            "Devices": {
                "Moza-R9": {"name": "MOZA R9"},
                "Pedals": {"name": "MOZA CRP Pedals"}
            },
            "Input": {
                "Steer": {"device": "Moza-R9", "id": 0},
                "Brake": {"device": "Pedals", "id": 2}
            },
            "Alternative Input": {
                "Pause": {"device": "Moza-R9", "id": 40}
            },
            "Future LMU Field": {"keep": true}
        });

        assert_eq!(Store::remove_device(&mut document, "Moza-R9").unwrap(), 2);
        assert!(document["Devices"].get("Moza-R9").is_none());
        assert!(document["Input"].get("Steer").is_none());
        assert!(document["Alternative Input"].get("Pause").is_none());
        assert_eq!(document["Input"]["Brake"]["device"], "Pedals");
        assert_eq!(document["Future LMU Field"]["keep"], true);
    }

    #[test]
    fn stores_replaces_and_removes_a_managed_profile_wheel_image() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("GT3.json");
        fs::write(&preset, br#"{"Devices":{},"Input":{}}"#).unwrap();
        let store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let mut profile = store.import_profile(&preset).unwrap();
        let png = b"\x89PNG\r\n\x1a\nsynthetic";

        store.save_profile_wheel_image(&mut profile, png).unwrap();
        assert_eq!(profile.wheel_image.as_deref(), Some("wheel-image.png"));
        assert_eq!(store.profile_wheel_image(&profile).unwrap().unwrap(), png);

        store.remove_profile_wheel_image(&mut profile).unwrap();
        assert_eq!(profile.wheel_image, None);
        assert_eq!(store.profile_wheel_image(&profile).unwrap(), None);
        assert_eq!(store.profiles().unwrap()[0].wheel_image, None);
    }

    #[test]
    fn shares_a_wheel_and_its_image_between_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lmukit");
        let mut store = Store::open_at(root.clone()).unwrap();
        let first = store
            .import_profile_bytes("GT3", br#"{"Input":{}}"#)
            .unwrap();
        let second = store
            .import_profile_bytes("GTE", br#"{"Input":{}}"#)
            .unwrap();
        let mut wheel = store
            .create_wheel(Some("Simagic".into()), "GT Neo".into())
            .unwrap();
        let png = b"\x89PNG\r\n\x1a\nsynthetic";
        store.save_wheel_image(&mut wheel, png).unwrap();
        let replacement = b"\x89PNG\r\n\x1a\nreplacement";
        store.save_wheel_image(&mut wheel, replacement).unwrap();
        store
            .assign_profile_wheel(first.id, Some(wheel.id))
            .unwrap();
        store
            .assign_profile_wheel(second.id, Some(wheel.id))
            .unwrap();

        let profiles = store.profiles().unwrap();
        assert!(
            profiles
                .iter()
                .all(|profile| profile.wheel_id == Some(wheel.id))
        );
        assert_eq!(store.wheel_image(&wheel).unwrap().unwrap(), replacement);

        drop(store);
        let reopened = Store::open_at(root).unwrap();
        assert_eq!(reopened.wheels().unwrap().len(), 1);
    }

    #[test]
    fn updates_and_only_deletes_unused_wheels() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store
            .import_profile_bytes("GT3", br#"{"Input":{}}"#)
            .unwrap();
        let wheel = store
            .create_wheel(Some("Simagic".into()), "GT Neo".into())
            .unwrap();

        store
            .update_wheel(wheel.id, Some("SIMAGIC".into()), "Neo X".into())
            .unwrap();
        assert_eq!(store.wheel(wheel.id).unwrap().name, "Neo X");
        store
            .assign_profile_wheel(profile.id, Some(wheel.id))
            .unwrap();
        assert!(store.delete_wheel(wheel.id).is_err());
        store.assign_profile_wheel(profile.id, None).unwrap();
        store.delete_wheel(wheel.id).unwrap();
        assert!(store.wheels().unwrap().is_empty());
    }

    #[test]
    fn migrates_matching_legacy_profile_wheels_to_one_library_entry() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lmukit");
        let store = Store::open_at(root.clone()).unwrap();
        for name in ["GT3", "GTE"] {
            let mut profile = store
                .import_profile_bytes(name, br#"{"Input":{}}"#)
                .unwrap();
            profile.wheel_brand = Some("Simagic".into());
            profile.wheel_name = Some("GT Neo".into());
            store.save_profile_metadata(&profile).unwrap();
        }
        drop(store);

        let migrated = Store::open_at(root).unwrap();
        let wheels = migrated.wheels().unwrap();
        assert_eq!(wheels.len(), 1);
        assert!(
            migrated
                .profiles()
                .unwrap()
                .iter()
                .all(|profile| profile.wheel_id == Some(wheels[0].id))
        );
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
        assert_eq!(store.settings.ui_font_scale, 1.0);
        assert_eq!(store.settings.log_retention_days, 5);
        assert_eq!(store.settings.input_mode, InputMode::Legacy);
        assert!(!store.settings.auto_start_virtual_profile);
        assert_eq!(store.settings.last_virtual_profile, None);
        assert!(store.settings.virtual_layout.is_empty());
        assert_eq!(store.settings.virtual_device_key, None);
        assert_eq!(store.settings.virtual_device_id, 1);
    }

    #[test]
    fn persists_virtual_layout_and_profile_button_mappings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lmukit");
        let mut store = Store::open_at(root.clone()).unwrap();
        let profile = store
            .import_profile_bytes("GT Neo GT3", br#"{"Input":{}}"#)
            .unwrap();
        store.settings.input_mode = InputMode::Virtual;
        store.settings.auto_start_virtual_profile = true;
        store.settings.last_virtual_profile = Some(profile.id);
        store.settings.virtual_layout = vec![
            VirtualLayoutEntry {
                logical_action: "pit_limiter".into(),
                virtual_button: 1,
            },
            VirtualLayoutEntry {
                logical_action: "headlights".into(),
                virtual_button: 2,
            },
        ];
        store.save_settings().unwrap();
        store
            .set_profile_virtual_mapping(
                profile.id,
                Some(VirtualProfileMapping {
                    buttons: vec![
                        VirtualButtonMapping {
                            device_id: "simagic-alpha-evo".into(),
                            device_name: "Simagic Alpha EVO".into(),
                            physical_button: 5,
                            modifiers: Vec::new(),
                            logical_action: "pit_limiter".into(),
                        },
                        VirtualButtonMapping {
                            device_id: "simagic-alpha-evo".into(),
                            device_name: "Simagic Alpha EVO".into(),
                            physical_button: 6,
                            modifiers: vec![VirtualButtonModifier {
                                device_id: "simagic-alpha-evo".into(),
                                device_name: "Simagic Alpha EVO".into(),
                                physical_button: 4,
                            }],
                            logical_action: "headlights".into(),
                        },
                    ],
                    encoder_groups: vec![VirtualEncoderGroup {
                        name: "Encoder 1".into(),
                        clockwise: VirtualEncoderControl {
                            device_id: "simagic-alpha-evo".into(),
                            device_name: "Simagic Alpha EVO".into(),
                            physical_button: 10,
                        },
                        counterclockwise: VirtualEncoderControl {
                            device_id: "simagic-alpha-evo".into(),
                            device_name: "Simagic Alpha EVO".into(),
                            physical_button: 11,
                        },
                        cycle: None,
                        modes: vec![VirtualEncoderMode {
                            name: "Race".into(),
                            clockwise_action: "pit_limiter".into(),
                            counterclockwise_action: "headlights".into(),
                        }],
                    }],
                }),
            )
            .unwrap();
        drop(store);

        let reopened = Store::open_at(root).unwrap();
        let saved_profile = reopened
            .profiles()
            .unwrap()
            .into_iter()
            .find(|saved| saved.id == profile.id)
            .unwrap();

        assert_eq!(reopened.settings.input_mode, InputMode::Virtual);
        assert!(reopened.settings.auto_start_virtual_profile);
        assert_eq!(reopened.settings.last_virtual_profile, Some(profile.id));
        assert_eq!(reopened.settings.virtual_layout.len(), 2);
        let saved_mapping = saved_profile.virtual_mapping.unwrap();
        assert_eq!(
            saved_mapping.buttons,
            vec![
                VirtualButtonMapping {
                    device_id: "simagic-alpha-evo".into(),
                    device_name: "Simagic Alpha EVO".into(),
                    physical_button: 5,
                    modifiers: Vec::new(),
                    logical_action: "pit_limiter".into(),
                },
                VirtualButtonMapping {
                    device_id: "simagic-alpha-evo".into(),
                    device_name: "Simagic Alpha EVO".into(),
                    physical_button: 6,
                    modifiers: vec![VirtualButtonModifier {
                        device_id: "simagic-alpha-evo".into(),
                        device_name: "Simagic Alpha EVO".into(),
                        physical_button: 4,
                    }],
                    logical_action: "headlights".into(),
                },
            ]
        );
        assert_eq!(saved_mapping.encoder_groups.len(), 1);
        assert_eq!(saved_mapping.encoder_groups[0].name, "Encoder 1");
        assert_eq!(saved_mapping.encoder_groups[0].modes[0].name, "Race");
    }

    #[test]
    fn imports_legacy_profile_buttons_by_logical_action() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store
            .import_profile_bytes(
                "GT Neo GT3",
                br#"{
                    "Devices": {
                        "Wheel-123": {
                            "name": "SIMAGIC GT Neo",
                            "product guid": "{05003670-0000-0000-0000-504944564944}"
                        }
                    },
                    "Input": {
                        "Pit Limiter": {"device": "Wheel-123", "id": 36},
                        "Shift Up": {"device": "Wheel-123", "id": 37},
                        "Steering": {"device": "Wheel-123", "id": 0},
                        "Unshared action": {"device": "Wheel-123", "id": 38}
                    },
                    "Alternative Input": {
                        "Headlights": {"device": "Wheel-123", "id": 37}
                    }
                }"#,
            )
            .unwrap();
        store.settings.virtual_layout = vec![
            VirtualLayoutEntry {
                logical_action: "Pit Limiter".into(),
                virtual_button: 1,
            },
            VirtualLayoutEntry {
                logical_action: "Shift Up".into(),
                virtual_button: 2,
            },
            VirtualLayoutEntry {
                logical_action: "Headlights".into(),
                virtual_button: 3,
            },
        ];
        store.save_settings().unwrap();

        let report = store.import_profile_virtual_mapping(profile.id).unwrap();

        assert_eq!(report.added, 1);
        assert_eq!(report.conflicting_controls, 1);
        assert_eq!(report.skipped_non_button, 1);
        assert_eq!(report.skipped_without_virtual_action, 1);
        assert_eq!(
            report.mapping.buttons,
            vec![VirtualButtonMapping {
                device_id: "hid:3670:0500".into(),
                device_name: "SIMAGIC GT Neo".into(),
                physical_button: 5,
                modifiers: Vec::new(),
                logical_action: "Pit Limiter".into(),
            }]
        );

        let saved = store
            .profiles()
            .unwrap()
            .into_iter()
            .find(|saved| saved.id == profile.id)
            .unwrap();
        assert_eq!(saved.virtual_mapping.unwrap(), report.mapping);
    }

    #[test]
    fn rejects_invalid_virtual_layouts_and_profile_mappings() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("lmukit")).unwrap();
        let profile = store
            .import_profile_bytes("GT Neo", br#"{"Input":{}}"#)
            .unwrap();
        store.settings.virtual_layout = vec![VirtualLayoutEntry {
            logical_action: "pit_limiter".into(),
            virtual_button: 1,
        }];
        store.save_settings().unwrap();

        let missing_action = VirtualProfileMapping {
            buttons: vec![VirtualButtonMapping {
                device_id: "simagic-alpha-evo".into(),
                device_name: "Simagic Alpha EVO".into(),
                physical_button: 5,
                modifiers: Vec::new(),
                logical_action: "headlights".into(),
            }],
            encoder_groups: Vec::new(),
        };
        assert!(
            store
                .set_profile_virtual_mapping(profile.id, Some(missing_action))
                .is_err()
        );

        store.settings.virtual_layout.push(VirtualLayoutEntry {
            logical_action: "headlights".into(),
            virtual_button: 1,
        });
        assert!(store.save_settings().is_err());
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
    fn adds_openxr_to_the_generated_lmu_launch_command() {
        let temp = tempfile::tempdir().unwrap();
        let lmukit = temp.path().join("lmukit.exe");
        fs::write(&lmukit, []).unwrap();
        let mut store = Store::open_at(temp.path().join("data")).unwrap();
        store.settings.lmu_launch_mode = LaunchMode::Vr;

        store.companion_launch_option(&lmukit).unwrap();
        let script = fs::read_to_string(store.root.join("launch-lmu-with-companions.cmd")).unwrap();
        assert!(script.ends_with("%* +XR\r\n"));
    }

    #[test]
    fn assigns_unique_profile_hotkeys() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path().join("data")).unwrap();
        let first = store
            .import_profile_bytes("GT3", br#"{"Input":{}}"#)
            .unwrap();
        let second = store
            .import_profile_bytes("GTE", br#"{"Input":{}}"#)
            .unwrap();
        store.set_profile_hotkey(first.id, Some(2)).unwrap();
        store.set_profile_hotkey(second.id, Some(2)).unwrap();

        let profiles = store.profiles().unwrap();
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.id == first.id)
                .unwrap()
                .hotkey_slot,
            None
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.id == second.id)
                .unwrap()
                .hotkey_slot,
            Some(2)
        );
        assert!(store.set_profile_hotkey(second.id, Some(10)).is_err());
    }

    #[test]
    fn renames_profiles_without_allowing_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path().join("data")).unwrap();
        let first = store
            .import_profile_bytes("Old name", br#"{"Input":{}}"#)
            .unwrap();
        store
            .import_profile_bytes("Existing", br#"{"Input":{}}"#)
            .unwrap();

        store.rename_profile(first.id, " New name ".into()).unwrap();
        assert_eq!(store.profiles().unwrap()[1].name, "New name");
        assert!(store.rename_profile(first.id, "existing".into()).is_err());
    }

    #[test]
    fn resolves_one_default_per_wheel_and_class_with_a_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("data")).unwrap();
        let race = store
            .import_profile_bytes("GT3 Race", br#"{"Input":{}}"#)
            .unwrap();
        let alternate = store
            .import_profile_bytes("GT3 Alternate", br#"{"Input":{}}"#)
            .unwrap();
        let generic = store
            .import_profile_bytes("Generic", br#"{"Input":{}}"#)
            .unwrap();
        let wheel = store
            .create_wheel(Some("Simagic".into()), "GT Neo".into())
            .unwrap();
        for profile in [&race, &alternate, &generic] {
            store
                .assign_profile_wheel(profile.id, Some(wheel.id))
                .unwrap();
        }
        for profile in [&race, &alternate] {
            store
                .set_profile_categories(profile.id, vec!["GT3".into()], Vec::new())
                .unwrap();
        }

        store
            .set_virtual_profile_default(race.id, Some("gt3".into()))
            .unwrap();
        store.set_virtual_profile_default(generic.id, None).unwrap();
        store
            .set_virtual_profile_default(alternate.id, Some("GT3".into()))
            .unwrap();

        assert_eq!(store.settings.virtual_profile_defaults.len(), 2);
        let (resolved, exact) = store
            .resolve_virtual_profile(wheel.id, Some("gt3"))
            .unwrap()
            .unwrap();
        assert_eq!(resolved.id, alternate.id);
        assert!(exact);
        let (resolved, exact) = store
            .resolve_virtual_profile(wheel.id, Some("HY"))
            .unwrap()
            .unwrap();
        assert_eq!(resolved.id, generic.id);
        assert!(!exact);
    }

    #[test]
    fn removes_defaults_when_profile_identity_changes_or_is_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(temp.path().join("data")).unwrap();
        let profile = store
            .import_profile_bytes("GT3", br#"{"Input":{}}"#)
            .unwrap();
        let wheel = store.create_wheel(None, "GT Neo".into()).unwrap();
        store
            .assign_profile_wheel(profile.id, Some(wheel.id))
            .unwrap();
        store
            .set_profile_categories(profile.id, vec!["GT3".into()], Vec::new())
            .unwrap();
        store
            .set_virtual_profile_default(profile.id, Some("GT3".into()))
            .unwrap();

        store
            .set_profile_categories(profile.id, vec!["HY".into()], Vec::new())
            .unwrap();
        assert!(store.settings.virtual_profile_defaults.is_empty());

        store.set_virtual_profile_default(profile.id, None).unwrap();
        store.settings.last_virtual_profile = Some(profile.id);
        store.save_settings().unwrap();
        let saved = store
            .profiles()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == profile.id)
            .unwrap();
        store.delete(&saved).unwrap();
        assert!(store.settings.virtual_profile_defaults.is_empty());
        assert_eq!(store.settings.last_virtual_profile, None);
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

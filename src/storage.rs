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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub lmu_config_path: PathBuf,
    #[serde(default)]
    pub active_profile: Option<Uuid>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lmu_config_path: default_lmu_path(),
            active_profile: None,
        }
    }
}

pub struct Store {
    root: PathBuf,
    pub settings: Settings,
}

impl Store {
    pub fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("app", "Willa", "Willa")
            .context("could not determine Willa's application-data directory")?;
        Self::open_at(dirs.data_local_dir().to_path_buf())
    }

    pub fn open_at(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("backups"))?;
        let settings_path = root.join("settings.json");
        let settings = if settings_path.exists() {
            serde_json::from_slice(&fs::read(&settings_path)?)
                .context("Willa's settings file is invalid")?
        } else {
            Settings::default()
        };
        Ok(Self { root, settings })
    }

    pub fn save_settings(&self) -> Result<()> {
        write_json(&self.root.join("settings.json"), &self.settings)
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

    pub fn capture(&self, name: &str) -> Result<Profile> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Enter a profile name.");
        }
        let source = &self.settings.lmu_config_path;
        validate_lmu_file(source)?;

        let profile = Profile {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: unix_time()?,
            assignments: Vec::new(),
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

        let staged = parent.join(".willa-direct-input.tmp");
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

pub fn default_lmu_path() -> PathBuf {
    PathBuf::from(
        r"C:\Program Files (x86)\Steam\steamapps\common\Le Mans Ultimate\UserData\player\direct input.json",
    )
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
    fn captures_and_restores_a_profile() {
        let temp = tempfile::tempdir().unwrap();
        let live = temp.path().join("game").join(CONFIG_FILE_NAME);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, br#"{"wheel":"neo"}"#).unwrap();

        let mut store = Store::open_at(temp.path().join("willa")).unwrap();
        store.settings.lmu_config_path = live.clone();
        let profile = store.capture("GT Neo").unwrap();
        fs::write(&live, br#"{"wheel":"other"}"#).unwrap();

        let backup = store.activate(&profile).unwrap();
        assert_eq!(fs::read_to_string(live).unwrap(), r#"{"wheel":"neo"}"#);
        assert_eq!(fs::read_to_string(backup).unwrap(), r#"{"wheel":"other"}"#);
    }
}

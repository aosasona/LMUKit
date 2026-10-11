const LMU_PROCESS_NAMES: &[&str] = &["lemansultimate", "lmu"];

fn is_lmu_process_name(name: &str) -> bool {
    let lowercase = name.trim().to_ascii_lowercase();
    let normalized = lowercase
        .strip_suffix(".exe")
        .unwrap_or(&lowercase)
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    LMU_PROCESS_NAMES.contains(&normalized.as_str())
}

#[cfg(target_os = "windows")]
pub fn running_process() -> Result<Option<String>, String> {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, WIN32_ERROR},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };

    struct Snapshot(HANDLE);

    impl Drop for Snapshot {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    let snapshot = Snapshot(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|error| format!("Could not enumerate Windows processes: {error}"))?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }
        .map_err(|error| format!("Could not read the Windows process list: {error}"))?;
    loop {
        let length = entry
            .szExeFile
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..length]);
        if is_lmu_process_name(&name) {
            return Ok(Some(name));
        }
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error)
                if WIN32_ERROR::from_error(&error)
                    .is_some_and(|code| code == ERROR_NO_MORE_FILES) =>
            {
                break;
            }
            Err(error) => {
                return Err(format!(
                    "Could not finish reading the Windows process list: {error}"
                ));
            }
        }
    }
    Ok(None)
}

#[cfg(not(target_os = "windows"))]
pub fn running_process() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::is_lmu_process_name;

    #[test]
    fn recognizes_known_lmu_executable_spellings() {
        assert!(is_lmu_process_name("Le Mans Ultimate.exe"));
        assert!(is_lmu_process_name("LeMansUltimate.EXE"));
        assert!(is_lmu_process_name("LMU.exe"));
    }

    #[test]
    fn rejects_similar_unrelated_processes() {
        assert!(!is_lmu_process_name("LMUKit.exe"));
        assert!(!is_lmu_process_name("Le Mans Ultimate Launcher.exe"));
        assert!(!is_lmu_process_name("steam.exe"));
    }
}

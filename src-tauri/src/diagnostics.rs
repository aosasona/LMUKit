use serde::Serialize;
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_SIZE: u64 = 1_048_576;
const RETAINED_LOGS: usize = 4;
const RETAINED_ERRORS: usize = 20;

static LOGGER: OnceLock<Logger> = OnceLock::new();

struct LogWriter {
    file: Option<File>,
    size: u64,
}

struct Logger {
    directory: PathBuf,
    writer: Mutex<LogWriter>,
    recent_errors: Mutex<VecDeque<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub directory: String,
    pub current_size: u64,
    pub retained_files: usize,
    pub recent_errors: Vec<String>,
}

pub fn initialise(data_directory: &Path) -> std::io::Result<()> {
    let directory = data_directory.join("logs");
    fs::create_dir_all(&directory)?;
    rotate_if_oversized(&directory)?;
    let (file, size) = open_current(&directory)?;
    let _ = LOGGER.set(Logger {
        directory,
        writer: Mutex::new(LogWriter {
            file: Some(file),
            size,
        }),
        recent_errors: Mutex::new(VecDeque::new()),
    });
    info(
        "app.start",
        format!("LMUKit {} started", env!("CARGO_PKG_VERSION")),
    );
    Ok(())
}

pub fn info(event: &str, message: impl AsRef<str>) {
    write("INFO", event, message.as_ref());
}

pub fn warn(event: &str, message: impl AsRef<str>) {
    remember_error(event, message.as_ref());
    write("WARN", event, message.as_ref());
}

pub fn error(event: &str, message: impl AsRef<str>) {
    remember_error(event, message.as_ref());
    write("ERROR", event, message.as_ref());
}

pub fn directory() -> Option<PathBuf> {
    LOGGER.get().map(|logger| logger.directory.clone())
}

pub fn summary() -> Option<LogSummary> {
    let logger = LOGGER.get()?;
    let size = logger.writer.lock().ok().map(|writer| writer.size)?;
    let recent_errors = logger.recent_errors.lock().ok()?.iter().cloned().collect();
    let retained_files = (0..=RETAINED_LOGS)
        .filter(|index| log_path(&logger.directory, *index).is_file())
        .count();
    Some(LogSummary {
        directory: logger.directory.display().to_string(),
        current_size: size,
        retained_files,
        recent_errors,
    })
}

fn remember_error(event: &str, message: &str) {
    let Some(logger) = LOGGER.get() else { return };
    let Ok(mut errors) = logger.recent_errors.lock() else {
        return;
    };
    if errors.len() == RETAINED_ERRORS {
        errors.pop_front();
    }
    errors.push_back(format!("{event}: {}", clean(message)));
}

fn write(level: &str, event: &str, message: &str) {
    let Some(logger) = LOGGER.get() else { return };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let line = format!(
        "[{}.{:03}] {level:<5} {} {}\n",
        timestamp.as_secs(),
        timestamp.subsec_millis(),
        clean(event),
        clean(message)
    );
    let Ok(mut writer) = logger.writer.lock() else {
        return;
    };
    if writer.size + line.len() as u64 > MAX_LOG_SIZE {
        writer.file.take();
        let _ = rotate(&logger.directory);
        if let Ok((file, size)) = open_current(&logger.directory) {
            writer.file = Some(file);
            writer.size = size;
        }
    }
    let Some(file) = writer.file.as_mut() else {
        return;
    };
    if file.write_all(line.as_bytes()).is_ok() {
        let _ = file.flush();
        writer.size += line.len() as u64;
    }
}

fn clean(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn rotate_if_oversized(directory: &Path) -> std::io::Result<()> {
    let current = log_path(directory, 0);
    if current
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        >= MAX_LOG_SIZE
    {
        rotate(directory)?;
    }
    Ok(())
}

fn rotate(directory: &Path) -> std::io::Result<()> {
    for index in (1..=RETAINED_LOGS).rev() {
        let destination = log_path(directory, index);
        if destination.exists() {
            fs::remove_file(&destination)?;
        }
        let source = log_path(directory, index - 1);
        if source.exists() {
            fs::rename(source, destination)?;
        }
    }
    Ok(())
}

fn open_current(directory: &Path) -> std::io::Result<(File, u64)> {
    let path = log_path(directory, 0);
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let size = file.metadata()?.len();
    Ok((file, size))
}

fn log_path(directory: &Path, index: usize) -> PathBuf {
    if index == 0 {
        directory.join("lmukit.log")
    } else {
        directory.join(format!("lmukit.log.{index}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{clean, log_path, rotate};
    use std::fs;

    #[test]
    fn sanitises_log_lines() {
        assert_eq!(clean("first\r\nsecond"), "first  second");
    }

    #[test]
    fn rotates_current_and_retained_logs() {
        let directory =
            std::env::temp_dir().join(format!("lmukit-log-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(log_path(&directory, 0), "current").unwrap();
        fs::write(log_path(&directory, 1), "previous").unwrap();

        rotate(&directory).unwrap();

        assert_eq!(
            fs::read_to_string(log_path(&directory, 1)).unwrap(),
            "current"
        );
        assert_eq!(
            fs::read_to_string(log_path(&directory, 2)).unwrap(),
            "previous"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

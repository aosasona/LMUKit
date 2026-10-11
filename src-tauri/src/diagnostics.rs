use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const RETAINED_ERRORS: usize = 20;
const MIN_RETENTION_DAYS: usize = 1;
const MAX_RETENTION_DAYS: usize = 30;

static LOGGER: OnceLock<Logger> = OnceLock::new();

struct LogWriter {
    file: Option<File>,
    date: String,
    size: u64,
}

struct Logger {
    directory: PathBuf,
    writer: Mutex<LogWriter>,
    retention_days: AtomicUsize,
    recent_errors: Mutex<VecDeque<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub directory: String,
    pub current_size: u64,
    pub retained_files: usize,
    pub retention_days: usize,
    pub recent_errors: Vec<String>,
}

pub fn initialise(data_directory: &Path, retention_days: usize) -> std::io::Result<()> {
    let directory = data_directory.join("logs");
    fs::create_dir_all(&directory)?;
    let retention_days = normalize_retention(retention_days);
    let (date, today) = current_log_date();
    let _ = cleanup_old_logs(&directory, today, retention_days);
    let _ = cleanup_legacy_logs(&directory);
    let (file, size) = open_for_date(&directory, &date)?;
    let _ = LOGGER.set(Logger {
        directory,
        writer: Mutex::new(LogWriter {
            file: Some(file),
            date,
            size,
        }),
        retention_days: AtomicUsize::new(retention_days),
        recent_errors: Mutex::new(VecDeque::new()),
    });
    info(
        "app.start",
        format!("LMUKit {} started", env!("CARGO_PKG_VERSION")),
    );
    Ok(())
}

pub fn set_retention_days(retention_days: usize) -> std::io::Result<()> {
    let Some(logger) = LOGGER.get() else {
        return Ok(());
    };
    let retention_days = normalize_retention(retention_days);
    logger
        .retention_days
        .store(retention_days, Ordering::Relaxed);
    let (_, today) = current_log_date();
    cleanup_old_logs(&logger.directory, today, retention_days)
}

pub fn info(event: &str, message: impl AsRef<str>) {
    write("INFO", event, message.as_ref());
}

pub fn warn(event: &str, message: impl AsRef<str>) {
    remember_error(event);
    write("WARN", event, message.as_ref());
}

pub fn error(event: &str, message: impl AsRef<str>) {
    remember_error(event);
    write("ERROR", event, message.as_ref());
}

pub fn directory() -> Option<PathBuf> {
    LOGGER.get().map(|logger| logger.directory.clone())
}

pub fn summary() -> Option<LogSummary> {
    let logger = LOGGER.get()?;
    let size = logger.writer.lock().ok().map(|writer| writer.size)?;
    let recent_errors = logger.recent_errors.lock().ok()?.iter().cloned().collect();
    let retained_files = log_file_count(&logger.directory).unwrap_or_default();
    Some(LogSummary {
        directory: logger.directory.display().to_string(),
        current_size: size,
        retained_files,
        retention_days: logger.retention_days.load(Ordering::Relaxed),
        recent_errors,
    })
}

fn remember_error(event: &str) {
    let Some(logger) = LOGGER.get() else { return };
    let Ok(mut errors) = logger.recent_errors.lock() else {
        return;
    };
    if errors.len() == RETAINED_ERRORS {
        errors.pop_front();
    }
    errors.push_back(format!("{} · See the local log for details", clean(event)));
}

fn write(level: &str, event: &str, message: &str) {
    let Some(logger) = LOGGER.get() else { return };
    let line = format!(
        "[{}] {level:<5} {} {}\n",
        utc_timestamp(SystemTime::now()),
        clean(event),
        clean(message)
    );
    let Ok(mut writer) = logger.writer.lock() else {
        return;
    };
    let (date, today) = current_log_date();
    if writer.date != date {
        writer.file.take();
        let retention_days = logger.retention_days.load(Ordering::Relaxed);
        let _ = cleanup_old_logs(&logger.directory, today, retention_days);
        match open_for_date(&logger.directory, &date) {
            Ok((file, size)) => {
                writer.file = Some(file);
                writer.date = date;
                writer.size = size;
            }
            Err(_) => return,
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

fn utc_timestamp(time: SystemTime) -> String {
    let elapsed = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = elapsed.as_secs();
    let seconds_today = seconds % 86_400;
    let hour = seconds_today / 3_600;
    let minute = seconds_today % 3_600 / 60;
    let second = seconds_today % 60;
    let date = date_from_days((seconds / 86_400) as i64);
    format!(
        "{date}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        elapsed.subsec_millis()
    )
}

fn normalize_retention(retention_days: usize) -> usize {
    retention_days.clamp(MIN_RETENTION_DAYS, MAX_RETENTION_DAYS)
}

fn cleanup_old_logs(directory: &Path, today: i64, retention_days: usize) -> std::io::Result<()> {
    let retained: HashSet<_> = (0..normalize_retention(retention_days))
        .map(|offset| date_from_days(today - offset as i64))
        .collect();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(date) = log_date_from_filename(&name)
            && !retained.contains(date)
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn cleanup_legacy_logs(directory: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "lmukit.log" || name.starts_with("lmukit.log.") {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn log_file_count(directory: &Path) -> std::io::Result<usize> {
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| log_date_from_filename(&entry.file_name().to_string_lossy()).is_some())
        .count())
}

fn open_for_date(directory: &Path, date: &str) -> std::io::Result<(File, u64)> {
    let path = directory.join(format!("lmukit-{date}.log"));
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let size = file.metadata()?.len();
    Ok((file, size))
}

fn log_date_from_filename(name: &str) -> Option<&str> {
    let date = name.strip_prefix("lmukit-")?.strip_suffix(".log")?;
    (date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit()))
    .then_some(date)
}

#[cfg(target_os = "windows")]
fn current_log_date() -> (String, i64) {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let time = unsafe { GetLocalTime() };
    let day = days_from_civil(time.wYear as i32, time.wMonth as u32, time.wDay as u32);
    (date_from_days(day), day)
}

#[cfg(not(target_os = "windows"))]
fn current_log_date() -> (String, i64) {
    let day = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86_400;
    (date_from_days(day), day)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let adjusted_month = month as i32 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era - 719_468) as i64
}

fn date_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::{
        clean, cleanup_old_logs, date_from_days, days_from_civil, log_date_from_filename,
        log_file_count, utc_timestamp,
    };
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn sanitises_log_lines() {
        assert_eq!(clean("first\r\nsecond"), "first  second");
    }

    #[test]
    fn formats_calendar_dates() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(days_from_civil(2026, 10, 11)), "2026-10-11");
    }

    #[test]
    fn formats_readable_utc_timestamps() {
        let time = UNIX_EPOCH + Duration::new(90_061, 123_000_000);
        assert_eq!(utc_timestamp(time), "1970-01-02T01:01:01.123Z");
    }

    #[test]
    fn recognises_only_daily_log_names() {
        assert_eq!(
            log_date_from_filename("lmukit-2026-10-11.log"),
            Some("2026-10-11")
        );
        assert_eq!(log_date_from_filename("lmukit.log"), None);
        assert_eq!(log_date_from_filename("diagnostic-report-1.json"), None);
    }

    #[test]
    fn removes_logs_outside_the_retention_window() {
        let directory =
            std::env::temp_dir().join(format!("lmukit-log-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        for date in [
            "2026-10-07",
            "2026-10-08",
            "2026-10-09",
            "2026-10-10",
            "2026-10-11",
        ] {
            fs::write(directory.join(format!("lmukit-{date}.log")), date).unwrap();
        }
        fs::write(directory.join("diagnostic-report-1.json"), "report").unwrap();

        cleanup_old_logs(&directory, days_from_civil(2026, 10, 11), 3).unwrap();

        assert_eq!(log_file_count(&directory).unwrap(), 3);
        assert!(!directory.join("lmukit-2026-10-08.log").exists());
        assert!(directory.join("lmukit-2026-10-09.log").exists());
        assert!(directory.join("diagnostic-report-1.json").exists());
        fs::remove_dir_all(directory).unwrap();
    }
}

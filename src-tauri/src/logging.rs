use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use log::{LevelFilter, Log, Metadata, Record};

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BACKUPS: usize = 3;

struct FileLogger {
    file: Mutex<File>,
    level: LevelFilter,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let line = format!(
            "{timestamp} {:<5} [{}] {}\n",
            record.level(),
            record.target(),
            record.args()
        );
        match self.file.lock() {
            Ok(mut file) => {
                if let Err(error) = file.write_all(line.as_bytes()) {
                    eprintln!("DisplayPilot log write failed: {error}");
                }
            }
            Err(_) => eprintln!("DisplayPilot log mutex poisoned"),
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Installs the process-wide logger. Safe to call more than once; later calls are ignored.
pub fn init(log_dir: &Path, log_file: &Path) -> Result<(), String> {
    fs::create_dir_all(log_dir)
        .map_err(|error| format!("无法创建日志目录 {}：{error}", log_dir.display()))?;
    rotate(log_file);

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .map_err(|error| format!("无法打开日志文件 {}：{error}", log_file.display()))?;

    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    let logger = FileLogger {
        file: Mutex::new(file),
        level,
    };

    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
        install_panic_hook();
    }

    log::info!(
        "DisplayPilot {} starting ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Ok(())
}

fn rotate(log_file: &Path) {
    let Ok(metadata) = fs::metadata(log_file) else {
        return;
    };
    if metadata.len() < MAX_LOG_BYTES {
        return;
    }

    let directory = match log_file.parent() {
        Some(directory) => directory,
        None => return,
    };
    let name = match log_file.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => return,
    };

    for index in (1..=MAX_BACKUPS).rev() {
        let source = directory.join(format!("{name}.{index}"));
        if index == MAX_BACKUPS {
            let _ = fs::remove_file(&source);
            continue;
        }
        let target = directory.join(format!("{name}.{}", index + 1));
        let _ = fs::remove_file(&target);
        let _ = fs::rename(&source, &target);
    }
    let _ = fs::rename(log_file, directory.join(format!("{name}.1")));
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        log::logger().flush();
        previous(info);
    }));
}

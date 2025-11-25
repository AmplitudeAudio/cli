use chrono::{DateTime, Local};
use colored::*;
use log::{Level, Log, Metadata, Record};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone)]
pub enum LogLevel {
    Standard(Level),
    Success,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Standard(level) => write!(f, "{}", level),
            LogLevel::Success => write!(f, "SUCCESS"),
        }
    }
}

const MAX_LOG_BUFFER_SIZE: usize = 1000;

pub struct LogEntry {
    timestamp: DateTime<Local>,
    level: LogLevel,
    target: String,
    message: String,
}

impl LogEntry {
    pub fn new(record: &Record) -> Self {
        Self {
            timestamp: Local::now(),
            level: LogLevel::Standard(record.level()),
            target: record.target().to_string(),
            message: record.args().to_string(),
        }
    }

    pub fn new_success(target: String, message: String) -> Self {
        Self {
            timestamp: Local::now(),
            level: LogLevel::Success,
            target,
            message,
        }
    }

    pub fn format_for_file(&self) -> String {
        format!(
            "[{}] [{}] [{}] {}\n",
            self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            self.level,
            self.target,
            self.message
        )
    }
}

static LOG_BUFFER: Mutex<Option<VecDeque<LogEntry>>> = Mutex::new(None);
static VERBOSE_MODE: RwLock<bool> = RwLock::new(false);

pub struct Logger;

impl Logger {
    pub fn new() -> Self {
        Self
    }

    pub fn set_verbose(verbose: bool) {
        if let Ok(mut v) = VERBOSE_MODE.write() {
            *v = verbose;
        }
    }

    pub fn is_verbose() -> bool {
        VERBOSE_MODE.read().map(|v| *v).unwrap_or(false)
    }

    fn add_to_buffer(entry: LogEntry) {
        if let Ok(mut buffer_opt) = LOG_BUFFER.lock() {
            if buffer_opt.is_none() {
                *buffer_opt = Some(VecDeque::with_capacity(MAX_LOG_BUFFER_SIZE));
            }

            if let Some(buffer) = buffer_opt.as_mut() {
                if buffer.len() >= MAX_LOG_BUFFER_SIZE {
                    buffer.pop_front();
                }
                buffer.push_back(entry);
            }
        }
    }

    fn format_console_message(record: &Record) -> String {
        let message = record.args().to_string();

        match record.level() {
            Level::Debug => format!("{} {}", "*".black(), message.black()),
            Level::Trace => format!("{} {}", "-".black(), message),
            Level::Info => format!("{} {}", "+".blue(), message),
            Level::Warn => format!("{} {}", "!".red(), message),
            Level::Error => format!("{} {}", "#".red(), message.red()),
        }
    }

    fn format_success_message(message: &str) -> String {
        format!("{} {}", "✓".green(), message.green())
    }

    fn should_display(level: Level) -> bool {
        match level {
            Level::Debug | Level::Trace => Self::is_verbose(),
            Level::Info | Level::Warn | Level::Error => true,
        }
    }

    pub fn log_success(target: &str, message: &str) {
        let entry = LogEntry::new_success(target.to_string(), message.to_string());

        // Add to buffer for crash logging
        Self::add_to_buffer(entry);

        // Always display SUCCESS messages
        let formatted_message = Self::format_success_message(message);
        println!("{}", formatted_message);
    }

    pub fn write_crash_log() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
        let amplitude_dir = home_dir.join(".amplitude");

        // Create .amplitude directory if it doesn't exist
        fs::create_dir_all(&amplitude_dir)?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S%.3f");
        let log_file_path = amplitude_dir.join(format!("{}.log", timestamp));

        let mut file = fs::File::create(&log_file_path)?;

        // Write crash header
        writeln!(file, "=== AMPLITUDE CLI CRASH LOG ===")?;
        writeln!(
            file,
            "Crash time: {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        )?;
        writeln!(file, "================================\n")?;

        if let Ok(buffer_opt) = LOG_BUFFER.lock() {
            if let Some(buffer) = buffer_opt.as_ref() {
                for entry in buffer.iter() {
                    file.write_all(entry.format_for_file().as_bytes())?;
                }
            }
        }

        file.flush()?;
        Ok(log_file_path)
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let entry = LogEntry::new(record);

            // Always add to buffer for crash logging
            Self::add_to_buffer(entry);

            // Display to console based on level and verbose mode
            if Self::should_display(record.level()) {
                let formatted_message = Self::format_console_message(record);
                println!("{}", formatted_message);
            }
        }
    }

    fn flush(&self) {
        // Nothing to flush for console output
    }
}

pub fn init_logger(verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    Logger::set_verbose(verbose);

    let logger = Logger::new();
    log::set_boxed_logger(Box::new(logger)).map_err(|e| format!("Failed to set logger: {}", e))?;
    log::set_max_level(log::LevelFilter::Trace);

    Ok(())
}

pub fn setup_crash_logging() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Log the panic to our buffer
        log::error!("PANIC: {}", panic_info);

        match Logger::write_crash_log() {
            Ok(log_path) => {
                eprintln!("Crash log written to: {}", log_path.display());
            }
            Err(e) => {
                eprintln!("Failed to write crash log: {}", e);
            }
        }

        default_hook(panic_info);
    }));
}

pub fn write_crash_log_on_error() -> Option<PathBuf> {
    Logger::write_crash_log().ok()
}

/// Macro for logging SUCCESS messages with green checkmark
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {
        $crate::common::logger::Logger::log_success(
            module_path!(),
            &format!($($arg)*)
        )
    };
}

// A static logger used throughout the application
// - Supports log levels: error, warn, info, debug (ascending verbosity)
// - Supports logging to stderr and optionally also a log file

use std::sync::OnceLock;

/// A configuration for the static logger
/// See `init` and `log` to use the logger
#[derive(Debug)]
pub struct LoggerConfig {
    /// Log file
    pub file: Option<std::fs::File>,

    /// Specified verbosity
    pub minimum_level: LogLevel,
}

/// The global logger instance
/// See `init` and `log` to use the logger
pub static LOGGER: OnceLock<LoggerConfig> = OnceLock::new();

/// Log levels that specify the severity of messages
/// Levels are ordered from least to most severe as:
/// `Debug < Info < Warn < Error`
///
/// Verbosity works by setting a minimum severity log-level.
/// Messages with a level less than the minimum level are ignored.
/// For example, setting the minimum level to `Debug` logs **all** messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    #[default]
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl std::fmt::Display for LogLevel {
    /// Format the log level as a string
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Error => "Error",
            LogLevel::Warn => "Warn",
            LogLevel::Info => "Info",
            LogLevel::Debug => "Debug",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    /// Parse a log level from a string
    /// Accepted values are all defined enum variants (case insensitive)
    /// or their associated integer values as strings.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // try parsing as number first
        if let Ok(n) = s.parse::<u8>() {
            return match n {
                0 => Ok(Self::Debug),
                1 => Ok(Self::Info),
                2 => Ok(Self::Warn),
                3 => Ok(Self::Error),
                _ => Err(format!("Invalid log level '{s}'")),
            };
        }

        // fall back to string matching
        match s.to_lowercase().as_str() {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(format!("Invalid log level '{s}'")),
        }
    }
}

/// Initialize the global logger once
/// Returns: `Err(Logger)` if already initialized, otherwise `Ok(())`
pub fn init<F>(file: F, minimum_level: LogLevel) -> Result<(), LoggerConfig>
where
    F: Into<Option<std::fs::File>>,
{
    LOGGER.set(LoggerConfig {
        file: file.into(),
        minimum_level,
    })
}

/// Log a message
/// Note that the Logger must first be initialized via `init`
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        for _ in 0..1 { // trick to allow early exit via break
            let logger = $crate::logger::LOGGER
                .get()
                .expect("Fatal: Logger used while uninitialized");

            // filter by minimum level
            if $level < logger.minimum_level {
                break;
            }

            let message = format!($($arg)*);
            let datetime = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

            let prefix = match $level {
                $crate::logger::LogLevel::Debug => "[debug]",
                $crate::logger::LogLevel::Info => "[info] ",
                $crate::logger::LogLevel::Warn => "[warn] ",
                $crate::logger::LogLevel::Error => "[error]",
            };

            // format only once
            let msg = format!("[{datetime}] {prefix}  {message}");
            eprintln!("{msg}");

            if let Some(file) = &logger.file {
                use std::io::Write;
                let mut file = file.try_clone().expect("Failed to clone log file handle");
                writeln!(file, "{msg}").expect("Failed to write to log file");
            }
        }
    };
}

/// Shorthand for logging a debug message using `log`
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        log!($crate::logger::LogLevel::Debug, $($arg)*);
    };
}

/// Shorthand for logging an info message using `log`
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        log!($crate::logger::LogLevel::Info, $($arg)*);
    };
}

/// Shorthand for logging a warning message using `log`
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        log!($crate::logger::LogLevel::Warn, $($arg)*);
    };
}

/// Shorthand for logging an error message using `log`
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        log!($crate::logger::LogLevel::Error, $($arg)*);
    };
}

// TODO: logger color support

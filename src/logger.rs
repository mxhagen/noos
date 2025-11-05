// A static logger used throughout the application
// - Supports log levels: error, warn, info, debug (ascending verbosity)
// - Supports logging to stderr and optionally also a log file

use std::sync::{LazyLock, OnceLock};

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

/// A macro helper to generate color functions
macro_rules! color_fn {
    ($name:ident, $code:expr) => {
        #[doc = concat!("Wrap text in ", stringify!($name), " ANSI color codes")]
        pub fn $name(text: &str) -> String {
            format!("{}{}\x1b[0m", $code, text)
        }
    };
}

color_fn!(red, "\x1b[31m");
color_fn!(yellow, "\x1b[33m");
color_fn!(blue, "\x1b[34m");
color_fn!(magenta, "\x1b[35m");
color_fn!(lightgray, "\x1b[37m");

/// A global flag indicating whether to colorize output
pub static COLORIZE: LazyLock<bool> = LazyLock::new(|| {
    use std::io::IsTerminal;
    let colorize = std::env::var_os("NO_COLOR").is_none() // NO_COLOR disables all colors
        && std::io::stderr().is_terminal(); // only color terminal output

    use std::env::var_os;
    match (var_os("CLICOLOR_FORCE"), var_os("CLICOLOR"), var_os("TERM")) {
        (Some(force), _, _) => force != "0", // CLICOLOR_FORCE overrides all
        (_, Some(color), _) => color != "0", // CLICOLOR enables/disables colors
        (_, _, Some(term)) => term != "dumb", // check TERM last
        _ => colorize,
    }
});

/// Log a message
/// Note that the Logger must first be initialized via `init`
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        for _ in 0..1 { // trick to allow early exit via break
            use $crate::logger::*;

            let logger = LOGGER.get()
                .expect("Fatal: Logger used while uninitialized");

            // filter by minimum level
            if $level < logger.minimum_level {
                break;
            }

            let message = format!($($arg)*);
            let datetime = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]").to_string();

            let prefix = match $level {
                LogLevel::Debug => "[debug]",
                LogLevel::Info => "[info] ",
                LogLevel::Warn => "[warn] ",
                LogLevel::Error => "[error]",
            };

            let msg = format!("{datetime} {prefix}  {message}");

            // write to stderr (colorized if supported)
            if *COLORIZE {
                let prefix = match $level {
                    LogLevel::Debug => magenta(prefix),
                    LogLevel::Info => blue(prefix),
                    LogLevel::Warn => yellow(prefix),
                    LogLevel::Error => red(prefix),
                };
                let datetime = lightgray(&datetime);
                let msg_colorized = format!("{datetime} {prefix}  {message}");
                eprintln!("{msg_colorized}");
            } else {
                eprintln!("{msg}");
            }

            // write uncolorized to file
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

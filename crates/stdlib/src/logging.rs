//! Structured logging module for Roast.
//!
//! Provides Python-like logging with levels, handlers, and formatters.
//!
//! # Example
//! ```roast
//! from logging import Logger, Level
//!
//! log = Logger.new("my_app")
//! log.info("Application started")
//! log.warning("Deprecated feature used")
//! log.error("Connection failed")
//! ```

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// Log Level
// =============================================================================

/// Log severity levels (compatible with Python's logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Level {
    /// Detailed debug information.
    Debug = 10,
    /// Informational messages.
    Info = 20,
    /// Warning messages.
    Warning = 30,
    /// Error messages.
    Error = 40,
    /// Critical errors.
    Critical = 50,
}

impl Level {
    /// Get level name.
    pub fn name(&self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
            Level::Critical => "CRITICAL",
        }
    }

    /// Parse level from name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_uppercase().as_str() {
            "DEBUG" => Some(Level::Debug),
            "INFO" => Some(Level::Info),
            "WARNING" | "WARN" => Some(Level::Warning),
            "ERROR" => Some(Level::Error),
            "CRITICAL" | "FATAL" => Some(Level::Critical),
            _ => None,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

// =============================================================================
// Log Record
// =============================================================================

/// A single log record.
#[derive(Debug, Clone)]
pub struct Record {
    /// Log level.
    pub level: Level,
    /// Log message.
    pub message: String,
    /// Logger name.
    pub logger_name: String,
    /// Timestamp (Unix epoch seconds).
    pub timestamp: u64,
    /// Source file (optional).
    pub file: Option<String>,
    /// Line number (optional).
    pub line: Option<u32>,
    /// Extra context.
    pub extra: HashMap<String, String>,
}

impl Record {
    /// Create a new record.
    pub fn new(level: Level, message: impl Into<String>, logger_name: impl Into<String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            level,
            message: message.into(),
            logger_name: logger_name.into(),
            timestamp,
            file: None,
            line: None,
            extra: HashMap::new(),
        }
    }

    /// Add source location.
    pub fn with_location(mut self, file: &str, line: u32) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(line);
        self
    }

    /// Add extra context.
    pub fn with_extra(mut self, key: &str, value: &str) -> Self {
        self.extra.insert(key.to_string(), value.to_string());
        self
    }
}

// =============================================================================
// Formatter
// =============================================================================

/// Log record formatter.
pub trait Formatter: Send + Sync {
    /// Format a record to string.
    fn format(&self, record: &Record) -> String;
}

/// Default formatter: "LEVEL - NAME: message"
pub struct DefaultFormatter;

impl Formatter for DefaultFormatter {
    fn format(&self, record: &Record) -> String {
        format!("{} - {}: {}", record.level, record.logger_name, record.message)
    }
}

/// Detailed formatter with timestamp.
pub struct DetailedFormatter {
    /// Include timestamp.
    pub show_time: bool,
    /// Include source location.
    pub show_location: bool,
}

impl Default for DetailedFormatter {
    fn default() -> Self {
        Self {
            show_time: true,
            show_location: true,
        }
    }
}

impl Formatter for DetailedFormatter {
    fn format(&self, record: &Record) -> String {
        let mut parts = Vec::new();

        if self.show_time {
            parts.push(format!("[{}]", record.timestamp));
        }

        parts.push(format!("{:8}", record.level.name()));
        parts.push(format!("{}", record.logger_name));

        if self.show_location {
            if let (Some(file), Some(line)) = (&record.file, record.line) {
                parts.push(format!("({}:{})", file, line));
            }
        }

        parts.push(record.message.clone());

        parts.join(" ")
    }
}

/// JSON formatter for structured logs.
pub struct JsonFormatter;

impl Formatter for JsonFormatter {
    fn format(&self, record: &Record) -> String {
        let mut obj = String::from("{");
        obj.push_str(&format!("\"level\":\"{}\",", record.level.name()));
        obj.push_str(&format!("\"logger\":\"{}\",", record.logger_name));
        obj.push_str(&format!("\"message\":\"{}\",", escape_json(&record.message)));
        obj.push_str(&format!("\"timestamp\":{}", record.timestamp));

        if let Some(ref file) = record.file {
            obj.push_str(&format!(",\"file\":\"{}\"", file));
        }
        if let Some(line) = record.line {
            obj.push_str(&format!(",\"line\":{}", line));
        }
        if !record.extra.is_empty() {
            obj.push_str(",\"extra\":{");
            let extras: Vec<_> = record.extra.iter()
                .map(|(k, v)| format!("\"{}\":\"{}\"", k, escape_json(v)))
                .collect();
            obj.push_str(&extras.join(","));
            obj.push('}');
        }

        obj.push('}');
        obj
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// =============================================================================
// Handler
// =============================================================================

/// Log handler interface.
pub trait Handler: Send + Sync {
    /// Handle a log record.
    fn handle(&self, record: &Record);

    /// Set minimum level.
    fn set_level(&mut self, level: Level);

    /// Get minimum level.
    fn level(&self) -> Level;
}

/// Console handler (writes to stderr).
pub struct ConsoleHandler {
    level: Level,
    formatter: Arc<dyn Formatter>,
    use_colors: bool,
}

impl ConsoleHandler {
    /// Create a new console handler.
    pub fn new() -> Self {
        Self {
            level: Level::Debug,
            formatter: Arc::new(DefaultFormatter),
            use_colors: true,
        }
    }

    /// Set formatter.
    pub fn with_formatter(mut self, formatter: impl Formatter + 'static) -> Self {
        self.formatter = Arc::new(formatter);
        self
    }

    /// Disable colors.
    pub fn no_colors(mut self) -> Self {
        self.use_colors = false;
        self
    }
}

impl Default for ConsoleHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for ConsoleHandler {
    fn handle(&self, record: &Record) {
        if record.level < self.level {
            return;
        }

        let msg = self.formatter.format(record);

        if self.use_colors {
            let colored = match record.level {
                Level::Debug => format!("\x1b[36m{}\x1b[0m", msg),    // Cyan
                Level::Info => format!("\x1b[32m{}\x1b[0m", msg),     // Green
                Level::Warning => format!("\x1b[33m{}\x1b[0m", msg),  // Yellow
                Level::Error => format!("\x1b[31m{}\x1b[0m", msg),    // Red
                Level::Critical => format!("\x1b[1;31m{}\x1b[0m", msg), // Bold Red
            };
            eprintln!("{}", colored);
        } else {
            eprintln!("{}", msg);
        }
    }

    fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    fn level(&self) -> Level {
        self.level
    }
}

/// File handler (writes to a file).
pub struct FileHandler {
    level: Level,
    formatter: Arc<dyn Formatter>,
    file: Mutex<std::fs::File>,
}

impl FileHandler {
    /// Create a new file handler.
    pub fn new(path: &str) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        Ok(Self {
            level: Level::Debug,
            formatter: Arc::new(DetailedFormatter::default()),
            file: Mutex::new(file),
        })
    }

    /// Set formatter.
    pub fn with_formatter(mut self, formatter: impl Formatter + 'static) -> Self {
        self.formatter = Arc::new(formatter);
        self
    }
}

impl Handler for FileHandler {
    fn handle(&self, record: &Record) {
        if record.level < self.level {
            return;
        }

        let msg = self.formatter.format(record);

        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{}", msg);
        }
    }

    fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    fn level(&self) -> Level {
        self.level
    }
}

// =============================================================================
// Logger
// =============================================================================

/// A logger instance.
pub struct Logger {
    name: String,
    level: RwLock<Level>,
    handlers: RwLock<Vec<Arc<dyn Handler>>>,
    parent: Option<Arc<Logger>>,
}

impl Logger {
    /// Create a new logger.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level: RwLock::new(Level::Debug),
            handlers: RwLock::new(Vec::new()),
            parent: None,
        }
    }

    /// Get logger name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set minimum level.
    pub fn set_level(&self, level: Level) {
        *self.level.write().unwrap() = level;
    }

    /// Get minimum level.
    pub fn level(&self) -> Level {
        *self.level.read().unwrap()
    }

    /// Add a handler.
    pub fn add_handler(&self, handler: impl Handler + 'static) {
        self.handlers.write().unwrap().push(Arc::new(handler));
    }

    /// Log a record.
    pub fn log(&self, record: &Record) {
        let level = *self.level.read().unwrap();
        if record.level < level {
            return;
        }

        let handlers = self.handlers.read().unwrap();
        for handler in handlers.iter() {
            handler.handle(record);
        }

        // Propagate to parent
        if let Some(ref parent) = self.parent {
            parent.log(record);
        }
    }

    /// Log a message at the given level.
    pub fn log_msg(&self, level: Level, message: &str) {
        let record = Record::new(level, message, &self.name);
        self.log(&record);
    }

    /// Log a debug message.
    pub fn debug(&self, message: &str) {
        self.log_msg(Level::Debug, message);
    }

    /// Log an info message.
    pub fn info(&self, message: &str) {
        self.log_msg(Level::Info, message);
    }

    /// Log a warning message.
    pub fn warning(&self, message: &str) {
        self.log_msg(Level::Warning, message);
    }

    /// Log an error message.
    pub fn error(&self, message: &str) {
        self.log_msg(Level::Error, message);
    }

    /// Log a critical message.
    pub fn critical(&self, message: &str) {
        self.log_msg(Level::Critical, message);
    }
}

// =============================================================================
// Global Logger Registry
// =============================================================================

lazy_static::lazy_static! {
    static ref LOGGERS: RwLock<HashMap<String, Arc<Logger>>> = RwLock::new(HashMap::new());
    static ref ROOT_LOGGER: Arc<Logger> = {
        let logger = Logger::new("root");
        logger.add_handler(ConsoleHandler::new());
        Arc::new(logger)
    };
}

/// Get or create a logger by name.
pub fn get_logger(name: &str) -> Arc<Logger> {
    {
        let loggers = LOGGERS.read().unwrap();
        if let Some(logger) = loggers.get(name) {
            return Arc::clone(logger);
        }
    }

    let logger = Arc::new(Logger::new(name));
    LOGGERS.write().unwrap().insert(name.to_string(), Arc::clone(&logger));
    logger
}

/// Get the root logger.
pub fn root() -> Arc<Logger> {
    Arc::clone(&ROOT_LOGGER)
}

/// Configure basic logging (convenience function).
pub fn basic_config(level: Level) {
    ROOT_LOGGER.set_level(level);
}

// =============================================================================
// Convenience Macros (for Rust code)
// =============================================================================

/// Log at debug level.
#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $($arg:tt)*) => {
        $logger.debug(&format!($($arg)*))
    };
}

/// Log at info level.
#[macro_export]
macro_rules! log_info {
    ($logger:expr, $($arg:tt)*) => {
        $logger.info(&format!($($arg)*))
    };
}

/// Log at warning level.
#[macro_export]
macro_rules! log_warning {
    ($logger:expr, $($arg:tt)*) => {
        $logger.warning(&format!($($arg)*))
    };
}

/// Log at error level.
#[macro_export]
macro_rules! log_error {
    ($logger:expr, $($arg:tt)*) => {
        $logger.error(&format!($($arg)*))
    };
}

/// Log at critical level.
#[macro_export]
macro_rules! log_critical {
    ($logger:expr, $($arg:tt)*) => {
        $logger.critical(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_ordering() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Warning);
        assert!(Level::Warning < Level::Error);
        assert!(Level::Error < Level::Critical);
    }

    #[test]
    fn test_level_from_name() {
        assert_eq!(Level::from_name("DEBUG"), Some(Level::Debug));
        assert_eq!(Level::from_name("info"), Some(Level::Info));
        assert_eq!(Level::from_name("WARN"), Some(Level::Warning));
        assert_eq!(Level::from_name("ERROR"), Some(Level::Error));
        assert_eq!(Level::from_name("FATAL"), Some(Level::Critical));
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter;
        let record = Record::new(Level::Info, "test message", "test_logger");
        let json = formatter.format(&record);
        assert!(json.contains("\"level\":\"INFO\""));
        assert!(json.contains("\"message\":\"test message\""));
    }

    #[test]
    fn test_logger_basic() {
        let logger = Logger::new("test");
        logger.set_level(Level::Info);
        assert_eq!(logger.level(), Level::Info);
    }
}

//! Environment configuration for Roast.
//!
//! Handles `ROAST_HOME` and other environment-based settings.
//!
//! # Environment Variables
//!
//! - `ROAST_HOME` - Base directory for Roast data (default: `~/.roast`)
//! - `ROAST_CACHE` - Cache directory (default: `$ROAST_HOME/cache`)
//! - `ROAST_REGISTRY` - Custom registry URL
//! - `ROAST_LOG` - Log level (debug, info, warn, error)
//! - `ROAST_NO_COLOR` - Disable colored output

use std::env;
use std::path::PathBuf;

/// Roast environment configuration.
#[derive(Debug, Clone)]
pub struct RoastEnv {
    /// Base directory for Roast data.
    pub home: PathBuf,
    /// Cache directory.
    pub cache: PathBuf,
    /// Registry URL.
    pub registry: Option<String>,
    /// Log level.
    pub log_level: LogLevel,
    /// Disable colors.
    pub no_color: bool,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl LogLevel {
    /// Parse from string.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "debug" | "trace" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" | "warning" => LogLevel::Warn,
            "error" => LogLevel::Error,
            "off" | "none" | "quiet" => LogLevel::Off,
            _ => LogLevel::Info,
        }
    }
}

impl RoastEnv {
    /// Get environment configuration.
    pub fn get() -> Self {
        let home = Self::get_home();
        let cache = Self::get_cache(&home);
        let registry = env::var("ROAST_REGISTRY").ok();
        let log_level = env::var("ROAST_LOG")
            .map(|s| LogLevel::from_str(&s))
            .unwrap_or_default();
        let no_color = env::var("ROAST_NO_COLOR").is_ok()
            || env::var("NO_COLOR").is_ok();

        Self {
            home,
            cache,
            registry,
            log_level,
            no_color,
        }
    }

    /// Get ROAST_HOME directory.
    fn get_home() -> PathBuf {
        if let Ok(home) = env::var("ROAST_HOME") {
            return PathBuf::from(home);
        }

        // Default: ~/.roast
        if let Some(home) = dirs::home_dir() {
            return home.join(".roast");
        }

        // Fallback
        PathBuf::from(".roast")
    }

    /// Get cache directory.
    fn get_cache(home: &PathBuf) -> PathBuf {
        if let Ok(cache) = env::var("ROAST_CACHE") {
            return PathBuf::from(cache);
        }

        home.join("cache")
    }

    /// Get the bin directory.
    pub fn bin_dir(&self) -> PathBuf {
        self.home.join("bin")
    }

    /// Get the lib directory.
    pub fn lib_dir(&self) -> PathBuf {
        self.home.join("lib")
    }

    /// Get the toolchains directory.
    pub fn toolchains_dir(&self) -> PathBuf {
        self.home.join("toolchains")
    }

    /// Get the packages directory.
    pub fn packages_dir(&self) -> PathBuf {
        self.home.join("packages")
    }

    /// Get the global config file path.
    pub fn config_file(&self) -> PathBuf {
        self.home.join("config.toml")
    }

    /// Ensure all directories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(self.bin_dir())?;
        std::fs::create_dir_all(self.lib_dir())?;
        std::fs::create_dir_all(self.packages_dir())?;
        Ok(())
    }
}

impl Default for RoastEnv {
    fn default() -> Self {
        Self::get()
    }
}

/// Get ROAST_HOME path (convenience function).
pub fn roast_home() -> PathBuf {
    RoastEnv::get().home
}

/// Get cache path (convenience function).
pub fn cache_dir() -> PathBuf {
    RoastEnv::get().cache
}

/// Check if colors are disabled.
pub fn no_color() -> bool {
    RoastEnv::get().no_color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_home() {
        let env = RoastEnv::get();
        assert!(env.home.to_string_lossy().contains("roast") ||
                env.home.to_string_lossy().contains(".roast"));
    }

    #[test]
    fn test_log_level_parse() {
        assert_eq!(LogLevel::from_str("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("INFO"), LogLevel::Info);
        assert_eq!(LogLevel::from_str("WARN"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("error"), LogLevel::Error);
    }
}

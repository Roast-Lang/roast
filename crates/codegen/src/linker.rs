//! Linker support for native code generation.
//!
//! This module provides utilities for linking object files into
//! executables and shared libraries.

use std::path::PathBuf;
use std::process::Command;
use crate::CodegenError;

/// Linker configuration.
#[derive(Clone, Debug)]
pub struct LinkerConfig {
    /// Output path for the executable/library.
    pub output: PathBuf,
    /// Whether to create a shared library.
    pub shared: bool,
    /// Whether to strip debug symbols.
    pub strip: bool,
    /// Additional library directories.
    pub lib_dirs: Vec<PathBuf>,
    /// Libraries to link.
    pub libs: Vec<String>,
    /// Additional linker flags.
    pub flags: Vec<String>,
    /// Linker to use (auto-detected if None).
    pub linker: Option<String>,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            output: PathBuf::from("a.out"),
            shared: false,
            strip: false,
            lib_dirs: Vec::new(),
            libs: Vec::new(),
            flags: Vec::new(),
            linker: None,
        }
    }
}

impl LinkerConfig {
    /// Create a new linker configuration for an executable.
    pub fn executable<P: Into<PathBuf>>(output: P) -> Self {
        Self {
            output: output.into(),
            ..Default::default()
        }
    }

    /// Create a new linker configuration for a shared library.
    pub fn shared_library<P: Into<PathBuf>>(output: P) -> Self {
        Self {
            output: output.into(),
            shared: true,
            ..Default::default()
        }
    }

    /// Add a library directory.
    pub fn lib_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.lib_dirs.push(dir.into());
        self
    }

    /// Add a library to link.
    pub fn lib<S: Into<String>>(mut self, name: S) -> Self {
        self.libs.push(name.into());
        self
    }

    /// Add a linker flag.
    pub fn flag<S: Into<String>>(mut self, flag: S) -> Self {
        self.flags.push(flag.into());
        self
    }

    /// Strip debug symbols.
    pub fn strip(mut self) -> Self {
        self.strip = true;
        self
    }

    /// Use a specific linker.
    pub fn linker<S: Into<String>>(mut self, linker: S) -> Self {
        self.linker = Some(linker.into());
        self
    }
}

/// Linker abstraction.
pub struct Linker {
    config: LinkerConfig,
    objects: Vec<PathBuf>,
}

impl Linker {
    /// Create a new linker.
    pub fn new(config: LinkerConfig) -> Self {
        Self {
            config,
            objects: Vec::new(),
        }
    }

    /// Add an object file.
    pub fn add_object<P: Into<PathBuf>>(&mut self, path: P) {
        self.objects.push(path.into());
    }

    /// Add multiple object files.
    pub fn add_objects<I, P>(&mut self, paths: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        for path in paths {
            self.objects.push(path.into());
        }
    }

    /// Perform linking.
    pub fn link(&self) -> Result<(), CodegenError> {
        let linker = self.detect_linker()?;

        let mut cmd = Command::new(&linker);

        // Add objects
        for obj in &self.objects {
            cmd.arg(obj);
        }

        // Output
        cmd.arg("-o").arg(&self.config.output);

        // Shared library flag
        if self.config.shared {
            if cfg!(target_os = "macos") {
                cmd.arg("-dynamiclib");
            } else {
                cmd.arg("-shared");
            }
        }

        // Library directories
        for dir in &self.config.lib_dirs {
            cmd.arg("-L").arg(dir);
        }

        // Libraries
        for lib in &self.config.libs {
            cmd.arg("-l").arg(lib);
        }

        // Strip
        if self.config.strip {
            cmd.arg("-s");
        }

        // Additional flags
        for flag in &self.config.flags {
            cmd.arg(flag);
        }

        // Common runtime libraries
        self.add_runtime_libs(&mut cmd);

        let output = cmd.output()
            .map_err(|e| CodegenError::Target(format!("Failed to run linker: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CodegenError::Target(format!("Linking failed: {}", stderr)));
        }

        Ok(())
    }

    /// Detect the system linker.
    fn detect_linker(&self) -> Result<String, CodegenError> {
        if let Some(ref linker) = self.config.linker {
            return Ok(linker.clone());
        }

        // Try various linkers
        let candidates = if cfg!(target_os = "macos") {
            vec!["clang", "ld"]
        } else if cfg!(target_os = "windows") {
            vec!["link.exe", "lld-link"]
        } else {
            vec!["cc", "gcc", "clang", "ld"]
        };

        for candidate in candidates {
            if Self::is_linker_available(candidate) {
                return Ok(candidate.to_string());
            }
        }

        Err(CodegenError::Target("No linker found".to_string()))
    }

    /// Check if a linker is available.
    fn is_linker_available(name: &str) -> bool {
        Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Add common runtime libraries.
    fn add_runtime_libs(&self, cmd: &mut Command) {
        if cfg!(target_os = "linux") {
            // Common Linux libraries
            cmd.arg("-lc");
            cmd.arg("-lm");
            cmd.arg("-lpthread");
            cmd.arg("-ldl");
        } else if cfg!(target_os = "macos") {
            // macOS uses system libraries
            cmd.arg("-lSystem");
        }
    }
}

/// Create a minimal C runtime startup for standalone executables.
pub fn create_crt_startup() -> Vec<u8> {
    // This would contain platform-specific startup code
    // For now, return empty
    Vec::new()
}

/// Platform-specific object file extension.
pub fn object_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    }
}

/// Platform-specific executable extension.
pub fn executable_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// Platform-specific shared library extension.
pub fn shared_library_extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_config() {
        let config = LinkerConfig::executable("output")
            .lib_dir("/usr/lib")
            .lib("m")
            .strip();

        assert_eq!(config.output, PathBuf::from("output"));
        assert!(!config.shared);
        assert!(config.strip);
        assert_eq!(config.libs, vec!["m"]);
    }
}

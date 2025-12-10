//! Target configuration for code generation.

use std::fmt;

/// Target architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    Wasm32,
    Bytecode,  // Roast VM bytecode
}

impl Arch {
    pub fn native() -> Self {
        #[cfg(target_arch = "x86_64")]
        return Arch::X86_64;
        
        #[cfg(target_arch = "aarch64")]
        return Arch::Aarch64;
        
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        return Arch::Bytecode;
    }

    pub fn pointer_size(&self) -> u8 {
        match self {
            Arch::X86_64 | Arch::Aarch64 => 8,
            Arch::Wasm32 | Arch::Bytecode => 4,
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::Aarch64 => write!(f, "aarch64"),
            Arch::Wasm32 => write!(f, "wasm32"),
            Arch::Bytecode => write!(f, "bytecode"),
        }
    }
}

/// Target operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOS,
    Windows,
    Wasi,
    None,  // Bare metal / VM
}

impl Os {
    pub fn native() -> Self {
        #[cfg(target_os = "linux")]
        return Os::Linux;
        
        #[cfg(target_os = "macos")]
        return Os::MacOS;
        
        #[cfg(target_os = "windows")]
        return Os::Windows;
        
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        return Os::None;
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Os::Linux => write!(f, "linux"),
            Os::MacOS => write!(f, "macos"),
            Os::Windows => write!(f, "windows"),
            Os::Wasi => write!(f, "wasi"),
            Os::None => write!(f, "none"),
        }
    }
}

/// Target triple representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub arch: Arch,
    pub os: Os,
    pub env: Option<String>,
}

impl Target {
    /// Creates a new target.
    pub fn new(arch: Arch, os: Os) -> Self {
        Self { arch, os, env: None }
    }

    /// Returns the native target (current platform).
    pub fn native() -> Self {
        Self {
            arch: Arch::native(),
            os: Os::native(),
            env: None,
        }
    }

    /// Returns the bytecode target.
    pub fn bytecode() -> Self {
        Self {
            arch: Arch::Bytecode,
            os: Os::None,
            env: None,
        }
    }

    /// Returns the target triple string.
    pub fn triple(&self) -> String {
        match (&self.arch, &self.os) {
            (Arch::X86_64, Os::Linux) => "x86_64-unknown-linux-gnu".into(),
            (Arch::X86_64, Os::MacOS) => "x86_64-apple-darwin".into(),
            (Arch::X86_64, Os::Windows) => "x86_64-pc-windows-msvc".into(),
            (Arch::Aarch64, Os::Linux) => "aarch64-unknown-linux-gnu".into(),
            (Arch::Aarch64, Os::MacOS) => "aarch64-apple-darwin".into(),
            (Arch::Wasm32, Os::Wasi) => "wasm32-wasi".into(),
            (Arch::Wasm32, Os::None) => "wasm32-unknown-unknown".into(),
            (Arch::Bytecode, _) => "roast-vm".into(),
            _ => format!("{}-{}", self.arch, self.os),
        }
    }

    /// Parses a target triple string.
    pub fn parse(triple: &str) -> Option<Self> {
        let parts: Vec<&str> = triple.split('-').collect();
        if parts.is_empty() {
            return None;
        }

        let arch = match parts[0] {
            "x86_64" => Arch::X86_64,
            "aarch64" | "arm64" => Arch::Aarch64,
            "wasm32" => Arch::Wasm32,
            "bytecode" | "roast" => Arch::Bytecode,
            _ => return None,
        };

        let os = if parts.len() > 2 {
            match parts[2] {
                "linux" => Os::Linux,
                "darwin" | "macos" => Os::MacOS,
                "windows" => Os::Windows,
                "wasi" => Os::Wasi,
                _ => Os::None,
            }
        } else {
            Os::None
        };

        Some(Self { arch, os, env: None })
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.triple())
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::native()
    }
}

/// Configuration for code generation.
#[derive(Clone, Debug)]
pub struct TargetConfig {
    pub target: Target,
    pub opt_level: OptLevel,
    pub debug_info: bool,
    pub pic: bool,  // Position independent code
}

/// Optimization level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OptLevel {
    #[default]
    None,       // -O0
    Less,       // -O1
    Default,    // -O2
    Aggressive, // -O3
    Size,       // -Os
    SizeMin,    // -Oz
}

impl OptLevel {
    pub fn from_u8(level: u8) -> Self {
        match level {
            0 => OptLevel::None,
            1 => OptLevel::Less,
            2 => OptLevel::Default,
            3 => OptLevel::Aggressive,
            _ => OptLevel::Aggressive,
        }
    }
}

impl TargetConfig {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            opt_level: OptLevel::None,
            debug_info: true,
            pic: false,
        }
    }

    pub fn native() -> Self {
        Self::new(Target::native())
    }

    pub fn bytecode() -> Self {
        Self::new(Target::bytecode())
    }

    pub fn with_opt_level(mut self, level: OptLevel) -> Self {
        self.opt_level = level;
        self
    }

    pub fn with_debug_info(mut self, debug: bool) -> Self {
        self.debug_info = debug;
        self
    }

    pub fn release() -> Self {
        Self::native()
            .with_opt_level(OptLevel::Aggressive)
            .with_debug_info(false)
    }
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self::native()
    }
}


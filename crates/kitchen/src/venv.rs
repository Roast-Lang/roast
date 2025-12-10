//! Virtual environment management.

use std::path::{Path, PathBuf};
use std::fs;
// use std::process::Command;
use std::collections::HashMap;
use std::env;
use crate::{Error, Result};

/// A Roast virtual environment.
pub struct VirtualEnv {
    /// Path to the virtual environment.
    pub path: PathBuf,
    
    /// Python interpreter path (for compatibility).
    pub python_path: Option<PathBuf>,
    
    /// Environment configuration.
    pub config: VenvConfig,
}

/// Virtual environment configuration.
#[derive(Debug, Clone, Default)]
pub struct VenvConfig {
    /// Roast version.
    pub roast_version: String,
    
    /// Python version (for compatibility).
    pub python_version: Option<String>,
    
    /// Installed packages.
    pub packages: HashMap<String, String>,
    
    /// System site packages.
    pub system_site_packages: bool,
    
    /// Include pip.
    pub include_pip: bool,
}

impl VirtualEnv {
    /// Create a new virtual environment.
    pub fn create(path: &Path, config: VenvConfig) -> Result<Self> {
        if path.exists() {
            return Err(Error::AlreadyExists(
                format!("Virtual environment already exists: {}", path.display())
            ));
        }
        
        // Create directory structure
        fs::create_dir_all(path)?;
        fs::create_dir_all(path.join("bin"))?;
        fs::create_dir_all(path.join("lib"))?;
        fs::create_dir_all(path.join("lib/roast"))?;
        fs::create_dir_all(path.join("include"))?;
        fs::create_dir_all(path.join("share"))?;
        
        // Create pyvenv.cfg equivalent
        let cfg_content = format!(
            r#"# Roast virtual environment configuration
roast-version = {}
home = {}
include-system-site-packages = {}
"#,
            config.roast_version,
            env::current_exe()
                .map(|p| p.parent().unwrap_or(Path::new("")).display().to_string())
                .unwrap_or_default(),
            config.system_site_packages
        );
        fs::write(path.join("roastvenv.cfg"), cfg_content)?;
        
        // Create activation scripts
        Self::create_activation_scripts(path)?;
        
        // Create roast wrapper script
        Self::create_roast_wrapper(path)?;
        
        let venv = Self {
            path: path.to_path_buf(),
            python_path: None,
            config,
        };
        
        Ok(venv)
    }
    
    /// Load an existing virtual environment.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::NotFound(
                format!("Virtual environment not found: {}", path.display())
            ));
        }
        
        let cfg_path = path.join("roastvenv.cfg");
        if !cfg_path.exists() {
            return Err(Error::InvalidConfig(
                "Not a valid Roast virtual environment".to_string()
            ));
        }
        
        // Parse config
        let cfg_content = fs::read_to_string(&cfg_path)?;
        let config = Self::parse_config(&cfg_content)?;
        
        Ok(Self {
            path: path.to_path_buf(),
            python_path: None,
            config,
        })
    }
    
    fn parse_config(content: &str) -> Result<VenvConfig> {
        let mut config = VenvConfig::default();
        
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                
                match key {
                    "roast-version" => config.roast_version = value.to_string(),
                    "include-system-site-packages" => {
                        config.system_site_packages = value == "true";
                    }
                    _ => {}
                }
            }
        }
        
        Ok(config)
    }
    
    fn create_activation_scripts(path: &Path) -> Result<()> {
        let bin_dir = path.join("bin");
        
        // Bash/Zsh activation script
        let activate_sh = format!(
            r#"#!/bin/bash
# Roast virtual environment activation script

deactivate() {{
    if [ -n "$_OLD_VIRTUAL_PATH" ]; then
        export PATH="$_OLD_VIRTUAL_PATH"
        unset _OLD_VIRTUAL_PATH
    fi
    
    if [ -n "$_OLD_VIRTUAL_PS1" ]; then
        export PS1="$_OLD_VIRTUAL_PS1"
        unset _OLD_VIRTUAL_PS1
    fi
    
    unset VIRTUAL_ENV
    unset ROAST_VENV
    
    if [ "$1" != "nondestructive" ]; then
        unset -f deactivate
    fi
}}

# Ensure clean state
deactivate nondestructive

export VIRTUAL_ENV="{}"
export ROAST_VENV="$VIRTUAL_ENV"

_OLD_VIRTUAL_PATH="$PATH"
export PATH="$VIRTUAL_ENV/bin:$PATH"

_OLD_VIRTUAL_PS1="${{PS1:-}}"
export PS1="($(basename "$VIRTUAL_ENV")) $PS1"
"#,
            path.display()
        );
        fs::write(bin_dir.join("activate"), activate_sh)?;
        
        // Fish activation script
        let activate_fish = format!(
            r#"# Roast virtual environment activation script for fish

function deactivate -d "Deactivate the virtual environment"
    if set -q _OLD_VIRTUAL_PATH
        set -gx PATH $_OLD_VIRTUAL_PATH
        set -e _OLD_VIRTUAL_PATH
    end
    
    if set -q _OLD_FISH_PROMPT_OVERRIDE
        set -e _OLD_FISH_PROMPT_OVERRIDE
        functions -e fish_prompt
        functions -c _old_fish_prompt fish_prompt
        functions -e _old_fish_prompt
    end
    
    set -e VIRTUAL_ENV
    set -e ROAST_VENV
end

deactivate

set -gx VIRTUAL_ENV "{}"
set -gx ROAST_VENV "$VIRTUAL_ENV"

set -gx _OLD_VIRTUAL_PATH $PATH
set -gx PATH "$VIRTUAL_ENV/bin" $PATH

if set -q FISH_VERSION
    set -gx _OLD_FISH_PROMPT_OVERRIDE 1
    functions -c fish_prompt _old_fish_prompt
    function fish_prompt
        echo -n "($(basename $VIRTUAL_ENV)) "
        _old_fish_prompt
    end
end
"#,
            path.display()
        );
        fs::write(bin_dir.join("activate.fish"), activate_fish)?;
        
        // PowerShell activation script
        let activate_ps1 = format!(
            r#"# Roast virtual environment activation script for PowerShell

function global:deactivate {{
    if ($env:_OLD_VIRTUAL_PATH) {{
        $env:PATH = $env:_OLD_VIRTUAL_PATH
        Remove-Item env:_OLD_VIRTUAL_PATH
    }}
    
    Remove-Item env:VIRTUAL_ENV -ErrorAction SilentlyContinue
    Remove-Item env:ROAST_VENV -ErrorAction SilentlyContinue
    
    if (Get-Command _old_virtual_prompt -ErrorAction SilentlyContinue) {{
        Set-Item function:prompt (Get-Command _old_virtual_prompt).ScriptBlock
        Remove-Item function:_old_virtual_prompt
    }}
}}

deactivate

$env:VIRTUAL_ENV = "{}"
$env:ROAST_VENV = $env:VIRTUAL_ENV

$env:_OLD_VIRTUAL_PATH = $env:PATH
$env:PATH = "$env:VIRTUAL_ENV\bin;$env:PATH"

Copy-Item function:prompt function:_old_virtual_prompt
function global:prompt {{
    Write-Host -NoNewline -ForegroundColor Green "($(Split-Path $env:VIRTUAL_ENV -Leaf)) "
    _old_virtual_prompt
}}
"#,
            path.display()
        );
        fs::write(bin_dir.join("Activate.ps1"), activate_ps1)?;
        
        Ok(())
    }
    
    fn create_roast_wrapper(path: &Path) -> Result<()> {
        let bin_dir = path.join("bin");
        
        // Unix wrapper
        #[cfg(unix)]
        {
            let wrapper = r#"#!/bin/sh
exec roastc "$@"
"#;
            let wrapper_path = bin_dir.join("roast");
            fs::write(&wrapper_path, wrapper)?;
            
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&wrapper_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&wrapper_path, perms)?;
        }
        
        // Windows wrapper
        #[cfg(windows)]
        {
            let wrapper = r#"@echo off
roastc %*
"#;
            fs::write(bin_dir.join("roast.cmd"), wrapper)?;
        }
        
        Ok(())
    }
    
    /// Get the bin directory.
    pub fn bin_dir(&self) -> PathBuf {
        self.path.join("bin")
    }
    
    /// Get the lib directory.
    pub fn lib_dir(&self) -> PathBuf {
        self.path.join("lib/roast")
    }
    
    /// Check if this environment is active.
    pub fn is_active(&self) -> bool {
        env::var("ROAST_VENV")
            .map(|v| PathBuf::from(v) == self.path)
            .unwrap_or(false)
    }
    
    /// Get environment variables for activation.
    pub fn activation_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        
        env.insert("VIRTUAL_ENV".to_string(), self.path.display().to_string());
        env.insert("ROAST_VENV".to_string(), self.path.display().to_string());
        
        // Prepend bin to PATH
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", self.bin_dir().display(), path);
        env.insert("PATH".to_string(), new_path);
        
        env
    }
    
    /// Install a package into the virtual environment.
    pub fn install_package(&mut self, name: &str, version: &str) -> Result<()> {
        let lib_dir = self.lib_dir();
        let package_dir = lib_dir.join(name);
        
        fs::create_dir_all(&package_dir)?;
        
        // Create package marker
        let marker = format!("{} @ {}", name, version);
        fs::write(package_dir.join("VERSION"), &marker)?;
        
        self.config.packages.insert(name.to_string(), version.to_string());
        
        Ok(())
    }
    
    /// Uninstall a package from the virtual environment.
    pub fn uninstall_package(&mut self, name: &str) -> Result<()> {
        let lib_dir = self.lib_dir();
        let package_dir = lib_dir.join(name);
        
        if package_dir.exists() {
            fs::remove_dir_all(&package_dir)?;
        }
        
        self.config.packages.remove(name);
        
        Ok(())
    }
    
    /// List installed packages.
    pub fn list_packages(&self) -> Result<Vec<(String, String)>> {
        let lib_dir = self.lib_dir();
        let mut packages = Vec::new();
        
        if lib_dir.exists() {
            for entry in fs::read_dir(&lib_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_dir() {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    
                    let version_file = path.join("VERSION");
                    let version = if version_file.exists() {
                        fs::read_to_string(&version_file)?
                            .split('@')
                            .nth(1)
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    } else {
                        "unknown".to_string()
                    };
                    
                    packages.push((name, version));
                }
            }
        }
        
        Ok(packages)
    }
    
    /// Delete the virtual environment.
    pub fn delete(self) -> Result<()> {
        if self.is_active() {
            return Err(Error::VirtualEnv(
                "Cannot delete active virtual environment. Run 'deactivate' first.".to_string()
            ));
        }
        
        fs::remove_dir_all(&self.path)?;
        Ok(())
    }
}

/// Find the default virtual environment for a project.
pub fn find_venv(project_root: &Path) -> Option<PathBuf> {
    // Check for .venv in project
    let venv_path = project_root.join(".venv");
    if venv_path.exists() && venv_path.join("roastvenv.cfg").exists() {
        return Some(venv_path);
    }
    
    // Check for .kitchen/venv
    let kitchen_venv = project_root.join(".kitchen/venv");
    if kitchen_venv.exists() && kitchen_venv.join("roastvenv.cfg").exists() {
        return Some(kitchen_venv);
    }
    
    None
}

/// Get the active virtual environment.
pub fn active_venv() -> Option<PathBuf> {
    env::var("ROAST_VENV").ok().map(PathBuf::from)
}


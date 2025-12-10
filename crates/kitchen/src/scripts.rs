//! Script execution.

use std::collections::HashMap;
use std::process::{Command, Stdio, ExitStatus};
use std::path::Path;
use crate::{Error, Result};

/// Script runner.
pub struct ScriptRunner {
    /// Available scripts.
    scripts: HashMap<String, String>,
    
    /// Working directory.
    working_dir: std::path::PathBuf,
    
    /// Environment variables.
    env: HashMap<String, String>,
}

impl ScriptRunner {
    /// Create a new script runner.
    pub fn new(working_dir: &Path) -> Self {
        Self {
            scripts: HashMap::new(),
            working_dir: working_dir.to_path_buf(),
            env: HashMap::new(),
        }
    }
    
    /// Add scripts from config.
    pub fn with_scripts(mut self, scripts: HashMap<String, String>) -> Self {
        self.scripts = scripts;
        self
    }
    
    /// Add environment variable.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Run a script by name.
    pub fn run(&self, name: &str, args: &[String]) -> Result<ExitStatus> {
        let script = self.scripts.get(name)
            .ok_or_else(|| Error::NotFound(format!("Script '{}' not found", name)))?;
        
        self.run_command(script, args)
    }
    
    /// Run an arbitrary command.
    pub fn run_command(&self, command: &str, args: &[String]) -> Result<ExitStatus> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        
        if parts.is_empty() {
            return Err(Error::InvalidConfig("Empty command".to_string()));
        }
        
        let _program = parts[0];
        let _script_args: Vec<&str> = parts[1..].to_vec();
        
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", &format!("{} {}", command, args.join(" "))]);
            c
        };
        
        cmd.current_dir(&self.working_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        
        // Add environment
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        
        let status = cmd.status()?;
        
        Ok(status)
    }
    
    /// List available scripts.
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut scripts: Vec<_> = self.scripts.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        scripts.sort_by_key(|(k, _)| *k);
        scripts
    }
    
    /// Check if a script exists.
    pub fn has(&self, name: &str) -> bool {
        self.scripts.contains_key(name)
    }
}

/// Built-in script commands.
pub fn builtin_scripts() -> HashMap<String, String> {
    let mut scripts = HashMap::new();
    
    scripts.insert("build".to_string(), "kitchen build".to_string());
    scripts.insert("test".to_string(), "kitchen test".to_string());
    scripts.insert("check".to_string(), "roastc check src/".to_string());
    scripts.insert("fmt".to_string(), "roastc fmt src/".to_string());
    scripts.insert("lint".to_string(), "roastc check --strict src/".to_string());
    scripts.insert("clean".to_string(), "kitchen clean".to_string());
    scripts.insert("doc".to_string(), "roastc doc".to_string());
    
    scripts
}

/// Pre/post hook types.
#[derive(Debug, Clone, Copy)]
pub enum HookType {
    PreBuild,
    PostBuild,
    PreTest,
    PostTest,
    PreInstall,
    PostInstall,
    PrePublish,
    PostPublish,
}

impl HookType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::PreBuild => "pre-build",
            Self::PostBuild => "post-build",
            Self::PreTest => "pre-test",
            Self::PostTest => "post-test",
            Self::PreInstall => "pre-install",
            Self::PostInstall => "post-install",
            Self::PrePublish => "pre-publish",
            Self::PostPublish => "post-publish",
        }
    }
}

/// Run a hook if it exists.
pub fn run_hook(runner: &ScriptRunner, hook: HookType) -> Result<()> {
    let hook_name = hook.as_str();
    
    if runner.has(hook_name) {
        println!("Running {} hook...", hook_name);
        let status = runner.run(hook_name, &[])?;
        
        if !status.success() {
            return Err(Error::Build(format!(
                "{} hook failed with exit code {}",
                hook_name,
                status.code().unwrap_or(-1)
            )));
        }
    }
    
    Ok(())
}


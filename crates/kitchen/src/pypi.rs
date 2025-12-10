//! PyPI (Python Package Index) integration.
//!
//! Allows Roast to install and use Python packages from PyPI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::fs;
use serde::{Deserialize, Serialize};
use crate::{Error, Result};

/// PyPI API base URL.
pub const PYPI_API_URL: &str = "https://pypi.org/pypi";

/// PyPI simple API URL.
pub const PYPI_SIMPLE_URL: &str = "https://pypi.org/simple";

/// Python package specification.
#[derive(Debug, Clone)]
pub struct PythonPackage {
    /// Package name (without py: prefix).
    pub name: String,
    /// Version requirement.
    pub version: Option<String>,
    /// Is dev dependency.
    pub dev: bool,
}

impl PythonPackage {
    /// Parse a Python package specification.
    /// 
    /// Examples:
    /// - "requests" -> (requests, *)
    /// - "requests==2.28.0" -> (requests, ==2.28.0)
    /// - "requests>=2.0,<3.0" -> (requests, >=2.0,<3.0)
    pub fn parse(spec: &str) -> Self {
        let spec = spec.trim();
        
        // Find version specifier
        let operators = [">=", "<=", "==", "!=", "~=", ">", "<"];
        
        for op in &operators {
            if let Some(pos) = spec.find(op) {
                return Self {
                    name: spec[..pos].trim().to_string(),
                    version: Some(spec[pos..].to_string()),
                    dev: false,
                };
            }
        }
        
        // No version specifier
        Self {
            name: spec.to_string(),
            version: None,
            dev: false,
        }
    }
    
    /// Get pip install specifier.
    pub fn pip_spec(&self) -> String {
        if let Some(ref v) = self.version {
            format!("{}{}", self.name, v)
        } else {
            self.name.clone()
        }
    }
}

/// PyPI package metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyPIPackage {
    pub info: PyPIInfo,
    pub releases: HashMap<String, Vec<PyPIRelease>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyPIInfo {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
    pub home_page: Option<String>,
    pub author: Option<String>,
    pub author_email: Option<String>,
    pub license: Option<String>,
    pub requires_python: Option<String>,
    pub requires_dist: Option<Vec<String>>,
    pub keywords: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyPIRelease {
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub digests: PyPIDigests,
    pub packagetype: String,
    pub python_version: String,
    pub requires_python: Option<String>,
    pub yanked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyPIDigests {
    pub md5: String,
    pub sha256: String,
}

/// PyPI client for fetching package information.
pub struct PyPIClient {
    client: reqwest::Client,
}

impl PyPIClient {
    /// Create a new PyPI client.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(format!("kitchen/{} (roast-lang)", crate::VERSION))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
    
    /// Get package metadata from PyPI.
    pub async fn get_package(&self, name: &str) -> Result<PyPIPackage> {
        let url = format!("{}/{}/json", PYPI_API_URL, name);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound(format!("Python package '{}' not found on PyPI", name)));
        }
        
        if !response.status().is_success() {
            return Err(Error::Http(format!(
                "Failed to fetch package '{}': {}",
                name,
                response.status()
            )));
        }
        
        let package: PyPIPackage = response.json()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        Ok(package)
    }
    
    /// Search for packages on PyPI.
    pub async fn search(&self, query: &str) -> Result<Vec<PyPISearchResult>> {
        // PyPI's XML-RPC search API is deprecated
        // Use simple API or warehouse API for search
        // For now, return empty - real search requires different approach
        Ok(vec![])
    }
    
    /// Get latest version of a package.
    pub async fn get_latest_version(&self, name: &str) -> Result<String> {
        let package = self.get_package(name).await?;
        Ok(package.info.version)
    }
    
    /// Check if a package exists.
    pub async fn package_exists(&self, name: &str) -> bool {
        self.get_package(name).await.is_ok()
    }
}

impl Default for PyPIClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct PyPISearchResult {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
}

/// Python environment manager for managing Python packages within a Roast project.
pub struct PythonEnv {
    /// Path to the project root.
    project_root: PathBuf,
    /// Path to the Python venv.
    venv_path: PathBuf,
    /// Python executable path.
    python_path: PathBuf,
    /// Pip executable path.
    pip_path: PathBuf,
}

impl PythonEnv {
    /// Create a Python environment for a project.
    pub fn new(project_root: &Path) -> Self {
        let venv_path = project_root.join(".roast").join("python");
        
        let (python_path, pip_path) = if cfg!(windows) {
            (
                venv_path.join("Scripts").join("python.exe"),
                venv_path.join("Scripts").join("pip.exe"),
            )
        } else {
            (
                venv_path.join("bin").join("python"),
                venv_path.join("bin").join("pip"),
            )
        };
        
        Self {
            project_root: project_root.to_path_buf(),
            venv_path,
            python_path,
            pip_path,
        }
    }
    
    /// Check if the Python environment exists.
    pub fn exists(&self) -> bool {
        self.python_path.exists()
    }
    
    /// Create the Python environment.
    pub fn create(&self) -> Result<()> {
        if self.exists() {
            return Ok(());
        }
        
        // Create parent directory
        if let Some(parent) = self.venv_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Find system Python
        let python = find_system_python()?;
        
        // Create venv
        let status = Command::new(&python)
            .args(["-m", "venv", &self.venv_path.to_string_lossy()])
            .status()
            .map_err(|e| Error::Io(e))?;
        
        if !status.success() {
            return Err(Error::Python(format!(
                "Failed to create Python venv: exit code {:?}",
                status.code()
            )));
        }
        
        // Upgrade pip
        self.run_pip(&["install", "--upgrade", "pip"])?;
        
        Ok(())
    }
    
    /// Install a package.
    pub fn install(&self, package: &PythonPackage) -> Result<()> {
        // Ensure venv exists
        self.create()?;
        
        let spec = package.pip_spec();
        
        self.run_pip(&["install", &spec])
    }
    
    /// Install multiple packages.
    pub fn install_many(&self, packages: &[PythonPackage]) -> Result<()> {
        self.create()?;
        
        if packages.is_empty() {
            return Ok(());
        }
        
        let specs: Vec<String> = packages.iter()
            .map(|p| p.pip_spec())
            .collect();
        
        let mut args = vec!["install"];
        for spec in &specs {
            args.push(spec);
        }
        
        self.run_pip(&args)
    }
    
    /// Install from requirements file.
    pub fn install_requirements(&self, requirements_file: &Path) -> Result<()> {
        self.create()?;
        
        self.run_pip(&["install", "-r", &requirements_file.to_string_lossy()])
    }
    
    /// Uninstall a package.
    pub fn uninstall(&self, package: &str) -> Result<()> {
        self.run_pip(&["uninstall", "-y", package])
    }
    
    /// List installed packages.
    pub fn list(&self) -> Result<Vec<InstalledPythonPackage>> {
        if !self.exists() {
            return Ok(vec![]);
        }
        
        let output = Command::new(&self.pip_path)
            .args(["list", "--format=json"])
            .output()
            .map_err(|e| Error::Io(e))?;
        
        if !output.status.success() {
            return Err(Error::Python("Failed to list packages".to_string()));
        }
        
        let packages: Vec<InstalledPythonPackage> = serde_json::from_slice(&output.stdout)
            .map_err(|e| Error::Python(e.to_string()))?;
        
        Ok(packages)
    }
    
    /// Check for outdated packages.
    pub fn outdated(&self) -> Result<Vec<OutdatedPythonPackage>> {
        if !self.exists() {
            return Ok(vec![]);
        }
        
        let output = Command::new(&self.pip_path)
            .args(["list", "--outdated", "--format=json"])
            .output()
            .map_err(|e| Error::Io(e))?;
        
        if !output.status.success() {
            return Err(Error::Python("Failed to check outdated packages".to_string()));
        }
        
        let packages: Vec<OutdatedPythonPackage> = serde_json::from_slice(&output.stdout)
            .map_err(|e| Error::Python(e.to_string()))?;
        
        Ok(packages)
    }
    
    /// Update a package.
    pub fn update(&self, package: &str) -> Result<()> {
        self.run_pip(&["install", "--upgrade", package])
    }
    
    /// Update all packages.
    pub fn update_all(&self) -> Result<()> {
        let outdated = self.outdated()?;
        
        for pkg in outdated {
            self.update(&pkg.name)?;
        }
        
        Ok(())
    }
    
    /// Freeze current packages to requirements format.
    pub fn freeze(&self) -> Result<String> {
        if !self.exists() {
            return Ok(String::new());
        }
        
        let output = Command::new(&self.pip_path)
            .arg("freeze")
            .output()
            .map_err(|e| Error::Io(e))?;
        
        if !output.status.success() {
            return Err(Error::Python("Failed to freeze packages".to_string()));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Run pip with arguments.
    fn run_pip(&self, args: &[&str]) -> Result<()> {
        let status = Command::new(&self.pip_path)
            .args(args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| Error::Io(e))?;
        
        if !status.success() {
            return Err(Error::Python(format!(
                "pip {} failed with exit code {:?}",
                args.join(" "),
                status.code()
            )));
        }
        
        Ok(())
    }
    
    /// Get Python site-packages path.
    pub fn site_packages(&self) -> Result<PathBuf> {
        let output = Command::new(&self.python_path)
            .args(["-c", "import site; print(site.getsitepackages()[0])"])
            .output()
            .map_err(|e| Error::Io(e))?;
        
        if !output.status.success() {
            return Err(Error::Python("Failed to get site-packages path".to_string()));
        }
        
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPythonPackage {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedPythonPackage {
    pub name: String,
    pub version: String,
    pub latest_version: String,
    pub latest_filetype: String,
}

/// Find system Python executable.
pub fn find_system_python() -> Result<PathBuf> {
    // Try various Python paths
    let candidates = if cfg!(windows) {
        vec!["python", "python3", "py -3"]
    } else {
        vec!["python3", "python"]
    };
    
    for candidate in candidates {
        let parts: Vec<&str> = candidate.split_whitespace().collect();
        
        let output = Command::new(parts[0])
            .args(&parts[1..])
            .args(["--version"])
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                // Check if it's Python 3.x
                if version.contains("Python 3") {
                    if parts.len() > 1 {
                        // Handle "py -3" case
                        return Ok(PathBuf::from(parts[0]));
                    }
                    
                    // Find full path
                    if let Ok(path) = which::which(parts[0]) {
                        return Ok(path);
                    }
                    return Ok(PathBuf::from(parts[0]));
                }
            }
        }
    }
    
    Err(Error::Python(
        "Python 3 not found. Please install Python 3.8 or later.".to_string()
    ))
}

/// Check if a package spec is for a Python package (has py: prefix).
pub fn is_python_package(spec: &str) -> bool {
    spec.starts_with("py:") || spec.starts_with("python:")
}

/// Extract package name from py: prefixed spec.
pub fn strip_python_prefix(spec: &str) -> &str {
    if spec.starts_with("py:") {
        &spec[3..]
    } else if spec.starts_with("python:") {
        &spec[7..]
    } else {
        spec
    }
}

/// Python package lock entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonLockEntry {
    pub name: String,
    pub version: String,
    pub hash: Option<String>,
    pub requires: Vec<String>,
}

/// Save Python packages to a lock section.
pub fn save_python_lock(project_root: &Path, packages: &[InstalledPythonPackage]) -> Result<()> {
    let lock_path = project_root.join(".roast").join("python.lock");
    
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let entries: Vec<PythonLockEntry> = packages.iter()
        .map(|p| PythonLockEntry {
            name: p.name.clone(),
            version: p.version.clone(),
            hash: None,
            requires: vec![],
        })
        .collect();
    
    let content = toml::to_string_pretty(&PythonLock { packages: entries })
        .map_err(|e| Error::InvalidConfig(e.to_string()))?;
    
    fs::write(lock_path, content)?;
    
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PythonLock {
    packages: Vec<PythonLockEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_python_package() {
        let pkg = PythonPackage::parse("requests");
        assert_eq!(pkg.name, "requests");
        assert_eq!(pkg.version, None);
        
        let pkg = PythonPackage::parse("requests==2.28.0");
        assert_eq!(pkg.name, "requests");
        assert_eq!(pkg.version, Some("==2.28.0".to_string()));
        
        let pkg = PythonPackage::parse("pandas>=2.0,<3.0");
        assert_eq!(pkg.name, "pandas");
        assert_eq!(pkg.version, Some(">=2.0,<3.0".to_string()));
    }
    
    #[test]
    fn test_is_python_package() {
        assert!(is_python_package("py:requests"));
        assert!(is_python_package("python:numpy"));
        assert!(!is_python_package("requests"));
        assert!(!is_python_package("roast-http"));
    }
    
    #[test]
    fn test_strip_python_prefix() {
        assert_eq!(strip_python_prefix("py:requests"), "requests");
        assert_eq!(strip_python_prefix("python:numpy"), "numpy");
        assert_eq!(strip_python_prefix("requests"), "requests");
    }
}


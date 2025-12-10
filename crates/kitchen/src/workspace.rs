//! Workspace (mono-repo) support.

use std::path::{Path, PathBuf};
use std::fs;
// use glob::Pattern;
use crate::config::WorkspaceConfig;
use crate::project::Project;
use crate::{Error, Result};

/// A workspace containing multiple packages.
pub struct Workspace {
    /// Workspace root directory.
    pub root: PathBuf,
    
    /// Workspace configuration.
    pub config: WorkspaceConfig,
    
    /// Member packages.
    pub members: Vec<WorkspaceMember>,
}

/// A workspace member.
pub struct WorkspaceMember {
    /// Member name.
    pub name: String,
    
    /// Member path.
    pub path: PathBuf,
    
    /// Is default member.
    pub is_default: bool,
}

impl Workspace {
    /// Load a workspace.
    pub fn load(root: &Path) -> Result<Self> {
        let config_path = root.join("roast.toml");
        
        if !config_path.exists() {
            return Err(Error::NotFound("roast.toml not found".to_string()));
        }
        
        let project_config = crate::config::ProjectConfig::load(&config_path)?;
        
        let workspace_config = project_config.workspace
            .ok_or_else(|| Error::InvalidConfig(
                "Not a workspace: missing [workspace] section".to_string()
            ))?;
        
        let members = Self::discover_members(root, &workspace_config)?;
        
        Ok(Self {
            root: root.to_path_buf(),
            config: workspace_config,
            members,
        })
    }
    
    /// Discover workspace members.
    fn discover_members(root: &Path, config: &WorkspaceConfig) -> Result<Vec<WorkspaceMember>> {
        let mut members = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        for member_glob in &config.members {
            for path in glob_paths(root, member_glob)? {
                let config_path = path.join("roast.toml");
                
                if !config_path.exists() {
                    continue;
                }
                
                // Check not excluded
                let relative = path.strip_prefix(root).unwrap_or(&path);
                let relative_str = relative.to_string_lossy();
                
                let excluded = config.exclude.iter().any(|pattern| {
                    relative_str.contains(pattern)
                });
                
                if excluded {
                    continue;
                }
                
                if seen.contains(&path) {
                    continue;
                }
                seen.insert(path.clone());
                
                // Load member config
                let member_config = crate::config::ProjectConfig::load(&config_path)?;
                
                let is_default = config.default_members.is_empty()
                    || config.default_members.contains(&relative_str.to_string());
                
                members.push(WorkspaceMember {
                    name: member_config.package.name,
                    path,
                    is_default,
                });
            }
        }
        
        Ok(members)
    }
    
    /// Get all member projects.
    pub fn projects(&self) -> Result<Vec<Project>> {
        self.members.iter()
            .map(|m| Project::load(&m.path))
            .collect()
    }
    
    /// Get default member projects.
    pub fn default_projects(&self) -> Result<Vec<Project>> {
        self.members.iter()
            .filter(|m| m.is_default)
            .map(|m| Project::load(&m.path))
            .collect()
    }
    
    /// Find a member by name.
    pub fn find_member(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.name == name)
    }
    
    /// Check if this is a virtual workspace (no root package).
    pub fn is_virtual(&self) -> bool {
        !self.root.join("src").exists()
    }
}

/// Expand glob patterns to paths.
fn glob_paths(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let full_pattern = root.join(pattern);
    
    // Simple glob expansion - handles basic patterns like "crates/*"
    if pattern.contains('*') {
        let parent = full_pattern.parent().unwrap_or(root);
        let mut paths = Vec::new();
        
        if parent.exists() {
            for entry in fs::read_dir(parent)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    paths.push(path);
                }
            }
        }
        
        Ok(paths)
    } else {
        Ok(vec![full_pattern])
    }
}

/// Create a new workspace.
pub fn create_workspace(path: &Path, members: &[&str]) -> Result<Workspace> {
    fs::create_dir_all(path)?;
    
    let config = crate::config::ProjectConfig {
        package: crate::config::PackageConfig {
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("workspace")
                .to_string(),
            version: "0.1.0".to_string(),
            edition: "2024".to_string(),
            authors: vec![],
            description: Some("A Roast workspace".to_string()),
            license: None,
            repository: None,
            homepage: None,
            documentation: None,
            keywords: vec![],
            categories: vec![],
            readme: None,
            entry: "".to_string(),
            roast_version: None,
            include: vec![],
            exclude: vec![],
        },
        dependencies: std::collections::HashMap::new(),
        dev_dependencies: std::collections::HashMap::new(),
        build_dependencies: std::collections::HashMap::new(),
        optional_dependencies: std::collections::HashMap::new(),
        features: std::collections::HashMap::new(),
        default_features: vec![],
        scripts: std::collections::HashMap::new(),
        build: crate::config::BuildConfig::default(),
        workspace: Some(WorkspaceConfig {
            members: members.iter().map(|s| s.to_string()).collect(),
            exclude: vec![],
            default_members: vec![],
            dependencies: std::collections::HashMap::new(),
        }),
        profile: crate::config::ProfilesConfig::default(),
    };
    
    config.save(&path.join("roast.toml"))?;
    
    Workspace::load(path)
}

/// Add a member to a workspace.
pub fn add_workspace_member(workspace_root: &Path, member_path: &str) -> Result<()> {
    let config_path = workspace_root.join("roast.toml");
    let mut config = crate::config::ProjectConfig::load(&config_path)?;
    
    let workspace = config.workspace.as_mut()
        .ok_or_else(|| Error::InvalidConfig("Not a workspace".to_string()))?;
    
    if !workspace.members.contains(&member_path.to_string()) {
        workspace.members.push(member_path.to_string());
    }
    
    config.save(&config_path)?;
    
    Ok(())
}


//! Dependency management.

use std::collections::HashMap;
use std::path::PathBuf;
// use std::fs;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use crate::config::DependencyConfig;
use crate::{Error, Result};

/// A dependency specification.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name.
    pub name: String,
    
    /// Dependency specification.
    pub spec: DependencySpec,
    
    /// Is optional.
    pub optional: bool,
    
    /// Features to enable.
    pub features: Vec<String>,
    
    /// Use default features.
    pub default_features: bool,
}

/// Dependency source specification.
#[derive(Debug, Clone)]
pub enum DependencySpec {
    /// From registry with version.
    Registry {
        version: VersionReq,
        registry: Option<String>,
    },
    
    /// From git repository.
    Git {
        url: String,
        branch: Option<String>,
        tag: Option<String>,
        rev: Option<String>,
    },
    
    /// From local path.
    Path {
        path: PathBuf,
    },
}

impl Dependency {
    /// Parse from configuration.
    pub fn from_config(name: &str, config: &DependencyConfig) -> Result<Self> {
        match config {
            DependencyConfig::Simple(version) => {
                let version_req = VersionReq::parse(version)
                    .map_err(|e| Error::Dependency(format!("Invalid version '{}': {}", version, e)))?;
                
                Ok(Self {
                    name: name.to_string(),
                    spec: DependencySpec::Registry {
                        version: version_req,
                        registry: None,
                    },
                    optional: false,
                    features: vec![],
                    default_features: true,
                })
            }
            DependencyConfig::Detailed(detailed) => {
                let spec = if let Some(ref path) = detailed.path {
                    DependencySpec::Path {
                        path: PathBuf::from(path),
                    }
                } else if let Some(ref git) = detailed.git {
                    DependencySpec::Git {
                        url: git.clone(),
                        branch: detailed.branch.clone(),
                        tag: detailed.tag.clone(),
                        rev: detailed.rev.clone(),
                    }
                } else {
                    let version = detailed.version.as_deref().unwrap_or("*");
                    let version_req = VersionReq::parse(version)
                        .map_err(|e| Error::Dependency(format!("Invalid version '{}': {}", version, e)))?;
                    
                    DependencySpec::Registry {
                        version: version_req,
                        registry: detailed.registry.clone(),
                    }
                };
                
                Ok(Self {
                    name: name.to_string(),
                    spec,
                    optional: detailed.optional,
                    features: detailed.features.clone(),
                    default_features: detailed.default_features,
                })
            }
        }
    }
    
    /// Check if version matches.
    pub fn matches_version(&self, version: &Version) -> bool {
        match &self.spec {
            DependencySpec::Registry { version: req, .. } => req.matches(version),
            _ => true, // Git and path always match
        }
    }
}

/// A resolved dependency with exact version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedDependency {
    /// Package name.
    pub name: String,
    
    /// Exact version.
    pub version: String,
    
    /// Source type.
    pub source: ResolvedSource,
    
    /// Checksum.
    pub checksum: Option<String>,
    
    /// Dependencies of this package.
    pub dependencies: Vec<String>,
    
    /// Enabled features.
    pub features: Vec<String>,
}

/// Resolved source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ResolvedSource {
    Registry {
        registry: String,
    },
    Git {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
    },
    Path {
        path: String,
    },
}

/// Dependency graph.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// All resolved dependencies.
    pub packages: HashMap<String, ResolvedDependency>,
    
    /// Root dependencies.
    pub roots: Vec<String>,
}

impl DependencyGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            roots: Vec::new(),
        }
    }
    
    /// Add a resolved dependency.
    pub fn add(&mut self, dep: ResolvedDependency) {
        self.packages.insert(dep.name.clone(), dep);
    }
    
    /// Get a dependency by name.
    pub fn get(&self, name: &str) -> Option<&ResolvedDependency> {
        self.packages.get(name)
    }
    
    /// Get installation order (topologically sorted).
    pub fn installation_order(&self) -> Vec<&ResolvedDependency> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        
        for root in &self.roots {
            self.visit_deps(root, &mut visited, &mut order);
        }
        
        order
    }
    
    fn visit_deps<'a>(
        &'a self,
        name: &str,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<&'a ResolvedDependency>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        
        if let Some(dep) = self.packages.get(name) {
            // Visit dependencies first
            for dep_name in &dep.dependencies {
                self.visit_deps(dep_name, visited, order);
            }
            order.push(dep);
        }
    }
    
    /// Check for conflicts.
    pub fn check_conflicts(&self) -> Vec<DependencyConflict> {
        // In a simple implementation, we check if any package
        // appears with different versions
        // (Real implementation would be more sophisticated)
        Vec::new()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A dependency conflict.
#[derive(Debug, Clone)]
pub struct DependencyConflict {
    /// Package name.
    pub package: String,
    
    /// Conflicting requirements.
    pub requirements: Vec<(String, String)>,
}

/// Package metadata from registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Package name.
    pub name: String,
    
    /// Available versions.
    pub versions: Vec<PackageVersion>,
    
    /// Description.
    pub description: Option<String>,
    
    /// Repository.
    pub repository: Option<String>,
    
    /// License.
    pub license: Option<String>,
    
    /// Keywords.
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Package version metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVersion {
    /// Version string.
    pub version: String,
    
    /// Minimum Roast version.
    pub roast_version: Option<String>,
    
    /// Dependencies.
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    
    /// Features.
    #[serde(default)]
    pub features: HashMap<String, Vec<String>>,
    
    /// Default features.
    #[serde(default)]
    pub default_features: Vec<String>,
    
    /// Download URL.
    pub download_url: Option<String>,
    
    /// Checksum (SHA256).
    pub checksum: Option<String>,
    
    /// Publication date.
    pub published_at: Option<String>,
    
    /// Is yanked.
    #[serde(default)]
    pub yanked: bool,
}

/// Parse dependency requirements from a simple string.
pub fn parse_requirement(spec: &str) -> Result<(String, VersionReq)> {
    // Parse "package>=1.0" or "package==1.0" or "package" (any version)
    let spec = spec.trim();
    
    // Find operator position
    let ops = [">=", "<=", "==", "!=", ">", "<", "~=", "^"];
    
    for op in &ops {
        if let Some(pos) = spec.find(op) {
            let name = spec[..pos].trim().to_string();
            let version = &spec[pos..];
            
            // Convert Python-style to semver
            let semver_version = convert_version_spec(version)?;
            let req = VersionReq::parse(&semver_version)?;
            
            return Ok((name, req));
        }
    }
    
    // No operator, any version
    Ok((spec.to_string(), VersionReq::STAR))
}

fn convert_version_spec(spec: &str) -> Result<String> {
    let spec = spec.trim();
    
    if spec.starts_with("==") {
        // Exact version
        Ok(format!("={}", &spec[2..]))
    } else if spec.starts_with("~=") {
        // Compatible release (like ^)
        Ok(format!("^{}", &spec[2..]))
    } else {
        Ok(spec.to_string())
    }
}


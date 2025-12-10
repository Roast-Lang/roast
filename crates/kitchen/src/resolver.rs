//! Dependency resolution (SAT solver).

use std::collections::{HashMap, HashSet, VecDeque};
use semver::{Version, VersionReq};
use crate::deps::{Dependency, DependencyGraph, ResolvedDependency, ResolvedSource, PackageMetadata};
use crate::registry::Registry;
use crate::{Error, Result};

/// Dependency resolver.
pub struct Resolver {
    /// Package registry.
    registry: Registry,
    
    /// Resolution cache.
    cache: HashMap<String, PackageMetadata>,
    
    /// Preferred versions (from lockfile).
    preferred: HashMap<String, Version>,
}

impl Resolver {
    /// Create a new resolver.
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            cache: HashMap::new(),
            preferred: HashMap::new(),
        }
    }
    
    /// Set preferred versions from lockfile.
    pub fn with_preferred(mut self, preferred: HashMap<String, Version>) -> Self {
        self.preferred = preferred;
        self
    }
    
    /// Resolve dependencies.
    pub async fn resolve(&mut self, dependencies: &[Dependency]) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph::new();
        let mut pending: VecDeque<Dependency> = dependencies.iter().cloned().collect();
        let mut resolved: HashSet<String> = HashSet::new();
        
        // Add root dependencies
        for dep in dependencies {
            graph.roots.push(dep.name.clone());
        }
        
        while let Some(dep) = pending.pop_front() {
            if resolved.contains(&dep.name) {
                continue;
            }
            
            // Resolve this dependency
            let resolved_dep = self.resolve_one(&dep).await?;
            
            // Add transitive dependencies
            for trans_dep_name in &resolved_dep.dependencies {
                if !resolved.contains(trans_dep_name) {
                    // Fetch metadata and create dependency
                    let metadata = self.fetch_metadata(trans_dep_name).await?;
                    
                    // Find the version we resolved to
                    if let Some(version_info) = metadata.versions.first() {
                        for (name, version_req) in &version_info.dependencies {
                            let trans_dep = Dependency {
                                name: name.clone(),
                                spec: crate::deps::DependencySpec::Registry {
                                    version: VersionReq::parse(version_req)
                                        .unwrap_or(VersionReq::STAR),
                                    registry: None,
                                },
                                optional: false,
                                features: vec![],
                                default_features: true,
                            };
                            pending.push_back(trans_dep);
                        }
                    }
                }
            }
            
            resolved.insert(dep.name.clone());
            graph.add(resolved_dep);
        }
        
        // Check for conflicts
        let conflicts = graph.check_conflicts();
        if !conflicts.is_empty() {
            return Err(Error::Resolution(format!(
                "Dependency conflict: {}",
                conflicts.iter()
                    .map(|c| c.package.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        
        Ok(graph)
    }
    
    /// Resolve a single dependency.
    async fn resolve_one(&mut self, dep: &Dependency) -> Result<ResolvedDependency> {
        match &dep.spec {
            crate::deps::DependencySpec::Registry { version, registry } => {
                self.resolve_registry(dep, version, registry.as_deref()).await
            }
            crate::deps::DependencySpec::Git { url, branch, tag, rev } => {
                self.resolve_git(dep, url, branch.as_deref(), tag.as_deref(), rev.as_deref())
            }
            crate::deps::DependencySpec::Path { path } => {
                self.resolve_path(dep, path)
            }
        }
    }
    
    async fn resolve_registry(
        &mut self,
        dep: &Dependency,
        version_req: &VersionReq,
        registry: Option<&str>,
    ) -> Result<ResolvedDependency> {
        let metadata = self.fetch_metadata(&dep.name).await?;
        
        // Find best matching version
        let version = self.find_best_version(&metadata, version_req)?;
        
        // Get version details
        let version_info = metadata.versions.iter()
            .find(|v| v.version == version.to_string())
            .ok_or_else(|| Error::Resolution(format!(
                "Version {} not found for {}", version, dep.name
            )))?;
        
        Ok(ResolvedDependency {
            name: dep.name.clone(),
            version: version.to_string(),
            source: ResolvedSource::Registry {
                registry: registry.unwrap_or(crate::DEFAULT_REGISTRY).to_string(),
            },
            checksum: version_info.checksum.clone(),
            dependencies: version_info.dependencies.keys().cloned().collect(),
            features: dep.features.clone(),
        })
    }
    
    fn resolve_git(
        &self,
        dep: &Dependency,
        url: &str,
        branch: Option<&str>,
        tag: Option<&str>,
        rev: Option<&str>,
    ) -> Result<ResolvedDependency> {
        // For git dependencies, we use the rev/tag/branch as version
        let version = rev.or(tag).or(branch).unwrap_or("HEAD").to_string();
        
        Ok(ResolvedDependency {
            name: dep.name.clone(),
            version,
            source: ResolvedSource::Git {
                url: url.to_string(),
                branch: branch.map(String::from),
                rev: rev.map(String::from),
            },
            checksum: None,
            dependencies: vec![], // Would need to fetch and parse
            features: dep.features.clone(),
        })
    }
    
    fn resolve_path(
        &self,
        dep: &Dependency,
        path: &std::path::Path,
    ) -> Result<ResolvedDependency> {
        // For path dependencies, read version from roast.toml
        let config_path = path.join("roast.toml");
        if !config_path.exists() {
            return Err(Error::NotFound(format!(
                "roast.toml not found in {}", path.display()
            )));
        }
        
        let config = crate::config::ProjectConfig::load(&config_path)?;
        
        Ok(ResolvedDependency {
            name: dep.name.clone(),
            version: config.package.version,
            source: ResolvedSource::Path {
                path: path.display().to_string(),
            },
            checksum: None,
            dependencies: config.dependencies.keys().cloned().collect(),
            features: dep.features.clone(),
        })
    }
    
    fn find_best_version(
        &self,
        metadata: &PackageMetadata,
        req: &VersionReq,
    ) -> Result<Version> {
        let mut candidates: Vec<Version> = metadata.versions.iter()
            .filter(|v| !v.yanked)
            .filter_map(|v| Version::parse(&v.version).ok())
            .filter(|v| req.matches(v))
            .collect();
        
        if candidates.is_empty() {
            return Err(Error::Resolution(format!(
                "No version of {} matches requirement {}", 
                metadata.name, req
            )));
        }
        
        // Sort by version (descending)
        candidates.sort_by(|a, b| b.cmp(a));
        
        // Prefer locked version if available
        if let Some(preferred) = self.preferred.get(&metadata.name) {
            if candidates.contains(preferred) {
                return Ok(preferred.clone());
            }
        }
        
        // Return highest matching version
        Ok(candidates.into_iter().next().unwrap())
    }
    
    async fn fetch_metadata(&mut self, name: &str) -> Result<PackageMetadata> {
        // Check cache
        if let Some(metadata) = self.cache.get(name) {
            return Ok(metadata.clone());
        }
        
        // Fetch from registry
        let metadata = self.registry.get_package(name).await?;
        
        // Cache it
        self.cache.insert(name.to_string(), metadata.clone());
        
        Ok(metadata)
    }
}

/// Resolution options.
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    /// Use development dependencies.
    pub dev: bool,
    
    /// Only use locked versions.
    pub locked: bool,
    
    /// Update all dependencies.
    pub update: bool,
    
    /// Specific packages to update.
    pub update_packages: Vec<String>,
    
    /// Allow pre-release versions.
    pub allow_prerelease: bool,
    
    /// Offline mode.
    pub offline: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_matching() {
        let req = VersionReq::parse("^1.0").unwrap();
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.5.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();
        
        assert!(req.matches(&v1));
        assert!(req.matches(&v2));
        assert!(!req.matches(&v3));
    }
}


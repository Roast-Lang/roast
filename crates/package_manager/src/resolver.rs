//! Dependency resolver.

use crate::config::{Dependency, PackageConfig};
use indexmap::IndexMap;
use petgraph::graph::DiGraph;
use std::collections::HashSet;

/// Resolved dependency.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub version: String,
    pub source: DepSource,
}

/// Dependency source.
#[derive(Debug, Clone)]
pub enum DepSource {
    Registry { url: String },
    Git { url: String, rev: String },
    Path { path: String },
}

/// Dependency resolver.
pub struct Resolver {
    graph: DiGraph<String, ()>,
    resolved: IndexMap<String, ResolvedDep>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            resolved: IndexMap::new(),
        }
    }

    /// Resolves dependencies for a package.
    pub fn resolve(&mut self, config: &PackageConfig) -> Result<Vec<ResolvedDep>, ResolveError> {
        let mut to_resolve: Vec<(String, &Dependency)> = config
            .dependencies
            .iter()
            .map(|(k, v)| (k.clone(), v))
            .collect();

        let mut visited = HashSet::new();

        while let Some((name, dep)) = to_resolve.pop() {
            if visited.contains(&name) {
                continue;
            }
            visited.insert(name.clone());

            let resolved = self.resolve_single(&name, dep)?;
            self.resolved.insert(name.clone(), resolved);

            // Would recursively resolve transitive dependencies here
        }

        Ok(self.resolved.values().cloned().collect())
    }

    fn resolve_single(&self, name: &str, dep: &Dependency) -> Result<ResolvedDep, ResolveError> {
        match dep {
            Dependency::Simple(version) => Ok(ResolvedDep {
                name: name.to_string(),
                version: version.clone(),
                source: DepSource::Registry {
                    url: "https://registry.roastlang.wiki".to_string(),
                },
            }),
            Dependency::Detailed(details) => {
                let source = if let Some(path) = &details.path {
                    DepSource::Path { path: path.clone() }
                } else if let Some(git) = &details.git {
                    DepSource::Git {
                        url: git.clone(),
                        rev: details.rev.clone().unwrap_or_else(|| "HEAD".to_string()),
                    }
                } else {
                    DepSource::Registry {
                        url: "https://registry.roastlang.wiki".to_string(),
                    }
                };

                Ok(ResolvedDep {
                    name: name.to_string(),
                    version: details.version.clone().unwrap_or_else(|| "*".to_string()),
                    source,
                })
            }
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolution error.
#[derive(Debug)]
pub enum ResolveError {
    NotFound(String),
    VersionConflict { package: String, versions: Vec<String> },
    CyclicDependency(Vec<String>),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotFound(pkg) => write!(f, "package not found: {}", pkg),
            ResolveError::VersionConflict { package, versions } => {
                write!(f, "version conflict for {}: {:?}", package, versions)
            }
            ResolveError::CyclicDependency(cycle) => {
                write!(f, "cyclic dependency: {:?}", cycle)
            }
        }
    }
}

impl std::error::Error for ResolveError {}


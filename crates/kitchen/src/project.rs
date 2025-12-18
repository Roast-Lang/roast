//! Project management.

use std::path::{Path, PathBuf};
use std::fs;
use crate::config::{ProjectConfig, PackageConfig, BuildConfig, ProfilesConfig};
use crate::{Error, Result};

/// A Roast project.
pub struct Project {
    /// Project root directory.
    pub root: PathBuf,

    /// Project configuration.
    pub config: ProjectConfig,

    /// Is this a workspace member?
    pub is_member: bool,
}

impl Project {
    /// Create a new project.
    pub fn new(name: &str, path: &Path, template: ProjectTemplate) -> Result<Self> {
        if path.exists() {
            return Err(Error::AlreadyExists(format!("{} already exists", path.display())));
        }

        fs::create_dir_all(path)?;

        // Create directory structure
        fs::create_dir_all(path.join("src"))?;
        fs::create_dir_all(path.join("tests"))?;
        fs::create_dir_all(path.join("docs"))?;

        // Create configuration
        let config = template.generate_config(name);
        let config_path = path.join("roast.toml");
        config.save(&config_path)?;

        // Create source files
        let main_content = template.generate_main(name);
        fs::write(path.join("src/main.roast"), main_content)?;

        // Create test file
        let test_content = template.generate_test(name);
        fs::write(path.join("tests/test_main.roast"), test_content)?;

        // Create README
        let readme = template.generate_readme(name);
        fs::write(path.join("README.md"), readme)?;

        // Create .gitignore
        let gitignore = template.generate_gitignore();
        fs::write(path.join(".gitignore"), gitignore)?;

        Ok(Self {
            root: path.to_path_buf(),
            config,
            is_member: false,
        })
    }

    /// Initialize a project in existing directory.
    pub fn init(path: &Path, name: Option<&str>) -> Result<Self> {
        let config_path = path.join("roast.toml");

        if config_path.exists() {
            return Err(Error::AlreadyExists("roast.toml already exists".to_string()));
        }

        let project_name = name.unwrap_or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("my_project")
        });

        // Create src directory if it doesn't exist
        let src_dir = path.join("src");
        if !src_dir.exists() {
            fs::create_dir_all(&src_dir)?;

            // Create default main.roast
            let main_content = format!(
                r#"# {} - Main entry point

def main() -> None:
    print("Hello from {}!")

if __name__ == "__main__":
    main()
"#,
                project_name, project_name
            );
            fs::write(src_dir.join("main.roast"), main_content)?;
        }

        // Create configuration
        let config = ProjectTemplate::Binary.generate_config(project_name);
        config.save(&config_path)?;

        Ok(Self {
            root: path.to_path_buf(),
            config,
            is_member: false,
        })
    }

    /// Load an existing project.
    pub fn load(path: &Path) -> Result<Self> {
        let config_path = path.join("roast.toml");

        if !config_path.exists() {
            return Err(Error::NotFound("roast.toml not found".to_string()));
        }

        let config = ProjectConfig::load(&config_path)?;

        Ok(Self {
            root: path.to_path_buf(),
            config,
            is_member: false,
        })
    }

    /// Find and load the current project.
    pub fn find_current() -> Result<Self> {
        let config_path = ProjectConfig::find()?;
        let root = config_path.parent().unwrap().to_path_buf();
        let config = ProjectConfig::load(&config_path)?;

        Ok(Self {
            root,
            config,
            is_member: false,
        })
    }

    /// Get project name.
    pub fn name(&self) -> &str {
        &self.config.package.name
    }

    /// Get project version.
    pub fn version(&self) -> &str {
        &self.config.package.version
    }

    /// Get source directory.
    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    /// Get test directory.
    pub fn test_dir(&self) -> PathBuf {
        self.root.join("tests")
    }

    /// Get cooked (build output) directory - Roast's equivalent of Rust's "target".
    pub fn cooked_dir(&self) -> PathBuf {
        self.root.join(&self.config.build.target_dir)
    }

    /// Alias for backwards compatibility (deprecated, use cooked_dir()).
    #[deprecated(note = "Use cooked_dir() instead")]
    pub fn target_dir(&self) -> PathBuf {
        self.cooked_dir()
    }

    /// Get debug output directory (cooked/debug).
    pub fn debug_dir(&self) -> PathBuf {
        self.cooked_dir().join("debug")
    }

    /// Get release output directory (cooked/release).
    pub fn release_dir(&self) -> PathBuf {
        self.cooked_dir().join("release")
    }

    /// Get tmp directory for compilation artifacts (cooked/tmp).
    pub fn tmp_dir(&self) -> PathBuf {
        self.cooked_dir().join("tmp")
    }

    /// Initialize the cooked directory structure.
    pub fn init_cooked_dir(&self) -> Result<()> {
        let cooked = self.cooked_dir();

        // Create directory structure
        fs::create_dir_all(cooked.join("debug"))?;
        fs::create_dir_all(cooked.join("release"))?;
        fs::create_dir_all(cooked.join("tmp"))?;

        // Create CACHEDIR.TAG (standard cache directory marker)
        let cachedir_tag = cooked.join("CACHEDIR.TAG");
        if !cachedir_tag.exists() {
            fs::write(&cachedir_tag,
                "Signature: 8a477f597d28d172789f06886806bc55\n\
                 # This file is a cache directory tag created by Roast.\n\
                 # For information about cache directory tags, see:\n\
                 #   https://bford.info/cachedir/\n"
            )?;
        }

        // Create .roast_info.json with build metadata
        let info_path = cooked.join(".roast_info.json");
        let info = serde_json::json!({
            "project": self.name(),
            "version": self.version(),
            "roast_version": env!("CARGO_PKG_VERSION"),
            "created_at": chrono::Utc::now().to_rfc3339(),
            "build_dir": "cooked",
            "profiles": {
                "debug": {
                    "opt_level": 0,
                    "debug_info": true
                },
                "release": {
                    "opt_level": 3,
                    "debug_info": false
                }
            }
        });
        fs::write(&info_path, serde_json::to_string_pretty(&info).unwrap())?;

        Ok(())
    }

    /// Get entry point file.
    pub fn entry_point(&self) -> PathBuf {
        self.root.join(&self.config.package.entry)
    }

    /// Get all source files.
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        collect_roast_files(&self.src_dir(), &mut files)?;
        Ok(files)
    }

    /// Get all test files.
    pub fn test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let test_dir = self.test_dir();
        if test_dir.exists() {
            collect_roast_files(&test_dir, &mut files)?;
        }
        Ok(files)
    }

    /// Clean build artifacts.
    #[allow(deprecated)]
    pub fn clean(&self) -> Result<()> {
        let cooked = self.cooked_dir();
        if cooked.exists() {
            fs::remove_dir_all(&cooked)?;
        }
        Ok(())
    }

    /// Add a dependency.
    pub fn add_dependency(&mut self, name: &str, version: &str, dev: bool) -> Result<()> {
        let deps = if dev {
            &mut self.config.dev_dependencies
        } else {
            &mut self.config.dependencies
        };

        deps.insert(
            name.to_string(),
            crate::config::DependencyConfig::Simple(version.to_string()),
        );

        self.save_config()
    }

    /// Remove a dependency.
    pub fn remove_dependency(&mut self, name: &str) -> Result<()> {
        self.config.dependencies.remove(name);
        self.config.dev_dependencies.remove(name);
        self.save_config()
    }

    /// Save configuration.
    pub fn save_config(&self) -> Result<()> {
        let config_path = self.root.join("roast.toml");
        self.config.save(&config_path)
    }
}

fn collect_roast_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_roast_files(&path, files)?;
        } else if path.extension().map_or(false, |ext| ext == "roast" || ext == "ro" || ext == "🍗") {
            files.push(path);
        }
    }

    Ok(())
}

/// Project template.
#[derive(Debug, Clone, Copy)]
pub enum ProjectTemplate {
    /// Binary application.
    Binary,
    /// Library package.
    Library,
    /// Web application.
    Web,
    /// CLI application.
    Cli,
    /// GPU compute application.
    Gpu,
}

impl ProjectTemplate {
    /// Generate project configuration.
    pub fn generate_config(&self, name: &str) -> ProjectConfig {
        ProjectConfig {
            package: PackageConfig {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                edition: "2024".to_string(),
                authors: vec![],
                description: Some(format!("A Roast {}", self.description())),
                license: Some("MIT".to_string()),
                repository: None,
                homepage: None,
                documentation: None,
                keywords: vec![],
                categories: vec![],
                readme: Some("README.md".to_string()),
                entry: self.entry_point(),
                roast_version: Some(">=0.1.0".to_string()),
                include: vec![],
                exclude: vec![],
            },
            dependencies: self.default_dependencies(),
            dev_dependencies: self.dev_dependencies(),
            build_dependencies: Default::default(),
            optional_dependencies: Default::default(),
            features: Default::default(),
            default_features: vec![],
            scripts: self.default_scripts(),
            build: BuildConfig::default(),
            workspace: None,
            profile: ProfilesConfig::default(),
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Binary => "application",
            Self::Library => "library",
            Self::Web => "web application",
            Self::Cli => "CLI application",
            Self::Gpu => "GPU compute application",
        }
    }

    fn entry_point(&self) -> String {
        match self {
            Self::Library => "src/lib.roast".to_string(),
            _ => "src/main.roast".to_string(),
        }
    }

    fn default_dependencies(&self) -> std::collections::HashMap<String, crate::config::DependencyConfig> {
        let mut deps = std::collections::HashMap::new();

        match self {
            Self::Web => {
                deps.insert("roast-web".to_string(),
                    crate::config::DependencyConfig::Simple("0.1".to_string()));
            }
            Self::Cli => {
                deps.insert("roast-cli".to_string(),
                    crate::config::DependencyConfig::Simple("0.1".to_string()));
            }
            Self::Gpu => {
                deps.insert("roast-gpu".to_string(),
                    crate::config::DependencyConfig::Simple("0.1".to_string()));
            }
            _ => {}
        }

        deps
    }

    fn dev_dependencies(&self) -> std::collections::HashMap<String, crate::config::DependencyConfig> {
        let mut deps = std::collections::HashMap::new();
        deps.insert("roast-test".to_string(),
            crate::config::DependencyConfig::Simple("0.1".to_string()));
        deps
    }

    fn default_scripts(&self) -> std::collections::HashMap<String, String> {
        let mut scripts = std::collections::HashMap::new();
        scripts.insert("test".to_string(), "kitchen test".to_string());
        scripts.insert("lint".to_string(), "kitchen check".to_string());
        scripts.insert("format".to_string(), "kitchen fmt".to_string());
        scripts
    }

    /// Generate main source file.
    pub fn generate_main(&self, name: &str) -> String {
        match self {
            Self::Binary => format!(
                r#"# {name} - Main entry point

def main() -> None:
    """Main function."""
    print("Hello from {name}!")

if __name__ == "__main__":
    main()
"#
            ),
            Self::Library => format!(
                r#"# {name} - Library module

"""
{name} - A Roast library.

Example usage:
    from {name} import hello
    hello("World")
"""

def hello(name: str) -> None:
    """Say hello to someone."""
    print(f"Hello, {{name}}!")

def version() -> str:
    """Return the library version."""
    return "0.1.0"
"#
            ),
            Self::Web => format!(
                r#"# {name} - Web Application

from roast.web import App, Request, Response

app = App()

@app.route("/")
async def index(request: Request) -> Response:
    """Home page."""
    return Response.html("<h1>Welcome to {name}!</h1>")

@app.route("/api/hello")
async def api_hello(request: Request) -> Response:
    """API endpoint."""
    return Response.json({{"message": "Hello from {name}!"}})

def main() -> None:
    """Start the web server."""
    app.run(host="0.0.0.0", port=8000)

if __name__ == "__main__":
    main()
"#
            ),
            Self::Cli => format!(
                r#"# {name} - CLI Application

from roast.cli import App, command, argument, option

app = App(name="{name}", description="A Roast CLI application")

@app.command()
@argument("name", help="Name to greet")
@option("--count", "-c", default=1, help="Number of greetings")
def hello(name: str, count: int) -> None:
    """Say hello to someone."""
    for _ in range(count):
        print(f"Hello, {{name}}!")

@app.command()
def version() -> None:
    """Show version."""
    print("{name} version 0.1.0")

def main() -> None:
    """Run the CLI."""
    app.run()

if __name__ == "__main__":
    main()
"#
            ),
            Self::Gpu => format!(
                r#"# {name} - GPU Compute Application

from roast.gpu import Device, Tensor, kernel

# Get GPU device (auto-detects CUDA/OpenCL/Metal)
device = Device.default()
print(f"Using GPU: {{device.name}}")

@kernel
def vector_add(a: Tensor[float], b: Tensor[float], c: Tensor[float]) -> None:
    """GPU kernel for vector addition."""
    idx = thread_idx()
    if idx < len(a):
        c[idx] = a[idx] + b[idx]

def main() -> None:
    """Run GPU computation."""
    n = 1_000_000

    # Create tensors on GPU
    a = Tensor.rand(n, device=device)
    b = Tensor.rand(n, device=device)
    c = Tensor.zeros(n, device=device)

    # Launch kernel
    vector_add[n // 256, 256](a, b, c)

    # Verify result
    result = c.to_cpu()
    print(f"Computed {{n}} element vector addition on GPU")
    print(f"First 5 results: {{result[:5]}}")

if __name__ == "__main__":
    main()
"#
            ),
        }
    }

    /// Generate test file.
    pub fn generate_test(&self, name: &str) -> String {
        match self {
            Self::Library => format!(
                r#"# Tests for {name}

from {name} import hello, version

def test_version() -> None:
    """Test version function."""
    assert version() == "0.1.0"

def test_hello(capsys) -> None:
    """Test hello function."""
    hello("Test")
    captured = capsys.read()
    assert "Hello, Test!" in captured.out
"#
            ),
            _ => format!(
                r#"# Tests for {name}

def test_sanity() -> None:
    """Basic sanity test."""
    assert 1 + 1 == 2

def test_strings() -> None:
    """Test string operations."""
    s = "hello"
    assert len(s) == 5
    assert s.upper() == "HELLO"
"#
            ),
        }
    }

    /// Generate README.
    pub fn generate_readme(&self, name: &str) -> String {
        format!(
            r#"# {name}

A Roast {}.

## Installation

```bash
kitchen install
```

## Usage

```bash
kitchen run
```

## Development

```bash
# Run tests
kitchen test

# Format code
kitchen fmt

# Check types
kitchen check
```

## License

MIT
"#,
            self.description()
        )
    }

    /// Generate .gitignore.
    pub fn generate_gitignore(&self) -> String {
        r#"# Build artifacts (Roast's "cooked" directory - like Rust's "target")
/cooked/
*.rbc

# Virtual environment
/.venv/
/.kitchen/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Logs
*.log

# Local configuration
.env
.env.local

# Cache
__pycache__/
*.pyc
.roast_cache/
"#.to_string()
    }
}

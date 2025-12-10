//! Kitchen - Project and Environment Manager for Roast
//!
//! A comprehensive tool for managing Roast projects, like Cargo for Rust
//! or uv/Poetry for Python.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, Args};
use colored::Colorize;
use std::path::PathBuf;

use roast_kitchen::{
    project::{Project, ProjectTemplate},
    venv::VirtualEnv,
    build::{Builder, BuildOptions, BuildMode},
    gpu::{GpuInfo, print_gpu_info},
    cache::Cache,
    registry::Registry,
    lock::Lockfile,
    config::KitchenConfig,
    scripts::ScriptRunner,
    workspace::Workspace,
};

/// Kitchen - Roast Project Manager
#[derive(Parser)]
#[command(name = "kitchen")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Quiet mode (minimal output)
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Use offline mode
    #[arg(long, global = true)]
    offline: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Roast project
    New(NewArgs),

    /// Initialize a Roast project in existing directory
    Init(InitArgs),

    /// Build the project
    Build(BuildArgs),

    /// Run the project
    Run(RunArgs),

    /// Run tests
    Test(TestArgs),

    /// Run benchmarks
    Bench(BenchArgs),

    /// Type check without building
    Check(CheckArgs),

    /// Format source files
    Fmt(FmtArgs),

    /// Clean build artifacts
    Clean(CleanArgs),

    /// Generate documentation
    Doc(DocArgs),

    // --- Dependency Management ---

    /// Add a dependency
    Add(AddArgs),

    /// Remove a dependency
    Remove(RemoveArgs),

    /// Update dependencies
    Update(UpdateArgs),

    /// Install dependencies
    Install(InstallArgs),

    /// Show dependency tree
    Tree,

    /// List installed packages
    List(ListArgs),

    // --- Virtual Environment ---

    /// Create a virtual environment
    Venv(VenvArgs),

    /// Activate virtual environment (print activation command)
    Activate,

    // --- Publishing ---

    /// Publish to registry
    Publish(PublishArgs),

    /// Login to registry
    Login(LoginArgs),

    /// Search packages
    Search(SearchArgs),

    /// Show package info
    Info(InfoArgs),

    // --- Utilities ---

    /// Run a script
    #[command(name = "run-script")]
    RunScript(RunScriptArgs),

    /// Show GPU information
    Gpu,

    /// Manage cache
    Cache(CacheArgs),

    /// Show version
    Version,

    /// Show project info
    About,
}

#[derive(Args)]
struct NewArgs {
    /// Project name
    name: String,

    /// Project template
    #[arg(long, short, default_value = "binary")]
    template: String,

    /// Initialize git repository
    #[arg(long)]
    git: bool,

    /// Path to create project in
    #[arg(long)]
    path: Option<PathBuf>,
}

#[derive(Args)]
struct InitArgs {
    /// Project name (defaults to directory name)
    name: Option<String>,

    /// Project template
    #[arg(long, short, default_value = "binary")]
    template: String,

    /// Initialize git repository
    #[arg(long)]
    git: bool,
}

#[derive(Args)]
struct BuildArgs {
    /// Build in release mode
    #[arg(long, short)]
    release: bool,

    /// Specific targets to build
    #[arg(long)]
    target: Vec<String>,

    /// Enable features
    #[arg(long, short = 'F')]
    features: Vec<String>,

    /// Disable default features
    #[arg(long)]
    no_default_features: bool,

    /// Enable all features
    #[arg(long)]
    all_features: bool,

    /// Number of parallel jobs
    #[arg(long, short)]
    jobs: Option<usize>,

    /// Enable GPU compilation
    #[arg(long)]
    gpu: bool,
}

#[derive(Args)]
struct RunArgs {
    /// Build in release mode
    #[arg(long, short)]
    release: bool,

    /// Arguments to pass to the program
    args: Vec<String>,

    /// Enable GPU
    #[arg(long)]
    gpu: bool,
}

#[derive(Args)]
struct TestArgs {
    /// Test name filter
    filter: Option<String>,

    /// Run tests in release mode
    #[arg(long)]
    release: bool,

    /// Show output from tests
    #[arg(long)]
    nocapture: bool,

    /// Run tests in parallel
    #[arg(long)]
    parallel: bool,
}

#[derive(Args)]
struct BenchArgs {
    /// Benchmark name filter
    filter: Option<String>,

    /// Save baseline with name
    #[arg(long)]
    save_baseline: Option<String>,
}

#[derive(Args)]
struct CheckArgs {
    /// Path to check
    path: Option<PathBuf>,

    /// Enable strict mode
    #[arg(long)]
    strict: bool,
}

#[derive(Args)]
struct FmtArgs {
    /// Path to format
    path: Option<PathBuf>,

    /// Check formatting without modifying
    #[arg(long)]
    check: bool,
}

#[derive(Args)]
struct CleanArgs {
    /// Clean everything including dependencies, caches, and virtual environments
    #[arg(long)]
    all: bool,

    /// Also remove lock files (roast.lock, python.lock)
    #[arg(long)]
    locks: bool,

    /// Also clean global cache
    #[arg(long)]
    cache: bool,
}

#[derive(Args)]
struct DocArgs {
    /// Open documentation in browser
    #[arg(long)]
    open: bool,

    /// Include private items
    #[arg(long)]
    private: bool,
}

#[derive(Args)]
struct AddArgs {
    /// Package(s) to add
    packages: Vec<String>,

    /// Add as dev dependency
    #[arg(long, short = 'D')]
    dev: bool,

    /// Specific version
    #[arg(long)]
    version: Option<String>,

    /// Add from requirements file
    #[arg(long, short = 'r')]
    requirements: Option<PathBuf>,

    /// Add from git
    #[arg(long)]
    git: Option<String>,

    /// Git branch
    #[arg(long)]
    branch: Option<String>,

    /// Add from path
    #[arg(long)]
    path: Option<PathBuf>,

    /// Features to enable
    #[arg(long, short = 'F')]
    features: Vec<String>,
}

#[derive(Args)]
struct RemoveArgs {
    /// Package(s) to remove
    packages: Vec<String>,
}

#[derive(Args)]
struct UpdateArgs {
    /// Specific packages to update
    packages: Vec<String>,

    /// Allow pre-release versions
    #[arg(long)]
    prerelease: bool,

    /// Dry run
    #[arg(long, short = 'n')]
    dry_run: bool,
}

#[derive(Args)]
struct InstallArgs {
    /// Reinstall all packages
    #[arg(long)]
    reinstall: bool,

    /// Use exact versions from lockfile
    #[arg(long)]
    locked: bool,

    /// Don't update lockfile
    #[arg(long)]
    frozen: bool,
}

#[derive(Args)]
struct ListArgs {
    /// Show only Roast packages
    #[arg(long)]
    roast: bool,

    /// Show only Python packages
    #[arg(long)]
    python: bool,

    /// Show outdated packages
    #[arg(long)]
    outdated: bool,
}

#[derive(Args)]
struct VenvArgs {
    /// Path for virtual environment
    #[arg(default_value = ".venv")]
    path: PathBuf,

    /// Include system site packages
    #[arg(long)]
    system_site_packages: bool,

    /// Clear existing venv
    #[arg(long)]
    clear: bool,
}

#[derive(Args)]
struct PublishArgs {
    /// Skip confirmation
    #[arg(long, short)]
    yes: bool,

    /// Registry to publish to
    #[arg(long)]
    registry: Option<String>,

    /// Dry run
    #[arg(long, short = 'n')]
    dry_run: bool,
}

#[derive(Args)]
struct LoginArgs {
    /// Registry to login to
    registry: Option<String>,

    /// Token (if not provided, will prompt)
    #[arg(long)]
    token: Option<String>,
}

#[derive(Args)]
struct SearchArgs {
    /// Search query
    query: String,

    /// Maximum results
    #[arg(long, default_value = "10")]
    limit: usize,
}

#[derive(Args)]
struct InfoArgs {
    /// Package name
    package: String,

    /// Specific version
    #[arg(long)]
    version: Option<String>,
}

#[derive(Args)]
struct RunScriptArgs {
    /// Script name
    name: String,

    /// Arguments to pass
    args: Vec<String>,
}

#[derive(Args)]
struct CacheArgs {
    #[command(subcommand)]
    action: CacheAction,
}

#[derive(Subcommand)]
enum CacheAction {
    /// Show cache info
    Info,
    /// Clear cache
    Clear,
    /// Garbage collect old entries
    Gc,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New(args) => cmd_new(args, cli.verbose),
        Commands::Init(args) => cmd_init(args, cli.verbose),
        Commands::Build(args) => cmd_build(args, cli.verbose),
        Commands::Run(args) => cmd_run(args, cli.verbose),
        Commands::Test(args) => cmd_test(args, cli.verbose),
        Commands::Bench(args) => cmd_bench(args, cli.verbose),
        Commands::Check(args) => cmd_check(args),
        Commands::Fmt(args) => cmd_fmt(args),
        Commands::Clean(args) => cmd_clean(args),
        Commands::Doc(args) => cmd_doc(args),
        Commands::Add(args) => cmd_add(args).await,
        Commands::Remove(args) => cmd_remove(args),
        Commands::Update(args) => cmd_update(args).await,
        Commands::Install(args) => cmd_install(args).await,
        Commands::Tree => cmd_tree(),
        Commands::List(args) => cmd_list(args),
        Commands::Venv(args) => cmd_venv(args),
        Commands::Activate => cmd_activate(),
        Commands::Publish(args) => cmd_publish(args).await,
        Commands::Login(args) => cmd_login(args).await,
        Commands::Search(args) => cmd_search(args).await,
        Commands::Info(args) => cmd_info(args).await,
        Commands::RunScript(args) => cmd_run_script(args),
        Commands::Gpu => cmd_gpu(),
        Commands::Cache(args) => cmd_cache(args),
        Commands::Version => cmd_version(),
        Commands::About => cmd_about(),
    }
}

fn cmd_new(args: NewArgs, verbose: bool) -> Result<()> {
    let template = match args.template.as_str() {
        "binary" | "bin" => ProjectTemplate::Binary,
        "library" | "lib" => ProjectTemplate::Library,
        "web" => ProjectTemplate::Web,
        "cli" => ProjectTemplate::Cli,
        "gpu" => ProjectTemplate::Gpu,
        _ => bail!("Unknown template: {}. Use: binary, library, web, cli, gpu", args.template),
    };

    let path = args.path.unwrap_or_else(|| PathBuf::from(&args.name));

    println!("{} {} `{}` package",
        "     Creating".green().bold(),
        args.template,
        args.name
    );

    let project = Project::new(&args.name, &path, template)?;

    if args.git {
        std::process::Command::new("git")
            .args(["init", &path.to_string_lossy()])
            .status()
            .context("Failed to initialize git repository")?;
    }

    println!("{} Roast package `{}`",
        "      Created".green().bold(),
        args.name
    );

    println!();
    println!("To get started:");
    println!("  cd {}", path.display());
    println!("  kitchen run");

    Ok(())
}

fn cmd_init(args: InitArgs, verbose: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;

    let name = args.name.or_else(|| {
        current_dir.file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    });

    println!("{} Roast package in `{}`",
        "     Creating".green().bold(),
        current_dir.display()
    );

    let project = Project::init(&current_dir, name.as_deref())?;

    if args.git && !current_dir.join(".git").exists() {
        std::process::Command::new("git")
            .arg("init")
            .status()
            .context("Failed to initialize git repository")?;
    }

    println!("{} Roast package `{}`",
        " Initialized".green().bold(),
        project.name()
    );

    Ok(())
}

fn cmd_build(args: BuildArgs, verbose: bool) -> Result<()> {
    let project = Project::find_current()?;

    let options = BuildOptions {
        mode: if args.release { BuildMode::Release } else { BuildMode::Debug },
        targets: args.target,
        features: args.features,
        no_default_features: args.no_default_features,
        all_features: args.all_features,
        jobs: args.jobs,
        verbose,
        gpu: args.gpu,
        force: false,
        native: args.release, // Use native compilation for release builds
    };

    let builder = Builder::new(project, options);
    let result = builder.build()?;

    if verbose {
        println!();
        println!("Built {} artifacts:", result.artifacts.len());
        for artifact in &result.artifacts {
            println!("  {:?}: {}", artifact.kind, artifact.path.display());
        }
    }

    Ok(())
}

fn cmd_run(args: RunArgs, verbose: bool) -> Result<()> {
    // First build
    cmd_build(BuildArgs {
        release: args.release,
        target: vec![],
        features: vec![],
        no_default_features: false,
        all_features: false,
        jobs: None,
        gpu: args.gpu,
    }, verbose)?;

    let project = Project::find_current()?;

    println!("{} `{}`",
        "     Running".green().bold(),
        project.entry_point().display()
    );

    let exit_code = roast_kitchen::build::run(&project, &args.args)?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

fn cmd_test(args: TestArgs, verbose: bool) -> Result<()> {
    let project = Project::find_current()?;

    let result = roast_kitchen::build::test(
        &project,
        args.filter.as_deref(),
        verbose || args.nocapture,
    )?;

    if result.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn cmd_bench(args: BenchArgs, verbose: bool) -> Result<()> {
    let project = Project::find_current()?;

    println!("{} benchmarks",
        "     Running".green().bold()
    );

    // Build in release mode first
    cmd_build(BuildArgs {
        release: true,
        target: vec![],
        features: vec![],
        no_default_features: false,
        all_features: false,
        jobs: None,
        gpu: false,
    }, verbose)?;

    // Run benchmarks
    println!("Benchmarking...");

    Ok(())
}

fn cmd_check(args: CheckArgs) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("src"));

    println!("{} `{}`",
        "    Checking".green().bold(),
        path.display()
    );

    let status = std::process::Command::new("roastc")
        .arg("check")
        .arg(&path)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    println!("{} No errors found",
        "    Finished".green().bold()
    );

    Ok(())
}

fn cmd_fmt(args: FmtArgs) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("src"));

    let action = if args.check { "Checking" } else { "Formatting" };

    println!("{} `{}`",
        format!("{:>12}", action).green().bold(),
        path.display()
    );

    let mut cmd = std::process::Command::new("roastc");
    cmd.arg("fmt").arg(&path);

    if args.check {
        cmd.arg("--check");
    }

    let status = cmd.status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn cmd_clean(args: CleanArgs) -> Result<()> {
    let project = Project::find_current()?;

    if args.all {
        println!("{} `{}` (including dependencies)",
            "    Cleaning".green().bold(),
            project.root.display()
        );

        // Clean build artifacts
        project.clean()?;

        // Clean dependencies cache
        let roast_dir = project.root.join(".roast");
        if roast_dir.exists() {
            println!("  {} .roast/", "Removing".cyan());
            std::fs::remove_dir_all(&roast_dir)?;
        }

        // Clean Python virtual environment
        let venv_dir = project.root.join(".venv");
        if venv_dir.exists() {
            println!("  {} .venv/", "Removing".cyan());
            std::fs::remove_dir_all(&venv_dir)?;
        }

        // Clean lockfiles (optional - user should commit these)
        if args.locks {
            let lock_file = project.root.join("roast.lock");
            if lock_file.exists() {
                println!("  {} roast.lock", "Removing".cyan());
                std::fs::remove_file(&lock_file)?;
            }

            let py_lock = project.root.join("python.lock");
            if py_lock.exists() {
                println!("  {} python.lock", "Removing".cyan());
                std::fs::remove_file(&py_lock)?;
            }
        }

        // Clean global cache if requested
        if args.cache {
            if let Some(cache_dir) = dirs::cache_dir() {
                let roast_cache = cache_dir.join("roast");
                if roast_cache.exists() {
                    println!("  {} global cache", "Removing".cyan());
                    std::fs::remove_dir_all(&roast_cache)?;
                }
            }
        }

        println!("{}", "     Cleaned".green().bold());
    } else {
        println!("{} `{}`",
            "    Cleaning".green().bold(),
            project.root.display()
        );

        project.clean()?;

        println!("{} (use --all to also clean dependencies)",
            "     Cleaned".green().bold()
        );
    }

    Ok(())
}

fn cmd_doc(args: DocArgs) -> Result<()> {
    let project = Project::find_current()?;

    println!("{} `{}`",
        " Documenting".green().bold(),
        project.name()
    );

    let mut cmd = std::process::Command::new("roastc");
    cmd.arg("doc");

    if args.open {
        cmd.arg("--open");
    }

    let status = cmd.status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

async fn cmd_add(args: AddArgs) -> Result<()> {
    use roast_kitchen::pypi::{is_python_package, strip_python_prefix, PythonPackage, PythonEnv, PyPIClient};
    use std::io::{BufRead, BufReader};

    let mut project = Project::find_current()?;

    // Handle requirements file
    if let Some(ref req_file) = args.requirements {
        let req_str = req_file.to_string_lossy();

        // Check if it's a Python requirements file (py:requirements.txt)
        if req_str.starts_with("py:") || req_str.starts_with("python:") {
            let file_path = if req_str.starts_with("py:") {
                PathBuf::from(&req_str[3..])
            } else {
                PathBuf::from(&req_str[7..])
            };

            println!("{} from {}",
                "  Installing".cyan().bold(),
                file_path.display()
            );

            let py_env = PythonEnv::new(&project.root);
            py_env.install_requirements(&file_path)?;

            // Parse and add to config
            let file = std::fs::File::open(&file_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                let line = line.trim();

                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let pkg = PythonPackage::parse(line);
                let key = format!("py:{}", pkg.name);
                let version = pkg.version.unwrap_or_else(|| "*".to_string());
                project.add_dependency(&key, &version, args.dev)?;
            }

            println!("{} Python requirements",
                "   Installed".green().bold()
            );

            update_lockfile(&project)?;
            return Ok(());
        } else {
            // Roast requirements file
            println!("{} from {}",
                "      Adding".green().bold(),
                req_file.display()
            );

            let file = std::fs::File::open(req_file)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                let line = line.trim();

                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Parse name and version
                let (name, version) = if let Some(pos) = line.find("==") {
                    (line[..pos].to_string(), line[pos+2..].to_string())
                } else if let Some(pos) = line.find(">=") {
                    (line[..pos].to_string(), format!(">={}", &line[pos+2..]))
                } else {
                    (line.to_string(), "*".to_string())
                };

                project.add_dependency(&name, &version, args.dev)?;
            }

            update_lockfile(&project)?;
            println!("{} requirements",
                "       Added".green().bold()
            );
            return Ok(());
        }
    }

    // Separate Python and Roast packages
    let mut python_packages: Vec<PythonPackage> = Vec::new();
    let mut roast_packages: Vec<(String, String)> = Vec::new();

    for package in &args.packages {
        if is_python_package(package) {
            // Python package (py:name or python:name)
            let spec = strip_python_prefix(package);
            let py_pkg = PythonPackage::parse(spec);
            python_packages.push(PythonPackage {
                dev: args.dev,
                ..py_pkg
            });
        } else {
            // Roast package
            let version = args.version.as_deref().unwrap_or("*").to_string();
            roast_packages.push((package.clone(), version));
        }
    }

    // Handle Python packages
    if !python_packages.is_empty() {
        println!("{} Python packages...",
            "  Installing".cyan().bold()
        );

        let py_env = PythonEnv::new(&project.root);

        // Fetch package info from PyPI
        let client = PyPIClient::new();

        for pkg in &python_packages {
            println!("{} py:{} {}",
                "      Adding".green().bold(),
                pkg.name,
                pkg.version.as_deref().unwrap_or("(latest)")
            );

            // Verify package exists on PyPI
            match client.get_package(&pkg.name).await {
                Ok(info) => {
                    println!("        {} - {}",
                        info.info.version.dimmed(),
                        info.info.summary.as_deref().unwrap_or("").dimmed()
                    );
                }
                Err(e) => {
                    println!("{} Package '{}' not found on PyPI: {}",
                        "       Error".red().bold(),
                        pkg.name,
                        e
                    );
                    continue;
                }
            }
        }

        // Install with pip
        py_env.install_many(&python_packages)?;

        // Add to project config
        for pkg in &python_packages {
            let key = format!("py:{}", pkg.name);
            let version = pkg.version.clone().unwrap_or_else(|| "*".to_string());
            project.add_dependency(&key, &version, pkg.dev)?;
        }

        // Save Python lock
        if let Ok(installed) = py_env.list() {
            roast_kitchen::pypi::save_python_lock(&project.root, &installed)?;
        }

        println!("{} Python packages",
            "   Installed".green().bold()
        );
    }

    // Handle Roast packages
    for (package, version) in &roast_packages {
        println!("{} {} v{}",
            "      Adding".green().bold(),
            package,
            version
        );

        project.add_dependency(package, version, args.dev)?;
    }

    // Update roast.lock
    if !roast_packages.is_empty() || !python_packages.is_empty() {
        update_lockfile(&project)?;

        println!("{} roast.lock",
            "     Updated".green().bold()
        );
    }

    Ok(())
}

fn update_lockfile(project: &Project) -> Result<()> {
    use roast_kitchen::lock::{Lockfile, LockedPackage};

    let lock_path = project.root.join("roast.lock");

    let mut lockfile = if lock_path.exists() {
        Lockfile::load(&lock_path)?
    } else {
        Lockfile::new()
    };

    // Add/update entries for each dependency
    for (name, config) in &project.config.dependencies {
        let version = match config {
            roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
            roast_kitchen::config::DependencyConfig::Detailed(d) => {
                d.version.clone().unwrap_or_else(|| "*".to_string())
            }
        };

        let source = if name.starts_with("py:") {
            format!("pypi+{}", &name[3..])
        } else {
            format!("registry+{}", roast_kitchen::DEFAULT_REGISTRY)
        };

        lockfile.add(LockedPackage {
            name: name.clone(),
            version,
            source,
            checksum: None,
            dependencies: vec![],
            features: vec![],
        });
    }

    lockfile.save(&lock_path)?;

    Ok(())
}

fn cmd_remove(args: RemoveArgs) -> Result<()> {
    use roast_kitchen::pypi::{is_python_package, strip_python_prefix, PythonEnv};

    let mut project = Project::find_current()?;

    for package in &args.packages {
        println!("{} {}",
            "    Removing".green().bold(),
            package
        );

        // Check if it's a Python package
        if is_python_package(package) || package.starts_with("py:") {
            let pkg_name = if is_python_package(package) {
                strip_python_prefix(package)
            } else if package.starts_with("py:") {
                &package[3..]
            } else {
                package
            };

            let py_env = PythonEnv::new(&project.root);
            if py_env.exists() {
                py_env.uninstall(pkg_name)?;
            }
        }

        project.remove_dependency(package)?;
    }

    // Update lockfile
    update_lockfile(&project)?;

    println!("{} roast.lock",
        "     Updated".green().bold()
    );

    Ok(())
}

async fn cmd_update(args: UpdateArgs) -> Result<()> {
    let project = Project::find_current()?;

    if args.packages.is_empty() {
        println!("{} all dependencies",
            "    Updating".green().bold()
        );
    } else {
        for package in &args.packages {
            println!("{} {}",
                "    Updating".green().bold(),
                package
            );
        }
    }

    if args.dry_run {
        println!("{} (dry run)",
            "    Skipping".yellow().bold()
        );
    }

    Ok(())
}

async fn cmd_install(args: InstallArgs) -> Result<()> {
    use roast_kitchen::pypi::{PythonPackage, PythonEnv};

    let project = Project::find_current()?;

    println!("{} dependencies",
        "  Installing".green().bold()
    );

    // Separate Python and Roast packages
    let mut python_packages: Vec<PythonPackage> = Vec::new();

    for (name, config) in &project.config.dependencies {
        let version = match config {
            roast_kitchen::config::DependencyConfig::Simple(v) => Some(v.clone()),
            roast_kitchen::config::DependencyConfig::Detailed(d) => d.version.clone(),
        };

        if name.starts_with("py:") {
            python_packages.push(PythonPackage {
                name: name[3..].to_string(),
                version,
                dev: false,
            });
        }
    }

    for (name, config) in &project.config.dev_dependencies {
        let version = match config {
            roast_kitchen::config::DependencyConfig::Simple(v) => Some(v.clone()),
            roast_kitchen::config::DependencyConfig::Detailed(d) => d.version.clone(),
        };

        if name.starts_with("py:") {
            python_packages.push(PythonPackage {
                name: name[3..].to_string(),
                version,
                dev: true,
            });
        }
    }

    // Install Python packages
    if !python_packages.is_empty() {
        println!("{} Python packages...",
            "  Installing".cyan().bold()
        );

        let py_env = PythonEnv::new(&project.root);
        py_env.install_many(&python_packages)?;

        println!("{} {} Python packages",
            "   Installed".green().bold(),
            python_packages.len()
        );
    }

    // TODO: Install Roast packages from registry

    println!("{} installation",
        "    Finished".green().bold()
    );

    Ok(())
}

fn cmd_list(args: ListArgs) -> Result<()> {
    use roast_kitchen::pypi::PythonEnv;

    let project = Project::find_current()?;

    println!("{} {} v{}\n",
        "📦".bold(),
        project.name().bold(),
        project.version()
    );

    let show_roast = !args.python || args.roast;
    let show_python = !args.roast || args.python;

    // List Roast dependencies
    if show_roast {
        let roast_deps: Vec<_> = project.config.dependencies.iter()
            .filter(|(name, _)| !name.starts_with("py:"))
            .collect();

        let roast_dev_deps: Vec<_> = project.config.dev_dependencies.iter()
            .filter(|(name, _)| !name.starts_with("py:"))
            .collect();

        if !roast_deps.is_empty() || !roast_dev_deps.is_empty() {
            println!("{}", "Roast packages:".cyan().bold());

            for (name, config) in roast_deps {
                let version = match config {
                    roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
                    roast_kitchen::config::DependencyConfig::Detailed(d) => {
                        d.version.clone().unwrap_or_else(|| "*".to_string())
                    }
                };
                println!("  {} {}", name.green(), version.dimmed());
            }

            if !roast_dev_deps.is_empty() {
                println!("\n  {} (dev):", "Dev".yellow());
                for (name, config) in roast_dev_deps {
                    let version = match config {
                        roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
                        roast_kitchen::config::DependencyConfig::Detailed(d) => {
                            d.version.clone().unwrap_or_else(|| "*".to_string())
                        }
                    };
                    println!("    {} {}", name.green(), version.dimmed());
                }
            }
            println!();
        }
    }

    // List Python dependencies
    if show_python {
        let py_deps: Vec<_> = project.config.dependencies.iter()
            .filter(|(name, _)| name.starts_with("py:"))
            .collect();

        let py_dev_deps: Vec<_> = project.config.dev_dependencies.iter()
            .filter(|(name, _)| name.starts_with("py:"))
            .collect();

        if !py_deps.is_empty() || !py_dev_deps.is_empty() {
            println!("{}", "Python packages:".cyan().bold());

            for (name, config) in py_deps {
                let version = match config {
                    roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
                    roast_kitchen::config::DependencyConfig::Detailed(d) => {
                        d.version.clone().unwrap_or_else(|| "*".to_string())
                    }
                };
                // Remove py: prefix for display
                let display_name = &name[3..];
                println!("  {} {}", display_name.blue(), version.dimmed());
            }

            if !py_dev_deps.is_empty() {
                println!("\n  {} (dev):", "Dev".yellow());
                for (name, config) in py_dev_deps {
                    let version = match config {
                        roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
                        roast_kitchen::config::DependencyConfig::Detailed(d) => {
                            d.version.clone().unwrap_or_else(|| "*".to_string())
                        }
                    };
                    let display_name = &name[3..];
                    println!("    {} {}", display_name.blue(), version.dimmed());
                }
            }
            println!();
        }

        // Show actually installed Python packages from venv
        let py_env = PythonEnv::new(&project.root);
        if py_env.exists() {
            if let Ok(installed) = py_env.list() {
                if !installed.is_empty() {
                    println!("{}", "Installed Python packages (in venv):".dimmed());
                    for pkg in installed {
                        println!("  {} {}", pkg.name.blue(), pkg.version.dimmed());
                    }
                    println!();
                }
            }

            // Show outdated if requested
            if args.outdated {
                if let Ok(outdated) = py_env.outdated() {
                    if !outdated.is_empty() {
                        println!("{}", "Outdated Python packages:".yellow().bold());
                        for pkg in outdated {
                            println!("  {} {} -> {}",
                                pkg.name.blue(),
                                pkg.version.dimmed(),
                                pkg.latest_version.green()
                            );
                        }
                        println!();
                    }
                }
            }
        }
    }

    Ok(())
}

fn cmd_tree() -> Result<()> {
    let project = Project::find_current()?;

    println!("{} v{}",
        project.name().bold(),
        project.version()
    );

    for (name, config) in &project.config.dependencies {
        let version = match config {
            roast_kitchen::config::DependencyConfig::Simple(v) => v.clone(),
            roast_kitchen::config::DependencyConfig::Detailed(d) => {
                d.version.clone().unwrap_or_else(|| "*".to_string())
            }
        };
        println!("├── {} v{}", name, version);
    }

    Ok(())
}

fn cmd_venv(args: VenvArgs) -> Result<()> {
    if args.clear && args.path.exists() {
        std::fs::remove_dir_all(&args.path)?;
    }

    println!("{} virtual environment at `{}`",
        "    Creating".green().bold(),
        args.path.display()
    );

    let config = roast_kitchen::venv::VenvConfig {
        roast_version: roast_kitchen::VERSION.to_string(),
        python_version: None,
        packages: std::collections::HashMap::new(),
        system_site_packages: args.system_site_packages,
        include_pip: false,
    };

    let venv = VirtualEnv::create(&args.path, config)?;

    println!("{} virtual environment",
        "     Created".green().bold()
    );

    println!();
    println!("To activate:");
    if cfg!(windows) {
        println!("  {}\\Scripts\\Activate.ps1", args.path.display());
    } else {
        println!("  source {}/bin/activate", args.path.display());
    }

    Ok(())
}

fn cmd_activate() -> Result<()> {
    let project = Project::find_current()?;

    if let Some(venv_path) = roast_kitchen::venv::find_venv(&project.root) {
        if cfg!(windows) {
            println!("{}\\Scripts\\Activate.ps1", venv_path.display());
        } else {
            println!("source {}/bin/activate", venv_path.display());
        }
    } else {
        println!("{} No virtual environment found. Create one with: kitchen venv",
            "       Error".red().bold()
        );
        std::process::exit(1);
    }

    Ok(())
}

async fn cmd_publish(args: PublishArgs) -> Result<()> {
    let project = Project::find_current()?;

    println!("{} `{}` v{}",
        "  Publishing".green().bold(),
        project.name(),
        project.version()
    );

    if args.dry_run {
        println!("{} (dry run)",
            "    Skipping".yellow().bold()
        );
        return Ok(());
    }

    if !args.yes {
        print!("Continue? [y/N] ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled");
            return Ok(());
        }
    }

    // Create tarball and publish
    println!("{} package",
        "    Packing".green().bold()
    );

    let tarball_path = project.target_dir().join(format!(
        "{}-{}.tar.gz",
        project.name(),
        project.version()
    ));

    std::fs::create_dir_all(project.target_dir())?;
    roast_kitchen::registry::create_tarball(&project.root, &tarball_path)?;

    println!("{} published!",
        "    Package".green().bold()
    );

    Ok(())
}

async fn cmd_login(args: LoginArgs) -> Result<()> {
    let registry = args.registry.unwrap_or_else(|| roast_kitchen::DEFAULT_REGISTRY.to_string());

    println!("Logging in to {}", registry);

    let token = if let Some(t) = args.token {
        t
    } else {
        print!("Token: ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut token = String::new();
        std::io::stdin().read_line(&mut token)?;
        token.trim().to_string()
    };

    // Save token
    let mut config = KitchenConfig::load()?;
    config.set_token(&registry, &token);
    config.save()?;

    println!("{} logged in to {}",
        " Successfully".green().bold(),
        registry
    );

    Ok(())
}

async fn cmd_search(args: SearchArgs) -> Result<()> {
    println!("{} for `{}`",
        "  Searching".green().bold(),
        args.query
    );

    let config = KitchenConfig::load()?;
    let cache = Cache::new(config.cache_dir.clone());
    let registry = Registry::new(&config.registry, cache);

    let results = registry.search(&args.query).await?;

    if results.is_empty() {
        println!("No packages found");
    } else {
        for pkg in results.iter().take(args.limit) {
            println!("{} - {}",
                pkg.name.bold(),
                pkg.description.as_deref().unwrap_or("No description")
            );
        }
    }

    Ok(())
}

async fn cmd_info(args: InfoArgs) -> Result<()> {
    let config = KitchenConfig::load()?;
    let cache = Cache::new(config.cache_dir.clone());
    let registry = Registry::new(&config.registry, cache);

    let metadata = registry.get_package(&args.package).await?;

    println!("{} {}", "Package:".bold(), metadata.name);

    if let Some(desc) = &metadata.description {
        println!("{} {}", "Description:".bold(), desc);
    }

    if let Some(license) = &metadata.license {
        println!("{} {}", "License:".bold(), license);
    }

    if let Some(repo) = &metadata.repository {
        println!("{} {}", "Repository:".bold(), repo);
    }

    println!();
    println!("{}", "Versions:".bold());
    for version in metadata.versions.iter().take(10) {
        let yanked = if version.yanked { " (yanked)" } else { "" };
        println!("  {} {}{}", "•", version.version, yanked.red());
    }

    Ok(())
}

fn cmd_run_script(args: RunScriptArgs) -> Result<()> {
    let project = Project::find_current()?;

    let runner = ScriptRunner::new(&project.root)
        .with_scripts(project.config.scripts.clone());

    if !runner.has(&args.name) {
        bail!("Script '{}' not found. Available scripts: {:?}",
            args.name,
            runner.list().iter().map(|(k, _)| k).collect::<Vec<_>>()
        );
    }

    println!("{} script `{}`",
        "     Running".green().bold(),
        args.name
    );

    let status = runner.run(&args.name, &args.args)?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn cmd_gpu() -> Result<()> {
    print_gpu_info();
    Ok(())
}

fn cmd_cache(args: CacheArgs) -> Result<()> {
    let config = KitchenConfig::load()?;
    let cache = Cache::new(config.cache_dir.clone());

    match args.action {
        CacheAction::Info => {
            let size = cache.size()?;
            println!("{} {}", "Cache directory:".bold(), config.cache_dir.display());
            println!("{} {:.2} MB", "Size:".bold(), size as f64 / 1024.0 / 1024.0);
        }
        CacheAction::Clear => {
            println!("{} cache",
                "    Clearing".green().bold()
            );
            cache.clear()?;
            println!("{} cache cleared",
                " Successfully".green().bold()
            );
        }
        CacheAction::Gc => {
            println!("{} old cache entries",
                "   Collecting".green().bold()
            );
            let result = cache.gc(std::time::Duration::from_secs(7 * 24 * 3600))?;
            println!("{} {} files, freed {}",
                "     Removed".green().bold(),
                result.removed_files,
                result.freed_size_human()
            );
        }
    }

    Ok(())
}

fn cmd_version() -> Result<()> {
    println!("kitchen {}", roast_kitchen::VERSION);

    // Check for GPU
    if let Ok(gpu) = GpuInfo::detect() {
        if gpu.is_available() {
            println!("GPU: {} ({})", gpu.device_name, gpu.backend.as_str());
        }
    }

    Ok(())
}

fn cmd_about() -> Result<()> {
    println!("{}", r#"
    🔥 Kitchen - Roast Project Manager 🍳

    Kitchen is the all-in-one tool for managing Roast projects.
    Think of it as Cargo (Rust) + uv (Python) combined.

    Features:
    • Virtual environments
    • Dependency management
    • Project scaffolding
    • Build system
    • GPU support
    • Package publishing

    For more information: https://roast-lang.org
    "#.trim());

    Ok(())
}

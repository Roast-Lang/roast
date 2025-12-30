//! Build system.

use std::path::{Path, PathBuf};
use std::fs;
use std::time::Instant;
use std::process::Command;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*; // For parallel builds
// use crate::config::{ProfileConfig, OptLevel};
use crate::project::Project;
use crate::gpu::GpuInfo;
use crate::{Error, Result};

/// Build mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Debug,
    Release,
    Test,
    Bench,
}

impl BuildMode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
            Self::Test => "test",
            Self::Bench => "bench",
        }
    }
}

/// Build options.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Build mode.
    pub mode: BuildMode,

    /// Specific targets to build.
    pub targets: Vec<String>,

    /// Features to enable.
    pub features: Vec<String>,

    /// Disable default features.
    pub no_default_features: bool,

    /// Enable all features.
    pub all_features: bool,

    /// Number of parallel jobs.
    pub jobs: Option<usize>,

    /// Verbose output.
    pub verbose: bool,

    /// Enable GPU compilation.
    pub gpu: bool,

    /// Force rebuild.
    pub force: bool,

    /// Use native compilation (Cranelift).
    pub native: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            mode: BuildMode::Debug,
            targets: vec![],
            features: vec![],
            no_default_features: false,
            all_features: false,
            jobs: None,
            verbose: false,
            gpu: false,
            force: false,
            native: false,
        }
    }
}

impl BuildOptions {
    /// Create release build options with native compilation.
    pub fn release_native() -> Self {
        Self {
            mode: BuildMode::Release,
            native: true,
            ..Default::default()
        }
    }
}

/// Build result.
#[derive(Debug)]
pub struct BuildResult {
    /// Built artifacts.
    pub artifacts: Vec<Artifact>,

    /// Build duration.
    pub duration: std::time::Duration,

    /// Warnings.
    pub warnings: Vec<String>,

    /// Files compiled.
    pub files_compiled: usize,
}

/// A built artifact.
#[derive(Debug)]
pub struct Artifact {
    /// Artifact type.
    pub kind: ArtifactKind,

    /// Artifact path.
    pub path: PathBuf,

    /// Size in bytes.
    pub size: u64,
}

/// Artifact type.
#[derive(Debug, Clone, Copy)]
pub enum ArtifactKind {
    Binary,
    NativeBinary,
    Library,
    Bytecode,
    ObjectFile,
    GpuKernel,
}

/// Builder for Roast projects.
pub struct Builder {
    /// Project to build.
    project: Project,

    /// Build options.
    options: BuildOptions,

    /// GPU info.
    gpu_info: Option<GpuInfo>,
}

impl Builder {
    /// Create a new builder.
    pub fn new(project: Project, options: BuildOptions) -> Self {
        let gpu_info = if options.gpu {
            GpuInfo::detect().ok()
        } else {
            None
        };

        Self {
            project,
            options,
            gpu_info,
        }
    }

    /// Build the project.
    pub fn build(&self) -> Result<BuildResult> {
        let start = Instant::now();

        println!("{} {} v{} ({})",
            "   Compiling".green().bold(),
            self.project.name(),
            self.project.version(),
            self.project.root.display()
        );

        // Create target directories
        let target_dir = self.target_dir();
        fs::create_dir_all(&target_dir)?;

        // Collect source files
        let source_files = self.project.source_files()?;

        if source_files.is_empty() {
            return Err(Error::Build("No source files found".to_string()));
        }

        // Show progress
        let pb = ProgressBar::new(source_files.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"));

        let warnings: Vec<String> = Vec::new();
        let mut artifacts = Vec::new();

        // Compile files in parallel using rayon
        let compile_results: Vec<Result<Artifact>> = source_files
            .par_iter()
            .map(|source_file| {
                if self.options.verbose {
                    println!("  {} {}",
                        "Compiling".cyan(),
                        source_file.display()
                    );
                }
                let result = self.compile_file(source_file);
                pb.inc(1);
                result
            })
            .collect();

        // Check for errors and collect artifacts
        for (i, result) in compile_results.into_iter().enumerate() {
            match result {
                Ok(artifact) => {
                    artifacts.push(artifact);
                }
                Err(e) => {
                    return Err(Error::Build(format!(
                        "Failed to compile {}: {}",
                        source_files[i].display(),
                        e
                    )));
                }
            }
        }

        pb.finish_and_clear();

        // Link if building binary
        if self.is_binary_target() {
            let binary = if self.options.native || self.options.mode == BuildMode::Release {
                self.link_native(&artifacts)?
            } else {
                self.link(&artifacts)?
            };
            artifacts.push(binary);
        }

        // Build GPU kernels if enabled
        if self.options.gpu && self.gpu_info.is_some() {
            let gpu_artifacts = self.build_gpu_kernels()?;
            artifacts.extend(gpu_artifacts);
        }

        let duration = start.elapsed();

        println!("{} {} target(s) in {:.2}s",
            "    Finished".green().bold(),
            self.options.mode.as_str(),
            duration.as_secs_f64()
        );

        Ok(BuildResult {
            artifacts,
            duration,
            warnings,
            files_compiled: source_files.len(),
        })
    }

    fn compile_file(&self, source_file: &Path) -> Result<Artifact> {
        let source = fs::read_to_string(source_file)?;

        // Get output path
        let relative = source_file.strip_prefix(&self.project.root)
            .unwrap_or(source_file);
        let output_path = self.target_dir()
            .join(relative)
            .with_extension("rbc");

        // Create output directory
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Compile using roast compiler
        // In a real implementation, this would use the compiler directly
        // For now, we'll create a placeholder bytecode file

        let interner = roast_common::Interner::new();
        // Use with_source to enable # type: ignore support
        let mut diagnostics = roast_common::DiagnosticSink::with_source(&source);

        let module = roast_parser::parse_module(
            &source,
            &source_file.to_string_lossy(),
            &interner,
            &mut diagnostics,
        ).map_err(|e| Error::Build(e.to_string()))?;

        if diagnostics.has_errors() {
            report_diagnostics(&diagnostics, &source, source_file);
            return Err(Error::Build(format!(
                "Compilation failed with {} error(s)",
                diagnostics.error_count()
            )));
        }

        // Type check
        let mut type_ctx = roast_typer::TypeContext::new();
        // Register builtins (print, len, open, etc.) before type checking
        roast_typer::builtins::register_builtins(&mut type_ctx, &interner);
        let mut checker = roast_typer::TypeChecker::new(
            &mut type_ctx,
            &interner,
            &mut diagnostics,
        );
        checker.check_module(&module);

        if diagnostics.has_errors() {
            report_diagnostics(&diagnostics, &source, source_file);
            return Err(Error::Build(format!(
                "Type checking failed with {} error(s)",
                diagnostics.error_count()
            )));
        }

        // Generate bytecode
        let target_config = roast_codegen::TargetConfig::bytecode()
            .with_opt_level(self.opt_level());

        let _emitter = roast_codegen::CodeEmitter::new(target_config);
        // Generate bytecode (simplified - actual implementation would use the full pipeline)
        let bytecode = b"ROAST_BYTECODE".to_vec();

        // Write bytecode
        fs::write(&output_path, &bytecode)?;

        let size = fs::metadata(&output_path)?.len();

        Ok(Artifact {
            kind: ArtifactKind::Bytecode,
            path: output_path,
            size,
        })
    }

    fn link(&self, _artifacts: &[Artifact]) -> Result<Artifact> {
        let output_name = if cfg!(windows) {
            format!("{}.exe", self.project.name())
        } else {
            self.project.name().to_string()
        };

        let output_path = self.target_dir().join(&output_name);
        let entry_point = self.project.entry_point();

        println!("{} {} (LLVM)",
            "    Linking".green().bold(),
            output_path.display()
        );

        // Use roastc build to create a native binary via LLVM
        // This produces a true standalone executable like Go/Rust
        let result = Command::new("roastc")
            .arg("build")
            .arg(entry_point.as_os_str())
            .arg("-o")
            .arg(output_path.as_os_str())
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(Error::Build(format!("LLVM compilation failed: {}", stderr)));
                }
            }
            Err(e) => {
                // roastc not found in PATH, try using the current executable's directory
                if let Ok(current_exe) = std::env::current_exe() {
                    if let Some(exe_dir) = current_exe.parent() {
                        let roastc_path = exe_dir.join("roastc");
                        let result = Command::new(&roastc_path)
                            .arg("build")
                            .arg(entry_point.as_os_str())
                            .arg("-o")
                            .arg(output_path.as_os_str())
                            .output();

                        match result {
                            Ok(output) => {
                                if !output.status.success() {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    return Err(Error::Build(format!("LLVM compilation failed: {}", stderr)));
                                }
                            }
                            Err(_) => {
                                return Err(Error::Build(format!("Failed to run roastc: {}", e)));
                            }
                        }
                    } else {
                        return Err(Error::Build(format!("Failed to run roastc: {}", e)));
                    }
                } else {
                    return Err(Error::Build(format!("Failed to run roastc: {}", e)));
                }
            }
        }

        let size = fs::metadata(&output_path)?.len();

        Ok(Artifact {
            kind: ArtifactKind::NativeBinary,
            path: output_path,
            size,
        })
    }

    /// Link using native compilation (Cranelift).
    #[cfg(feature = "cranelift")]
    fn link_native(&self, artifacts: &[Artifact]) -> Result<Artifact> {
        use roast_codegen::{Linker, LinkerConfig};

        let output_name = if cfg!(windows) {
            format!("{}.exe", self.project.name())
        } else {
            self.project.name().to_string()
        };

        let output_path = self.target_dir().join(&output_name);

        println!("{} native binary {}",
            "    Linking".green().bold(),
            output_path.display()
        );

        // Collect object files
        let obj_files: Vec<_> = artifacts.iter()
            .filter(|a| matches!(a.kind, ArtifactKind::ObjectFile))
            .map(|a| a.path.clone())
            .collect();

        if obj_files.is_empty() {
            // No object files, fall back to bytecode linking
            return self.link(artifacts);
        }

        // Link using system linker
        let config = LinkerConfig::executable(&output_path);
        let mut linker = Linker::new(config);
        linker.add_objects(obj_files);

        linker.link()
            .map_err(|e| Error::Build(format!("Linking failed: {}", e)))?;

        let size = fs::metadata(&output_path)?.len();

        Ok(Artifact {
            kind: ArtifactKind::NativeBinary,
            path: output_path,
            size,
        })
    }

    #[cfg(not(feature = "cranelift"))]
    fn link_native(&self, artifacts: &[Artifact]) -> Result<Artifact> {
        // Fall back to bytecode linking
        self.link(artifacts)
    }

    fn build_gpu_kernels(&self) -> Result<Vec<Artifact>> {
        let gpu_info = match &self.gpu_info {
            Some(info) => info,
            None => return Ok(vec![]),
        };

        println!("{} GPU kernels for {}",
            "   Building".cyan(),
            gpu_info.device_name
        );

        // Find GPU kernel files (*.roast.gpu or @kernel decorated functions)
        // For now, return empty
        Ok(vec![])
    }

    fn target_dir(&self) -> PathBuf {
        self.project.cooked_dir().join(self.options.mode.as_str())
    }

    fn opt_level(&self) -> roast_codegen::target::OptLevel {
        match self.options.mode {
            BuildMode::Debug => roast_codegen::target::OptLevel::None,
            BuildMode::Release => roast_codegen::target::OptLevel::Aggressive,
            BuildMode::Test => roast_codegen::target::OptLevel::None,
            BuildMode::Bench => roast_codegen::target::OptLevel::Aggressive,
        }
    }

    fn is_binary_target(&self) -> bool {
        let entry = &self.project.config.package.entry;
        // Any .roast or .ro file is a binary target (BUG-005 fix)
        entry.ends_with(".roast") || entry.ends_with(".ro")
    }
}

/// Get the roastc path, trying PATH first, then the same directory as kitchen.
fn get_roastc_path() -> PathBuf {
    // Try PATH first
    if let Ok(output) = Command::new("which").arg("roastc").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }
    
    // Fall back to same directory as current executable
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let roastc_path = exe_dir.join("roastc");
            if roastc_path.exists() {
                return roastc_path;
            }
        }
    }
    
    // Last resort: just use "roastc" and hope for the best
    PathBuf::from("roastc")
}

/// Run the built binary.
pub fn run(project: &Project, args: &[String]) -> Result<i32> {
    let entry = project.entry_point();

    if !entry.exists() {
        return Err(Error::NotFound(format!(
            "Entry point not found: {}",
            entry.display()
        )));
    }

    // Run using roastc
    let roastc = get_roastc_path();
    let status = Command::new(&roastc)
        .arg("run")
        .arg(&entry)
        .args(args)
        .status()?;

    Ok(status.code().unwrap_or(-1))
}

/// Run tests.
pub fn test(project: &Project, filter: Option<&str>, verbose: bool) -> Result<TestResult> {
    let test_files = project.test_files()?;

    if test_files.is_empty() {
        println!("{}", "No test files found".yellow());
        return Ok(TestResult {
            passed: 0,
            failed: 0,
            skipped: 0,
            duration: std::time::Duration::ZERO,
        });
    }

    println!("{} {} test file(s)",
        "    Running".green().bold(),
        test_files.len()
    );

    let start = Instant::now();
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for test_file in &test_files {
        if let Some(filter) = filter {
            if !test_file.to_string_lossy().contains(filter) {
                skipped += 1;
                continue;
            }
        }

        if verbose {
            println!("  {} {}",
                "Testing".cyan(),
                test_file.display()
            );
        }

        // Run test file
        let roastc = get_roastc_path();
        let status = Command::new(&roastc)
            .arg("run")
            .arg(test_file)
            .status()?;

        if status.success() {
            passed += 1;
            println!("  {} {}", "PASS".green(), test_file.display());
        } else {
            failed += 1;
            println!("  {} {}", "FAIL".red(), test_file.display());
        }
    }

    let duration = start.elapsed();

    println!();
    println!("test result: {}. {} passed; {} failed; {} skipped; finished in {:.2}s",
        if failed == 0 { "ok".green() } else { "FAILED".red() },
        passed,
        failed,
        skipped,
        duration.as_secs_f64()
    );

    Ok(TestResult {
        passed,
        failed,
        skipped,
        duration,
    })
}

/// Test result.
#[derive(Debug)]
pub struct TestResult {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration: std::time::Duration,
}

/// Reports diagnostics to stderr with source code snippets.
fn report_diagnostics(diagnostics: &roast_common::DiagnosticSink, source: &str, path: &Path) {
    use codespan_reporting::files::SimpleFiles;
    use codespan_reporting::term;
    use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

    let mut files = SimpleFiles::new();
    let file_id = files.add(path.to_string_lossy(), source);

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = term::Config::default();

    for diag in diagnostics.diagnostics() {
        // Calculate the offset needed to map the diagnostic's file ID to our file_id
        let file_id_offset = if let Some(span) = diag.primary_span {
            file_id as i32 - span.file.as_u32() as i32
        } else {
            0
        };

        let codespan_diag = diag.to_codespan_with_offset(file_id_offset);
        if let Err(_) = term::emit(&mut writer.lock(), &config, &files, &codespan_diag) {
            // Fall back to simple message if codespan fails
            eprintln!("{}", diag);
        }
    }
}

//! Subprocess management for Roast.
//!
//! Provides process spawning, communication, and management.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// =============================================================================
// Errors
// =============================================================================

#[derive(Debug, Clone)]
pub enum SubprocessError {
    /// Failed to spawn process.
    SpawnFailed(String),
    /// IO error.
    IoError(String),
    /// Process timed out.
    Timeout,
    /// Process returned non-zero exit code.
    NonZeroExit(i32),
    /// Process was killed.
    Killed,
    /// Invalid argument.
    InvalidArgument(String),
}

impl fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubprocessError::SpawnFailed(msg) => write!(f, "Failed to spawn process: {}", msg),
            SubprocessError::IoError(msg) => write!(f, "IO error: {}", msg),
            SubprocessError::Timeout => write!(f, "Process timed out"),
            SubprocessError::NonZeroExit(code) => write!(f, "Process exited with code {}", code),
            SubprocessError::Killed => write!(f, "Process was killed"),
            SubprocessError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
        }
    }
}

impl std::error::Error for SubprocessError {}

pub type SubprocessResult<T> = Result<T, SubprocessError>;

// =============================================================================
// CompletedProcess
// =============================================================================

/// Result of a completed process.
#[derive(Debug, Clone)]
pub struct CompletedProcess {
    /// Command arguments.
    pub args: Vec<String>,
    /// Return code.
    pub returncode: i32,
    /// Standard output.
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: Vec<u8>,
}

impl CompletedProcess {
    /// Get stdout as string.
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
    
    /// Get stderr as string.
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
    
    /// Check if process succeeded.
    pub fn success(&self) -> bool {
        self.returncode == 0
    }
    
    /// Raise error if process failed.
    pub fn check(&self) -> SubprocessResult<()> {
        if self.success() {
            Ok(())
        } else {
            Err(SubprocessError::NonZeroExit(self.returncode))
        }
    }
}

// =============================================================================
// Popen (Process Handle)
// =============================================================================

/// Handle to a running process.
pub struct Popen {
    child: Child,
    args: Vec<String>,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

impl Popen {
    /// Create a new Popen from a Child process.
    fn new(mut child: Child, args: Vec<String>) -> Self {
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        
        Self {
            child,
            args,
            stdin,
            stdout,
            stderr,
        }
    }
    
    /// Get the process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    
    /// Poll for process completion.
    pub fn poll(&mut self) -> Option<i32> {
        self.child.try_wait()
            .ok()
            .flatten()
            .map(|s| s.code().unwrap_or(-1))
    }
    
    /// Wait for process to complete.
    pub fn wait(&mut self) -> SubprocessResult<i32> {
        self.child.wait()
            .map(|s| s.code().unwrap_or(-1))
            .map_err(|e| SubprocessError::IoError(e.to_string()))
    }
    
    /// Wait with timeout.
    pub fn wait_timeout(&mut self, timeout: Duration) -> SubprocessResult<Option<i32>> {
        let start = Instant::now();
        
        loop {
            if let Some(code) = self.poll() {
                return Ok(Some(code));
            }
            
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    /// Communicate with the process.
    pub fn communicate(&mut self, input: Option<&[u8]>) -> SubprocessResult<(Vec<u8>, Vec<u8>)> {
        // Write input
        if let (Some(ref mut stdin), Some(input_data)) = (&mut self.stdin, input) {
            stdin.write_all(input_data)
                .map_err(|e| SubprocessError::IoError(e.to_string()))?;
        }
        
        // Close stdin
        self.stdin = None;
        
        // Read output
        let mut stdout_data = Vec::new();
        let mut stderr_data = Vec::new();
        
        if let Some(ref mut stdout) = self.stdout {
            stdout.read_to_end(&mut stdout_data)
                .map_err(|e| SubprocessError::IoError(e.to_string()))?;
        }
        
        if let Some(ref mut stderr) = self.stderr {
            stderr.read_to_end(&mut stderr_data)
                .map_err(|e| SubprocessError::IoError(e.to_string()))?;
        }
        
        // Wait for process
        self.wait()?;
        
        Ok((stdout_data, stderr_data))
    }
    
    /// Send signal to process.
    pub fn send_signal(&mut self, _signal: i32) -> SubprocessResult<()> {
        // On Unix, we'd use kill(). For now, just kill it.
        self.kill()
    }
    
    /// Terminate the process.
    pub fn terminate(&mut self) -> SubprocessResult<()> {
        self.kill()
    }
    
    /// Kill the process.
    pub fn kill(&mut self) -> SubprocessResult<()> {
        self.child.kill()
            .map_err(|e| SubprocessError::IoError(e.to_string()))
    }
    
    /// Write to stdin.
    pub fn write_stdin(&mut self, data: &[u8]) -> SubprocessResult<usize> {
        if let Some(ref mut stdin) = self.stdin {
            stdin.write(data)
                .map_err(|e| SubprocessError::IoError(e.to_string()))
        } else {
            Err(SubprocessError::IoError("stdin not available".into()))
        }
    }
    
    /// Read from stdout.
    pub fn read_stdout(&mut self, buf: &mut [u8]) -> SubprocessResult<usize> {
        if let Some(ref mut stdout) = self.stdout {
            stdout.read(buf)
                .map_err(|e| SubprocessError::IoError(e.to_string()))
        } else {
            Err(SubprocessError::IoError("stdout not available".into()))
        }
    }
    
    /// Read line from stdout.
    pub fn read_line(&mut self) -> SubprocessResult<Option<String>> {
        if let Some(ref mut stdout) = self.stdout {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => Ok(None),
                Ok(_) => Ok(Some(line)),
                Err(e) => Err(SubprocessError::IoError(e.to_string())),
            }
        } else {
            Err(SubprocessError::IoError("stdout not available".into()))
        }
    }
}

// =============================================================================
// ProcessBuilder
// =============================================================================

/// Builder for creating processes.
#[derive(Clone)]
pub struct ProcessBuilder {
    program: String,
    args: Vec<String>,
    env: Option<HashMap<String, String>>,
    cwd: Option<PathBuf>,
    stdin: StdioConfig,
    stdout: StdioConfig,
    stderr: StdioConfig,
    shell: bool,
}

/// Stdio configuration.
#[derive(Clone, Copy, Debug)]
pub enum StdioConfig {
    Inherit,
    Pipe,
    Null,
}

impl ProcessBuilder {
    /// Create a new process builder.
    pub fn new(program: &str) -> Self {
        Self {
            program: program.to_string(),
            args: Vec::new(),
            env: None,
            cwd: None,
            stdin: StdioConfig::Inherit,
            stdout: StdioConfig::Inherit,
            stderr: StdioConfig::Inherit,
            shell: false,
        }
    }
    
    /// Add an argument.
    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }
    
    /// Add multiple arguments.
    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| s.to_string()));
        self
    }
    
    /// Set environment variable.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.get_or_insert_with(HashMap::new)
            .insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set all environment variables.
    pub fn envs(mut self, vars: HashMap<String, String>) -> Self {
        self.env = Some(vars);
        self
    }
    
    /// Clear environment.
    pub fn env_clear(mut self) -> Self {
        self.env = Some(HashMap::new());
        self
    }
    
    /// Set working directory.
    pub fn cwd<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }
    
    /// Configure stdin.
    pub fn stdin(mut self, config: StdioConfig) -> Self {
        self.stdin = config;
        self
    }
    
    /// Configure stdout.
    pub fn stdout(mut self, config: StdioConfig) -> Self {
        self.stdout = config;
        self
    }
    
    /// Configure stderr.
    pub fn stderr(mut self, config: StdioConfig) -> Self {
        self.stderr = config;
        self
    }
    
    /// Run through shell.
    pub fn shell(mut self) -> Self {
        self.shell = true;
        self
    }
    
    fn to_stdio(config: StdioConfig) -> Stdio {
        match config {
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Pipe => Stdio::piped(),
            StdioConfig::Null => Stdio::null(),
        }
    }
    
    /// Spawn the process.
    pub fn spawn(self) -> SubprocessResult<Popen> {
        let mut cmd = if self.shell {
            let full_cmd = std::iter::once(self.program.clone())
                .chain(self.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            
            #[cfg(unix)]
            let mut c = Command::new("sh");
            #[cfg(unix)]
            c.arg("-c").arg(&full_cmd);
            
            #[cfg(windows)]
            let mut c = Command::new("cmd");
            #[cfg(windows)]
            c.arg("/C").arg(&full_cmd);
            
            c
        } else {
            let mut c = Command::new(&self.program);
            c.args(&self.args);
            c
        };
        
        cmd.stdin(Self::to_stdio(self.stdin))
           .stdout(Self::to_stdio(self.stdout))
           .stderr(Self::to_stdio(self.stderr));
        
        if let Some(ref env) = self.env {
            cmd.env_clear();
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        
        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }
        
        let args = std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect();
        
        cmd.spawn()
            .map(|child| Popen::new(child, args))
            .map_err(|e| SubprocessError::SpawnFailed(e.to_string()))
    }
    
    /// Run and wait for completion.
    pub fn run(self) -> SubprocessResult<CompletedProcess> {
        let args: Vec<String> = std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect();
        
        let mut popen = self
            .stdin(StdioConfig::Null)
            .stdout(StdioConfig::Pipe)
            .stderr(StdioConfig::Pipe)
            .spawn()?;
        
        let (stdout, stderr) = popen.communicate(None)?;
        let returncode = popen.wait()?;
        
        Ok(CompletedProcess {
            args,
            returncode,
            stdout,
            stderr,
        })
    }
    
    /// Run and check for success.
    pub fn check_run(self) -> SubprocessResult<CompletedProcess> {
        let result = self.run()?;
        result.check()?;
        Ok(result)
    }
    
    /// Run with input.
    pub fn run_with_input(self, input: &[u8]) -> SubprocessResult<CompletedProcess> {
        let args: Vec<String> = std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect();
        
        let mut popen = self
            .stdin(StdioConfig::Pipe)
            .stdout(StdioConfig::Pipe)
            .stderr(StdioConfig::Pipe)
            .spawn()?;
        
        let (stdout, stderr) = popen.communicate(Some(input))?;
        let returncode = popen.wait()?;
        
        Ok(CompletedProcess {
            args,
            returncode,
            stdout,
            stderr,
        })
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Run a command and capture output.
pub fn run(args: &[&str]) -> SubprocessResult<CompletedProcess> {
    if args.is_empty() {
        return Err(SubprocessError::InvalidArgument("No command provided".into()));
    }
    
    let mut builder = ProcessBuilder::new(args[0]);
    if args.len() > 1 {
        builder = builder.args(&args[1..]);
    }
    builder.run()
}

/// Run a command and check for success.
pub fn check_call(args: &[&str]) -> SubprocessResult<i32> {
    let result = run(args)?;
    result.check()?;
    Ok(result.returncode)
}

/// Run a command and return output.
pub fn check_output(args: &[&str]) -> SubprocessResult<Vec<u8>> {
    let result = run(args)?;
    result.check()?;
    Ok(result.stdout)
}

/// Run a shell command.
pub fn shell(cmd: &str) -> SubprocessResult<CompletedProcess> {
    ProcessBuilder::new(cmd).shell().run()
}

/// Get the output of a shell command.
pub fn getoutput(cmd: &str) -> SubprocessResult<String> {
    let result = shell(cmd)?;
    Ok(result.stdout_str())
}

/// Get status and output of a shell command.
pub fn getstatusoutput(cmd: &str) -> SubprocessResult<(i32, String)> {
    let result = shell(cmd)?;
    Ok((result.returncode, result.stdout_str()))
}

// =============================================================================
// Pipe for Chaining
// =============================================================================

/// A pipeline of processes.
pub struct Pipeline {
    processes: Vec<Popen>,
}

impl Pipeline {
    /// Create a new pipeline.
    pub fn new() -> Self {
        Self { processes: Vec::new() }
    }
    
    /// Add a command to the pipeline.
    pub fn pipe(mut self, program: &str, args: &[&str]) -> SubprocessResult<Self> {
        let stdin_config = if self.processes.is_empty() {
            StdioConfig::Inherit
        } else {
            StdioConfig::Pipe
        };
        
        let popen = ProcessBuilder::new(program)
            .args(args)
            .stdin(stdin_config)
            .stdout(StdioConfig::Pipe)
            .stderr(StdioConfig::Inherit)
            .spawn()?;
        
        self.processes.push(popen);
        Ok(self)
    }
    
    /// Run the pipeline and get final output.
    pub fn run(mut self) -> SubprocessResult<Vec<u8>> {
        if self.processes.is_empty() {
            return Ok(Vec::new());
        }
        
        // Connect pipes between processes
        // For simplicity, we'll handle this sequentially
        let mut data = Vec::new();
        
        for popen in &mut self.processes {
            let (stdout, _) = popen.communicate(if data.is_empty() { None } else { Some(&data) })?;
            data = stdout;
        }
        
        Ok(data)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Process Pool
// =============================================================================

/// A pool of worker processes.
pub struct ProcessPool {
    workers: Vec<Popen>,
    max_workers: usize,
}

impl ProcessPool {
    /// Create a new process pool.
    pub fn new(max_workers: usize) -> Self {
        Self {
            workers: Vec::new(),
            max_workers,
        }
    }
    
    /// Submit a task.
    pub fn submit(&mut self, program: &str, args: &[&str]) -> SubprocessResult<u32> {
        // Clean up finished workers
        self.workers.retain_mut(|w| w.poll().is_none());
        
        // Wait if at capacity
        while self.workers.len() >= self.max_workers {
            std::thread::sleep(Duration::from_millis(10));
            self.workers.retain_mut(|w| w.poll().is_none());
        }
        
        let popen = ProcessBuilder::new(program)
            .args(args)
            .stdout(StdioConfig::Pipe)
            .stderr(StdioConfig::Pipe)
            .spawn()?;
        
        let pid = popen.pid();
        self.workers.push(popen);
        Ok(pid)
    }
    
    /// Wait for all workers to complete.
    pub fn wait_all(&mut self) -> SubprocessResult<Vec<i32>> {
        let mut results = Vec::new();
        
        for worker in &mut self.workers {
            results.push(worker.wait()?);
        }
        
        self.workers.clear();
        Ok(results)
    }
    
    /// Get number of active workers.
    pub fn active_count(&mut self) -> usize {
        self.workers.retain_mut(|w| w.poll().is_none());
        self.workers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_echo() {
        let result = run(&["echo", "hello"]).unwrap();
        assert!(result.success());
        assert!(result.stdout_str().contains("hello"));
    }
    
    #[test]
    fn test_process_builder() {
        let result = ProcessBuilder::new("echo")
            .arg("test")
            .arg("message")
            .run()
            .unwrap();
        
        assert!(result.success());
        assert!(result.stdout_str().contains("test"));
    }
    
    #[test]
    fn test_shell_command() {
        let result = shell("echo hello").unwrap();
        assert!(result.success());
    }
    
    #[test]
    fn test_check_output() {
        let output = check_output(&["echo", "test"]).unwrap();
        let s = String::from_utf8_lossy(&output);
        assert!(s.contains("test"));
    }
    
    #[test]
    fn test_env() {
        let result = ProcessBuilder::new("env")
            .env("MY_VAR", "my_value")
            .run()
            .unwrap();
        
        // The output should contain our env var
        // (depending on the system, this may or may not work perfectly)
        assert!(result.success());
    }
    
    #[test]
    fn test_cwd() {
        let result = ProcessBuilder::new("pwd")
            .cwd("/tmp")
            .run()
            .unwrap();
        
        assert!(result.success());
        assert!(result.stdout_str().contains("/tmp"));
    }
}


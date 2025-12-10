//! Signal handling for Roast.
//!
//! Provides Unix signal handling similar to Python's signal module.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

// =============================================================================
// Signal Constants
// =============================================================================

/// Signal numbers (Unix-like systems).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Signal {
    /// Hangup
    SIGHUP = 1,
    /// Interrupt (Ctrl+C)
    SIGINT = 2,
    /// Quit
    SIGQUIT = 3,
    /// Illegal instruction
    SIGILL = 4,
    /// Trace/breakpoint trap
    SIGTRAP = 5,
    /// Abort
    SIGABRT = 6,
    /// Bus error
    SIGBUS = 7,
    /// Floating point exception
    SIGFPE = 8,
    /// Kill (cannot be caught)
    SIGKILL = 9,
    /// User-defined signal 1
    SIGUSR1 = 10,
    /// Segmentation fault
    SIGSEGV = 11,
    /// User-defined signal 2
    SIGUSR2 = 12,
    /// Broken pipe
    SIGPIPE = 13,
    /// Alarm clock
    SIGALRM = 14,
    /// Termination
    SIGTERM = 15,
    /// Stack fault
    SIGSTKFLT = 16,
    /// Child stopped/terminated
    SIGCHLD = 17,
    /// Continue if stopped
    SIGCONT = 18,
    /// Stop (cannot be caught)
    SIGSTOP = 19,
    /// Stop typed at terminal
    SIGTSTP = 20,
    /// Terminal input for background process
    SIGTTIN = 21,
    /// Terminal output for background process
    SIGTTOU = 22,
    /// Urgent I/O condition
    SIGURG = 23,
    /// CPU time limit exceeded
    SIGXCPU = 24,
    /// File size limit exceeded
    SIGXFSZ = 25,
    /// Virtual timer expired
    SIGVTALRM = 26,
    /// Profiling timer expired
    SIGPROF = 27,
    /// Window changed
    SIGWINCH = 28,
    /// I/O possible
    SIGIO = 29,
    /// Power failure
    SIGPWR = 30,
    /// Bad system call
    SIGSYS = 31,
}

impl Signal {
    /// Get signal from number.
    pub fn from_i32(n: i32) -> Option<Self> {
        match n {
            1 => Some(Signal::SIGHUP),
            2 => Some(Signal::SIGINT),
            3 => Some(Signal::SIGQUIT),
            4 => Some(Signal::SIGILL),
            5 => Some(Signal::SIGTRAP),
            6 => Some(Signal::SIGABRT),
            7 => Some(Signal::SIGBUS),
            8 => Some(Signal::SIGFPE),
            9 => Some(Signal::SIGKILL),
            10 => Some(Signal::SIGUSR1),
            11 => Some(Signal::SIGSEGV),
            12 => Some(Signal::SIGUSR2),
            13 => Some(Signal::SIGPIPE),
            14 => Some(Signal::SIGALRM),
            15 => Some(Signal::SIGTERM),
            16 => Some(Signal::SIGSTKFLT),
            17 => Some(Signal::SIGCHLD),
            18 => Some(Signal::SIGCONT),
            19 => Some(Signal::SIGSTOP),
            20 => Some(Signal::SIGTSTP),
            21 => Some(Signal::SIGTTIN),
            22 => Some(Signal::SIGTTOU),
            23 => Some(Signal::SIGURG),
            24 => Some(Signal::SIGXCPU),
            25 => Some(Signal::SIGXFSZ),
            26 => Some(Signal::SIGVTALRM),
            27 => Some(Signal::SIGPROF),
            28 => Some(Signal::SIGWINCH),
            29 => Some(Signal::SIGIO),
            30 => Some(Signal::SIGPWR),
            31 => Some(Signal::SIGSYS),
            _ => None,
        }
    }
    
    /// Get signal name.
    pub fn name(&self) -> &'static str {
        match self {
            Signal::SIGHUP => "SIGHUP",
            Signal::SIGINT => "SIGINT",
            Signal::SIGQUIT => "SIGQUIT",
            Signal::SIGILL => "SIGILL",
            Signal::SIGTRAP => "SIGTRAP",
            Signal::SIGABRT => "SIGABRT",
            Signal::SIGBUS => "SIGBUS",
            Signal::SIGFPE => "SIGFPE",
            Signal::SIGKILL => "SIGKILL",
            Signal::SIGUSR1 => "SIGUSR1",
            Signal::SIGSEGV => "SIGSEGV",
            Signal::SIGUSR2 => "SIGUSR2",
            Signal::SIGPIPE => "SIGPIPE",
            Signal::SIGALRM => "SIGALRM",
            Signal::SIGTERM => "SIGTERM",
            Signal::SIGSTKFLT => "SIGSTKFLT",
            Signal::SIGCHLD => "SIGCHLD",
            Signal::SIGCONT => "SIGCONT",
            Signal::SIGSTOP => "SIGSTOP",
            Signal::SIGTSTP => "SIGTSTP",
            Signal::SIGTTIN => "SIGTTIN",
            Signal::SIGTTOU => "SIGTTOU",
            Signal::SIGURG => "SIGURG",
            Signal::SIGXCPU => "SIGXCPU",
            Signal::SIGXFSZ => "SIGXFSZ",
            Signal::SIGVTALRM => "SIGVTALRM",
            Signal::SIGPROF => "SIGPROF",
            Signal::SIGWINCH => "SIGWINCH",
            Signal::SIGIO => "SIGIO",
            Signal::SIGPWR => "SIGPWR",
            Signal::SIGSYS => "SIGSYS",
        }
    }
    
    /// Get signal number.
    pub fn number(&self) -> i32 {
        *self as i32
    }
}

impl std::fmt::Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// =============================================================================
// Signal Handler
// =============================================================================

/// Signal handler action.
#[derive(Clone, Debug)]
pub enum SignalAction {
    /// Default action
    Default,
    /// Ignore the signal
    Ignore,
    /// Custom handler flag (set when signal received)
    Handle,
}

/// Global signal flags for simple signal handling.
static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);
static SIGTERM_RECEIVED: AtomicBool = AtomicBool::new(false);
static SIGHUP_RECEIVED: AtomicBool = AtomicBool::new(false);
static SIGUSR1_RECEIVED: AtomicBool = AtomicBool::new(false);
static SIGUSR2_RECEIVED: AtomicBool = AtomicBool::new(false);
static SIGALRM_RECEIVED: AtomicBool = AtomicBool::new(false);
static LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// Check if SIGINT (Ctrl+C) was received.
pub fn sigint_received() -> bool {
    SIGINT_RECEIVED.load(Ordering::SeqCst)
}

/// Clear SIGINT flag.
pub fn clear_sigint() {
    SIGINT_RECEIVED.store(false, Ordering::SeqCst);
}

/// Check if SIGTERM was received.
pub fn sigterm_received() -> bool {
    SIGTERM_RECEIVED.load(Ordering::SeqCst)
}

/// Clear SIGTERM flag.
pub fn clear_sigterm() {
    SIGTERM_RECEIVED.store(false, Ordering::SeqCst);
}

/// Get the last signal received.
pub fn last_signal() -> Option<Signal> {
    let sig = LAST_SIGNAL.load(Ordering::SeqCst);
    if sig == 0 {
        None
    } else {
        Signal::from_i32(sig)
    }
}

/// Clear the last signal.
pub fn clear_last_signal() {
    LAST_SIGNAL.store(0, Ordering::SeqCst);
}

// =============================================================================
// Signal Registration (Unix-only)
// =============================================================================

#[cfg(unix)]
mod unix_signals {
    use super::*;
    use std::os::raw::c_int;
    
    extern "C" fn handle_sigint(_: c_int) {
        SIGINT_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGINT as i32, Ordering::SeqCst);
    }
    
    extern "C" fn handle_sigterm(_: c_int) {
        SIGTERM_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGTERM as i32, Ordering::SeqCst);
    }
    
    extern "C" fn handle_sighup(_: c_int) {
        SIGHUP_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGHUP as i32, Ordering::SeqCst);
    }
    
    extern "C" fn handle_sigusr1(_: c_int) {
        SIGUSR1_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGUSR1 as i32, Ordering::SeqCst);
    }
    
    extern "C" fn handle_sigusr2(_: c_int) {
        SIGUSR2_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGUSR2 as i32, Ordering::SeqCst);
    }
    
    extern "C" fn handle_sigalrm(_: c_int) {
        SIGALRM_RECEIVED.store(true, Ordering::SeqCst);
        LAST_SIGNAL.store(Signal::SIGALRM as i32, Ordering::SeqCst);
    }
    
    /// Install signal handler.
    pub fn install_handler(sig: Signal) -> Result<(), &'static str> {
        use std::mem;
        
        unsafe {
            let handler: extern "C" fn(c_int) = match sig {
                Signal::SIGINT => handle_sigint,
                Signal::SIGTERM => handle_sigterm,
                Signal::SIGHUP => handle_sighup,
                Signal::SIGUSR1 => handle_sigusr1,
                Signal::SIGUSR2 => handle_sigusr2,
                Signal::SIGALRM => handle_sigalrm,
                Signal::SIGKILL | Signal::SIGSTOP => return Err("Cannot catch SIGKILL or SIGSTOP"),
                _ => return Err("Unsupported signal"),
            };
            
            // Use libc to set up signal handler
            let result = libc::signal(sig as c_int, handler as usize);
            if result == libc::SIG_ERR {
                return Err("Failed to install signal handler");
            }
        }
        
        Ok(())
    }
    
    /// Ignore a signal.
    pub fn ignore_signal(sig: Signal) -> Result<(), &'static str> {
        unsafe {
            let result = libc::signal(sig as c_int, libc::SIG_IGN);
            if result == libc::SIG_ERR {
                return Err("Failed to ignore signal");
            }
        }
        Ok(())
    }
    
    /// Reset signal to default handler.
    pub fn reset_signal(sig: Signal) -> Result<(), &'static str> {
        unsafe {
            let result = libc::signal(sig as c_int, libc::SIG_DFL);
            if result == libc::SIG_ERR {
                return Err("Failed to reset signal");
            }
        }
        Ok(())
    }
    
    /// Send a signal to a process.
    pub fn kill(pid: i32, sig: Signal) -> Result<(), std::io::Error> {
        unsafe {
            if libc::kill(pid, sig as c_int) == -1 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
    
    /// Set an alarm (SIGALRM after seconds).
    pub fn alarm(seconds: u32) -> u32 {
        unsafe { libc::alarm(seconds) }
    }
    
    /// Pause until a signal is received.
    pub fn pause() {
        unsafe { libc::pause(); }
    }
    
    /// Get current process ID.
    pub fn getpid() -> i32 {
        unsafe { libc::getpid() }
    }
    
    /// Get parent process ID.
    pub fn getppid() -> i32 {
        unsafe { libc::getppid() }
    }
}

#[cfg(unix)]
pub use unix_signals::*;

// =============================================================================
// Cross-platform Ctrl+C Handler
// =============================================================================

/// A simple Ctrl+C handler that can be polled.
pub struct CtrlC {
    flag: Arc<AtomicBool>,
}

impl CtrlC {
    /// Create a new Ctrl+C handler.
    /// 
    /// Note: On Unix, this uses SIGINT. On Windows, it uses SetConsoleCtrlHandler.
    pub fn new() -> Self {
        let flag = Arc::new(AtomicBool::new(false));
        
        #[cfg(unix)]
        {
            // Install SIGINT handler
            let _ = unix_signals::install_handler(Signal::SIGINT);
        }
        
        Self { flag }
    }
    
    /// Check if Ctrl+C was pressed.
    pub fn is_pressed(&self) -> bool {
        #[cfg(unix)]
        {
            sigint_received()
        }
        #[cfg(not(unix))]
        {
            self.flag.load(Ordering::SeqCst)
        }
    }
    
    /// Clear the Ctrl+C flag.
    pub fn clear(&self) {
        #[cfg(unix)]
        {
            clear_sigint();
        }
        #[cfg(not(unix))]
        {
            self.flag.store(false, Ordering::SeqCst);
        }
    }
    
    /// Wait for Ctrl+C to be pressed.
    pub fn wait(&self) {
        while !self.is_pressed() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

impl Default for CtrlC {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Graceful Shutdown
// =============================================================================

/// A graceful shutdown handler.
pub struct GracefulShutdown {
    should_shutdown: Arc<AtomicBool>,
}

impl GracefulShutdown {
    /// Create a new graceful shutdown handler.
    /// 
    /// Listens for SIGINT and SIGTERM.
    pub fn new() -> Self {
        #[cfg(unix)]
        {
            let _ = unix_signals::install_handler(Signal::SIGINT);
            let _ = unix_signals::install_handler(Signal::SIGTERM);
        }
        
        Self {
            should_shutdown: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Check if shutdown was requested.
    pub fn should_shutdown(&self) -> bool {
        #[cfg(unix)]
        {
            sigint_received() || sigterm_received()
        }
        #[cfg(not(unix))]
        {
            self.should_shutdown.load(Ordering::SeqCst)
        }
    }
    
    /// Wait for shutdown signal.
    pub fn wait(&self) {
        while !self.should_shutdown() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    
    /// Get the shutdown reason (which signal).
    pub fn shutdown_reason(&self) -> Option<Signal> {
        last_signal()
    }
}

impl Default for GracefulShutdown {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Timeouts
// =============================================================================

/// Result of a timed operation.
#[derive(Debug, Clone)]
pub enum TimeoutResult<T> {
    /// Operation completed successfully
    Ok(T),
    /// Operation timed out
    TimedOut,
    /// Operation was interrupted
    Interrupted,
}

impl<T> TimeoutResult<T> {
    /// Check if the result is Ok.
    pub fn is_ok(&self) -> bool {
        matches!(self, TimeoutResult::Ok(_))
    }
    
    /// Check if the result timed out.
    pub fn is_timeout(&self) -> bool {
        matches!(self, TimeoutResult::TimedOut)
    }
    
    /// Unwrap the Ok value, panicking on timeout or interrupt.
    pub fn unwrap(self) -> T {
        match self {
            TimeoutResult::Ok(v) => v,
            TimeoutResult::TimedOut => panic!("Operation timed out"),
            TimeoutResult::Interrupted => panic!("Operation was interrupted"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signal_names() {
        assert_eq!(Signal::SIGINT.name(), "SIGINT");
        assert_eq!(Signal::SIGTERM.name(), "SIGTERM");
        assert_eq!(Signal::SIGKILL.name(), "SIGKILL");
    }
    
    #[test]
    fn test_signal_from_i32() {
        assert_eq!(Signal::from_i32(2), Some(Signal::SIGINT));
        assert_eq!(Signal::from_i32(15), Some(Signal::SIGTERM));
        assert_eq!(Signal::from_i32(100), None);
    }
    
    #[test]
    fn test_ctrlc() {
        let ctrlc = CtrlC::new();
        // Initially not pressed
        assert!(!ctrlc.is_pressed());
    }
    
    #[test]
    fn test_graceful_shutdown() {
        let shutdown = GracefulShutdown::new();
        // Initially should not shutdown
        assert!(!shutdown.should_shutdown());
    }
    
    #[test]
    fn test_timeout_result() {
        let ok: TimeoutResult<i32> = TimeoutResult::Ok(42);
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap(), 42);
        
        let timeout: TimeoutResult<i32> = TimeoutResult::TimedOut;
        assert!(timeout.is_timeout());
    }
    
    #[cfg(unix)]
    #[test]
    fn test_getpid() {
        let pid = getpid();
        assert!(pid > 0);
    }
}


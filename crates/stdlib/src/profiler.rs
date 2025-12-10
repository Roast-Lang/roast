//! Profiling utilities.
//!
//! CPU and memory profiling for performance analysis.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// =============================================================================
// CPU Profiler
// =============================================================================

/// CPU profiling statistics.
#[derive(Debug, Clone)]
pub struct ProfileStats {
    /// Function name.
    pub name: String,
    /// Number of calls.
    pub calls: u64,
    /// Total time spent.
    pub total_time: Duration,
    /// Time excluding subcalls.
    pub self_time: Duration,
    /// Average time per call.
    pub avg_time: Duration,
    /// Maximum single call time.
    pub max_time: Duration,
    /// Minimum single call time.
    pub min_time: Duration,
}

impl ProfileStats {
    fn new(name: String) -> Self {
        Self {
            name,
            calls: 0,
            total_time: Duration::ZERO,
            self_time: Duration::ZERO,
            avg_time: Duration::ZERO,
            max_time: Duration::ZERO,
            min_time: Duration::MAX,
        }
    }
    
    fn record(&mut self, duration: Duration) {
        self.calls += 1;
        self.total_time += duration;
        self.avg_time = self.total_time / self.calls as u32;
        self.max_time = self.max_time.max(duration);
        self.min_time = self.min_time.min(duration);
    }
}

/// CPU profiler.
pub struct CpuProfiler {
    /// Stats by function name.
    stats: Mutex<HashMap<String, ProfileStats>>,
    /// Current call stack.
    stack: Mutex<Vec<(String, Instant)>>,
    /// Whether profiling is enabled.
    enabled: std::sync::atomic::AtomicBool,
}

impl CpuProfiler {
    /// Create a new profiler.
    pub fn new() -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            stack: Mutex::new(Vec::new()),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }
    
    /// Enable profiling.
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }
    
    /// Disable profiling.
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }
    
    /// Check if profiling is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
    
    /// Enter a function.
    pub fn enter(&self, name: &str) {
        if !self.is_enabled() {
            return;
        }
        
        let mut stack = self.stack.lock().unwrap();
        stack.push((name.to_string(), Instant::now()));
    }
    
    /// Exit a function.
    pub fn exit(&self, name: &str) {
        if !self.is_enabled() {
            return;
        }
        
        let mut stack = self.stack.lock().unwrap();
        
        if let Some((fname, start)) = stack.pop() {
            if fname != name {
                // Mismatch - try to recover
                stack.push((fname, start));
                return;
            }
            
            let duration = start.elapsed();
            drop(stack); // Release lock
            
            let mut stats = self.stats.lock().unwrap();
            stats.entry(name.to_string())
                .or_insert_with(|| ProfileStats::new(name.to_string()))
                .record(duration);
        }
    }
    
    /// Profile a closure.
    pub fn profile<F, R>(&self, name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.enter(name);
        let result = f();
        self.exit(name);
        result
    }
    
    /// Get statistics.
    pub fn stats(&self) -> Vec<ProfileStats> {
        self.stats.lock().unwrap().values().cloned().collect()
    }
    
    /// Get statistics sorted by total time.
    pub fn stats_by_time(&self) -> Vec<ProfileStats> {
        let mut stats = self.stats();
        stats.sort_by(|a, b| b.total_time.cmp(&a.total_time));
        stats
    }
    
    /// Get statistics sorted by call count.
    pub fn stats_by_calls(&self) -> Vec<ProfileStats> {
        let mut stats = self.stats();
        stats.sort_by(|a, b| b.calls.cmp(&a.calls));
        stats
    }
    
    /// Clear all statistics.
    pub fn clear(&self) {
        self.stats.lock().unwrap().clear();
        self.stack.lock().unwrap().clear();
    }
    
    /// Print a report.
    pub fn print_report(&self) {
        println!("\n{}", "=".repeat(80));
        println!("CPU Profile Report");
        println!("{}", "=".repeat(80));
        println!(
            "{:<40} {:>10} {:>12} {:>12} {:>12}",
            "Function", "Calls", "Total (ms)", "Avg (μs)", "Max (μs)"
        );
        println!("{}", "-".repeat(80));
        
        for stat in self.stats_by_time() {
            println!(
                "{:<40} {:>10} {:>12.2} {:>12.2} {:>12.2}",
                truncate(&stat.name, 40),
                stat.calls,
                stat.total_time.as_secs_f64() * 1000.0,
                stat.avg_time.as_secs_f64() * 1_000_000.0,
                stat.max_time.as_secs_f64() * 1_000_000.0,
            );
        }
        
        println!("{}", "=".repeat(80));
    }
}

impl Default for CpuProfiler {
    fn default() -> Self {
        Self::new()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Profile guard - automatically records when dropped.
pub struct ProfileGuard<'a> {
    profiler: &'a CpuProfiler,
    name: String,
}

impl<'a> ProfileGuard<'a> {
    pub fn new(profiler: &'a CpuProfiler, name: &str) -> Self {
        profiler.enter(name);
        Self {
            profiler,
            name: name.to_string(),
        }
    }
}

impl Drop for ProfileGuard<'_> {
    fn drop(&mut self) {
        self.profiler.exit(&self.name);
    }
}

// =============================================================================
// Memory Profiler
// =============================================================================

/// Memory allocation info.
#[derive(Debug, Clone)]
pub struct AllocationInfo {
    /// Allocation site.
    pub site: String,
    /// Number of allocations.
    pub alloc_count: u64,
    /// Number of deallocations.
    pub dealloc_count: u64,
    /// Total bytes allocated.
    pub total_bytes: u64,
    /// Current live bytes.
    pub live_bytes: i64,
    /// Peak bytes.
    pub peak_bytes: u64,
}

impl AllocationInfo {
    fn new(site: String) -> Self {
        Self {
            site,
            alloc_count: 0,
            dealloc_count: 0,
            total_bytes: 0,
            live_bytes: 0,
            peak_bytes: 0,
        }
    }
}

/// Memory profiler.
pub struct MemoryProfiler {
    /// Allocations by site.
    allocations: Mutex<HashMap<String, AllocationInfo>>,
    /// Global counters.
    total_alloc: AtomicU64,
    total_dealloc: AtomicU64,
    current_bytes: AtomicU64,
    peak_bytes: AtomicU64,
    /// Enabled flag.
    enabled: std::sync::atomic::AtomicBool,
}

impl MemoryProfiler {
    /// Create a new memory profiler.
    pub fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
            total_alloc: AtomicU64::new(0),
            total_dealloc: AtomicU64::new(0),
            current_bytes: AtomicU64::new(0),
            peak_bytes: AtomicU64::new(0),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }
    
    /// Record an allocation.
    pub fn alloc(&self, site: &str, bytes: usize) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        
        let bytes = bytes as u64;
        
        self.total_alloc.fetch_add(1, Ordering::Relaxed);
        let current = self.current_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        
        // Update peak
        let mut peak = self.peak_bytes.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_bytes.compare_exchange_weak(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
        
        let mut allocs = self.allocations.lock().unwrap();
        let info = allocs.entry(site.to_string())
            .or_insert_with(|| AllocationInfo::new(site.to_string()));
        
        info.alloc_count += 1;
        info.total_bytes += bytes;
        info.live_bytes += bytes as i64;
        info.peak_bytes = info.peak_bytes.max(info.live_bytes as u64);
    }
    
    /// Record a deallocation.
    pub fn dealloc(&self, site: &str, bytes: usize) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        
        let bytes = bytes as u64;
        
        self.total_dealloc.fetch_add(1, Ordering::Relaxed);
        self.current_bytes.fetch_sub(bytes, Ordering::Relaxed);
        
        let mut allocs = self.allocations.lock().unwrap();
        if let Some(info) = allocs.get_mut(site) {
            info.dealloc_count += 1;
            info.live_bytes -= bytes as i64;
        }
    }
    
    /// Get current memory usage.
    pub fn current_usage(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }
    
    /// Get peak memory usage.
    pub fn peak_usage(&self) -> u64 {
        self.peak_bytes.load(Ordering::Relaxed)
    }
    
    /// Get total allocations.
    pub fn total_allocations(&self) -> u64 {
        self.total_alloc.load(Ordering::Relaxed)
    }
    
    /// Get allocation info by site.
    pub fn allocations(&self) -> Vec<AllocationInfo> {
        self.allocations.lock().unwrap().values().cloned().collect()
    }
    
    /// Get allocations sorted by total bytes.
    pub fn allocations_by_bytes(&self) -> Vec<AllocationInfo> {
        let mut allocs = self.allocations();
        allocs.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
        allocs
    }
    
    /// Clear all statistics.
    pub fn clear(&self) {
        self.allocations.lock().unwrap().clear();
        self.total_alloc.store(0, Ordering::SeqCst);
        self.total_dealloc.store(0, Ordering::SeqCst);
        self.current_bytes.store(0, Ordering::SeqCst);
        self.peak_bytes.store(0, Ordering::SeqCst);
    }
    
    /// Print a report.
    pub fn print_report(&self) {
        println!("\n{}", "=".repeat(80));
        println!("Memory Profile Report");
        println!("{}", "=".repeat(80));
        println!(
            "Total allocations: {}  |  Current: {} bytes  |  Peak: {} bytes",
            self.total_allocations(),
            format_bytes(self.current_usage()),
            format_bytes(self.peak_usage())
        );
        println!("{}", "-".repeat(80));
        println!(
            "{:<40} {:>10} {:>12} {:>12}",
            "Site", "Allocs", "Total", "Live"
        );
        println!("{}", "-".repeat(80));
        
        for info in self.allocations_by_bytes() {
            println!(
                "{:<40} {:>10} {:>12} {:>12}",
                truncate(&info.site, 40),
                info.alloc_count,
                format_bytes(info.total_bytes),
                format_bytes_signed(info.live_bytes),
            );
        }
        
        println!("{}", "=".repeat(80));
    }
}

impl Default for MemoryProfiler {
    fn default() -> Self {
        Self::new()
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_bytes_signed(bytes: i64) -> String {
    if bytes < 0 {
        format!("-{}", format_bytes((-bytes) as u64))
    } else {
        format_bytes(bytes as u64)
    }
}

// =============================================================================
// Combined Profiler
// =============================================================================

/// Combined CPU and memory profiler.
pub struct Profiler {
    pub cpu: CpuProfiler,
    pub memory: MemoryProfiler,
}

impl Profiler {
    /// Create a new profiler.
    pub fn new() -> Self {
        Self {
            cpu: CpuProfiler::new(),
            memory: MemoryProfiler::new(),
        }
    }
    
    /// Print combined report.
    pub fn print_report(&self) {
        self.cpu.print_report();
        self.memory.print_report();
    }
    
    /// Clear all statistics.
    pub fn clear(&self) {
        self.cpu.clear();
        self.memory.clear();
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Sampling Profiler
// =============================================================================

/// Sampling profiler for low-overhead profiling.
pub struct SamplingProfiler {
    /// Samples by stack trace.
    samples: Mutex<HashMap<Vec<String>, u64>>,
    /// Sample count.
    sample_count: AtomicU64,
    /// Sampling interval.
    interval: Duration,
}

impl SamplingProfiler {
    /// Create a new sampling profiler.
    pub fn new(interval: Duration) -> Self {
        Self {
            samples: Mutex::new(HashMap::new()),
            sample_count: AtomicU64::new(0),
            interval,
        }
    }
    
    /// Record a sample.
    pub fn sample(&self, stack: Vec<String>) {
        self.sample_count.fetch_add(1, Ordering::Relaxed);
        
        let mut samples = self.samples.lock().unwrap();
        *samples.entry(stack).or_insert(0) += 1;
    }
    
    /// Get top stacks by sample count.
    pub fn top_stacks(&self, limit: usize) -> Vec<(Vec<String>, u64)> {
        let samples = self.samples.lock().unwrap();
        let mut entries: Vec<_> = samples.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(limit);
        entries
    }
    
    /// Get total sample count.
    pub fn total_samples(&self) -> u64 {
        self.sample_count.load(Ordering::Relaxed)
    }
    
    /// Print flame graph data (folded format).
    pub fn print_folded(&self) {
        let samples = self.samples.lock().unwrap();
        
        for (stack, count) in samples.iter() {
            let path = stack.join(";");
            println!("{} {}", path, count);
        }
    }
}

// =============================================================================
// Timer
// =============================================================================

/// Simple timer for measuring elapsed time.
#[derive(Debug, Clone)]
pub struct Timer {
    start: Instant,
    elapsed: Option<Duration>,
}

impl Timer {
    /// Start a new timer.
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            elapsed: None,
        }
    }
    
    /// Stop the timer.
    pub fn stop(&mut self) -> Duration {
        let elapsed = self.start.elapsed();
        self.elapsed = Some(elapsed);
        elapsed
    }
    
    /// Get elapsed time without stopping.
    pub fn elapsed(&self) -> Duration {
        self.elapsed.unwrap_or_else(|| self.start.elapsed())
    }
    
    /// Reset and restart.
    pub fn restart(&mut self) {
        self.start = Instant::now();
        self.elapsed = None;
    }
}

/// Time a closure.
pub fn time<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cpu_profiler() {
        let profiler = CpuProfiler::new();
        
        for _ in 0..100 {
            profiler.profile("test_function", || {
                std::thread::sleep(Duration::from_micros(100));
            });
        }
        
        let stats = profiler.stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].calls, 100);
    }
    
    #[test]
    fn test_memory_profiler() {
        let profiler = MemoryProfiler::new();
        
        profiler.alloc("test", 1024);
        profiler.alloc("test", 2048);
        profiler.dealloc("test", 1024);
        
        assert_eq!(profiler.current_usage(), 2048);
        assert_eq!(profiler.peak_usage(), 3072);
    }
    
    #[test]
    fn test_timer() {
        let mut timer = Timer::start();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.stop();
        
        assert!(elapsed >= Duration::from_millis(10));
    }
    
    #[test]
    fn test_time_closure() {
        let (result, duration) = time(|| {
            std::thread::sleep(Duration::from_millis(10));
            42
        });
        
        assert_eq!(result, 42);
        assert!(duration >= Duration::from_millis(10));
    }
}


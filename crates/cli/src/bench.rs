//! Benchmarking infrastructure.

use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Benchmark configuration.
#[derive(Clone, Debug)]
pub struct BenchConfig {
    /// Warmup iterations.
    pub warmup_iters: u32,
    /// Measurement iterations.
    pub measure_iters: u32,
    /// Minimum time for measurement.
    pub min_time: Duration,
    /// Maximum time for measurement.
    pub max_time: Duration,
    /// Confidence level (0.0-1.0).
    pub confidence: f64,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            warmup_iters: 3,
            measure_iters: 100,
            min_time: Duration::from_millis(100),
            max_time: Duration::from_secs(5),
            confidence: 0.95,
        }
    }
}

/// Benchmark result.
#[derive(Clone, Debug)]
pub struct BenchResult {
    pub name: String,
    pub samples: Vec<Duration>,
    pub mean: Duration,
    pub median: Duration,
    pub std_dev: Duration,
    pub min: Duration,
    pub max: Duration,
    pub iterations: u32,
    pub throughput: Option<Throughput>,
}

impl BenchResult {
    /// Calculate from samples.
    pub fn from_samples(name: &str, samples: Vec<Duration>, iters: u32) -> Self {
        let n = samples.len();
        
        if n == 0 {
            return Self {
                name: name.to_string(),
                samples: vec![],
                mean: Duration::ZERO,
                median: Duration::ZERO,
                std_dev: Duration::ZERO,
                min: Duration::ZERO,
                max: Duration::ZERO,
                iterations: iters,
                throughput: None,
            };
        }
        
        let mut sorted = samples.clone();
        sorted.sort();
        
        let sum: Duration = samples.iter().sum();
        let mean = sum / n as u32;
        
        let median = if n % 2 == 0 {
            (sorted[n/2 - 1] + sorted[n/2]) / 2
        } else {
            sorted[n/2]
        };
        
        let variance: f64 = samples.iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - mean.as_nanos() as f64;
                diff * diff
            })
            .sum::<f64>() / (n - 1).max(1) as f64;
        
        let std_dev = Duration::from_nanos(variance.sqrt() as u64);
        
        Self {
            name: name.to_string(),
            samples,
            mean,
            median,
            std_dev,
            min: *sorted.first().unwrap(),
            max: *sorted.last().unwrap(),
            iterations: iters,
            throughput: None,
        }
    }
    
    /// Set throughput measurement.
    pub fn with_throughput(mut self, throughput: Throughput) -> Self {
        self.throughput = Some(throughput);
        self
    }
    
    /// Format as string.
    pub fn format(&self) -> String {
        let mean_ns = self.mean.as_nanos();
        let time_str = if mean_ns < 1_000 {
            format!("{:.2} ns", mean_ns as f64)
        } else if mean_ns < 1_000_000 {
            format!("{:.2} µs", mean_ns as f64 / 1_000.0)
        } else if mean_ns < 1_000_000_000 {
            format!("{:.2} ms", mean_ns as f64 / 1_000_000.0)
        } else {
            format!("{:.2} s", mean_ns as f64 / 1_000_000_000.0)
        };
        
        let std_dev_ns = self.std_dev.as_nanos();
        let std_dev_str = if std_dev_ns < 1_000 {
            format!("{:.2} ns", std_dev_ns as f64)
        } else if std_dev_ns < 1_000_000 {
            format!("{:.2} µs", std_dev_ns as f64 / 1_000.0)
        } else {
            format!("{:.2} ms", std_dev_ns as f64 / 1_000_000.0)
        };
        
        let mut result = format!(
            "{}: {} ± {} (n={})",
            self.name, time_str, std_dev_str, self.samples.len()
        );
        
        if let Some(ref tp) = self.throughput {
            result.push_str(&format!(" [{:.2} {}/s]", tp.value(), tp.unit()));
        }
        
        result
    }
}

/// Throughput measurement.
#[derive(Clone, Debug)]
pub enum Throughput {
    Bytes(u64),
    Elements(u64),
    Custom(f64, String),
}

impl Throughput {
    pub fn value(&self) -> f64 {
        match self {
            Throughput::Bytes(n) => *n as f64,
            Throughput::Elements(n) => *n as f64,
            Throughput::Custom(v, _) => *v,
        }
    }
    
    pub fn unit(&self) -> &str {
        match self {
            Throughput::Bytes(_) => "bytes",
            Throughput::Elements(_) => "elements",
            Throughput::Custom(_, u) => u,
        }
    }
}

/// Benchmark runner.
pub struct Bencher {
    config: BenchConfig,
    results: Vec<BenchResult>,
}

impl Bencher {
    pub fn new(config: BenchConfig) -> Self {
        Self {
            config,
            results: Vec::new(),
        }
    }
    
    /// Run a benchmark.
    pub fn bench<F>(&mut self, name: &str, mut f: F) -> &BenchResult
    where
        F: FnMut(),
    {
        // Warmup
        for _ in 0..self.config.warmup_iters {
            f();
        }
        
        // Measurement
        let mut samples = Vec::new();
        let start = Instant::now();
        let mut iters = 0u32;
        
        while samples.len() < self.config.measure_iters as usize 
            && start.elapsed() < self.config.max_time 
        {
            let iter_start = Instant::now();
            f();
            let elapsed = iter_start.elapsed();
            samples.push(elapsed);
            iters += 1;
            
            // Ensure minimum time
            if start.elapsed() >= self.config.min_time && samples.len() >= 10 {
                break;
            }
        }
        
        let result = BenchResult::from_samples(name, samples, iters);
        self.results.push(result);
        self.results.last().unwrap()
    }
    
    /// Run a benchmark with setup.
    pub fn bench_with_setup<S, F, T>(&mut self, name: &str, mut setup: S, mut f: F) -> &BenchResult
    where
        S: FnMut() -> T,
        F: FnMut(T),
    {
        let mut samples = Vec::new();
        let start = Instant::now();
        let mut iters = 0u32;
        
        // Warmup
        for _ in 0..self.config.warmup_iters {
            let input = setup();
            f(input);
        }
        
        // Measurement
        while samples.len() < self.config.measure_iters as usize 
            && start.elapsed() < self.config.max_time 
        {
            let input = setup();
            let iter_start = Instant::now();
            f(input);
            let elapsed = iter_start.elapsed();
            samples.push(elapsed);
            iters += 1;
        }
        
        let result = BenchResult::from_samples(name, samples, iters);
        self.results.push(result);
        self.results.last().unwrap()
    }
    
    /// Get all results.
    pub fn results(&self) -> &[BenchResult] {
        &self.results
    }
    
    /// Print report.
    pub fn print_report(&self) {
        println!("\n{}", "=".repeat(60));
        println!("Benchmark Results");
        println!("{}", "=".repeat(60));
        
        for result in &self.results {
            println!("{}", result.format());
        }
        
        println!("{}", "=".repeat(60));
    }
}

impl Default for Bencher {
    fn default() -> Self {
        Self::new(BenchConfig::default())
    }
}

/// Compare two benchmark results.
pub fn compare(baseline: &BenchResult, current: &BenchResult) -> Comparison {
    let baseline_ns = baseline.mean.as_nanos() as f64;
    let current_ns = current.mean.as_nanos() as f64;
    
    let diff_ns = current_ns - baseline_ns;
    let diff_pct = (diff_ns / baseline_ns) * 100.0;
    
    let change = if diff_pct.abs() < 1.0 {
        ChangeKind::NoChange
    } else if diff_pct < 0.0 {
        ChangeKind::Improvement
    } else {
        ChangeKind::Regression
    };
    
    Comparison {
        baseline: baseline.clone(),
        current: current.clone(),
        diff_absolute: Duration::from_nanos(diff_ns.abs() as u64),
        diff_percent: diff_pct,
        change,
    }
}

/// Comparison between two benchmarks.
#[derive(Clone, Debug)]
pub struct Comparison {
    pub baseline: BenchResult,
    pub current: BenchResult,
    pub diff_absolute: Duration,
    pub diff_percent: f64,
    pub change: ChangeKind,
}

impl Comparison {
    pub fn format(&self) -> String {
        let sign = if self.diff_percent < 0.0 { "" } else { "+" };
        let status = match self.change {
            ChangeKind::Improvement => "✓ improved",
            ChangeKind::Regression => "✗ regressed",
            ChangeKind::NoChange => "= no change",
        };
        
        format!(
            "{}: {} -> {} ({}{:.2}%) {}",
            self.baseline.name,
            format_duration(self.baseline.mean),
            format_duration(self.current.mean),
            sign,
            self.diff_percent,
            status
        )
    }
}

/// Kind of change between benchmarks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Improvement,
    Regression,
    NoChange,
}

fn format_duration(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{} ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
    }
}

/// Profile a piece of code.
pub fn profile<F, T>(name: &str, f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    
    println!("[PROFILE] {}: {}", name, format_duration(elapsed));
    
    (result, elapsed)
}

/// Simple profiler for measuring code sections.
pub struct Profiler {
    timings: HashMap<String, Vec<Duration>>,
    current: Option<(String, Instant)>,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            timings: HashMap::new(),
            current: None,
        }
    }
    
    /// Start timing a section.
    pub fn start(&mut self, name: &str) {
        self.current = Some((name.to_string(), Instant::now()));
    }
    
    /// Stop timing current section.
    pub fn stop(&mut self) {
        if let Some((name, start)) = self.current.take() {
            let elapsed = start.elapsed();
            self.timings.entry(name).or_default().push(elapsed);
        }
    }
    
    /// Time a closure.
    pub fn time<F, T>(&mut self, name: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.start(name);
        let result = f();
        self.stop();
        result
    }
    
    /// Get summary.
    pub fn summary(&self) -> HashMap<String, ProfileSummary> {
        let mut summaries = HashMap::new();
        
        for (name, timings) in &self.timings {
            let total: Duration = timings.iter().sum();
            let count = timings.len();
            let mean = total / count as u32;
            
            summaries.insert(name.clone(), ProfileSummary {
                total,
                count,
                mean,
            });
        }
        
        summaries
    }
    
    /// Print summary.
    pub fn print_summary(&self) {
        println!("\nProfile Summary:");
        println!("{:-<50}", "");
        
        let mut entries: Vec<_> = self.summary().into_iter().collect();
        entries.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        
        for (name, summary) in entries {
            println!(
                "{:30} {:>10} ({} calls, {} mean)",
                name,
                format_duration(summary.total),
                summary.count,
                format_duration(summary.mean)
            );
        }
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Profile summary for a section.
#[derive(Clone, Debug)]
pub struct ProfileSummary {
    pub total: Duration,
    pub count: usize,
    pub mean: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bencher() {
        let mut bencher = Bencher::new(BenchConfig {
            warmup_iters: 1,
            measure_iters: 10,
            min_time: Duration::from_millis(10),
            max_time: Duration::from_secs(1),
            confidence: 0.95,
        });
        
        let result = bencher.bench("test", || {
            let mut sum = 0;
            for i in 0..1000 {
                sum += i;
            }
            std::hint::black_box(sum);
        });
        
        assert!(!result.samples.is_empty());
    }
    
    #[test]
    fn test_profiler() {
        let mut profiler = Profiler::new();
        
        profiler.time("test1", || {
            std::thread::sleep(Duration::from_millis(1));
        });
        
        profiler.time("test2", || {
            std::thread::sleep(Duration::from_millis(2));
        });
        
        let summary = profiler.summary();
        assert!(summary.contains_key("test1"));
        assert!(summary.contains_key("test2"));
    }
}


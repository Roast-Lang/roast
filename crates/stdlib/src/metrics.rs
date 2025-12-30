//! Metrics collection and export.
//!
//! Provides Prometheus-compatible metrics for observability:
//! - Counters (monotonically increasing values)
//! - Gauges (values that can go up and down)
//! - Histograms (distribution of values)
//! - Labels for dimensional data

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

// =============================================================================
// Labels
// =============================================================================

/// Labels for metrics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Labels {
    labels: Vec<(String, String)>,
}

impl Labels {
    /// Create empty labels.
    pub fn new() -> Self {
        Self { labels: Vec::new() }
    }
    
    /// Create labels from pairs.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            labels: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }
    
    /// Add a label.
    pub fn add(&mut self, key: &str, value: &str) -> &mut Self {
        self.labels.push((key.to_string(), value.to_string()));
        self
    }
    
    /// Format labels for Prometheus.
    pub fn to_prometheus(&self) -> String {
        if self.labels.is_empty() {
            return String::new();
        }
        
        let pairs: Vec<String> = self.labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
            .collect();
        
        format!("{{{}}}", pairs.join(","))
    }
}

fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// =============================================================================
// Counter
// =============================================================================

/// A counter metric (only increases).
#[derive(Debug)]
pub struct Counter {
    name: String,
    help: String,
    values: RwLock<HashMap<Labels, AtomicU64>>,
}

impl Counter {
    /// Create a new counter.
    pub fn new(name: &str, help: &str) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            values: RwLock::new(HashMap::new()),
        }
    }
    
    /// Increment the counter by 1.
    pub fn inc(&self) {
        self.inc_by(1);
    }
    
    /// Increment the counter by n.
    pub fn inc_by(&self, n: u64) {
        self.inc_with_labels(&Labels::new(), n);
    }
    
    /// Increment with labels.
    pub fn inc_with_labels(&self, labels: &Labels, n: u64) {
        let mut values = self.values.write().unwrap();
        values
            .entry(labels.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(n, Ordering::Relaxed);
    }
    
    /// Get current value.
    pub fn get(&self) -> u64 {
        self.get_with_labels(&Labels::new())
    }
    
    /// Get value with labels.
    pub fn get_with_labels(&self, labels: &Labels) -> u64 {
        let values = self.values.read().unwrap();
        values.get(labels)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
    
    /// Reset the counter.
    pub fn reset(&self) {
        let mut values = self.values.write().unwrap();
        values.clear();
    }
    
    /// Export to Prometheus format.
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        output.push_str(&format!("# TYPE {} counter\n", self.name));
        
        let values = self.values.read().unwrap();
        for (labels, value) in values.iter() {
            let val = value.load(Ordering::Relaxed);
            output.push_str(&format!("{}{} {}\n", 
                self.name, 
                labels.to_prometheus(),
                val));
        }
        
        output
    }
}

// =============================================================================
// Gauge
// =============================================================================

/// A gauge metric (can go up and down).
#[derive(Debug)]
pub struct Gauge {
    name: String,
    help: String,
    values: RwLock<HashMap<Labels, AtomicI64>>,
}

impl Gauge {
    /// Create a new gauge.
    pub fn new(name: &str, help: &str) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            values: RwLock::new(HashMap::new()),
        }
    }
    
    /// Set the gauge value.
    pub fn set(&self, value: i64) {
        self.set_with_labels(&Labels::new(), value);
    }
    
    /// Set with labels.
    pub fn set_with_labels(&self, labels: &Labels, value: i64) {
        let mut values = self.values.write().unwrap();
        values
            .entry(labels.clone())
            .or_insert_with(|| AtomicI64::new(0))
            .store(value, Ordering::Relaxed);
    }
    
    /// Increment the gauge.
    pub fn inc(&self) {
        self.add(1);
    }
    
    /// Decrement the gauge.
    pub fn dec(&self) {
        self.add(-1);
    }
    
    /// Add to the gauge.
    pub fn add(&self, n: i64) {
        self.add_with_labels(&Labels::new(), n);
    }
    
    /// Add with labels.
    pub fn add_with_labels(&self, labels: &Labels, n: i64) {
        let mut values = self.values.write().unwrap();
        values
            .entry(labels.clone())
            .or_insert_with(|| AtomicI64::new(0))
            .fetch_add(n, Ordering::Relaxed);
    }
    
    /// Get current value.
    pub fn get(&self) -> i64 {
        self.get_with_labels(&Labels::new())
    }
    
    /// Get value with labels.
    pub fn get_with_labels(&self, labels: &Labels) -> i64 {
        let values = self.values.read().unwrap();
        values.get(labels)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
    
    /// Export to Prometheus format.
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        output.push_str(&format!("# TYPE {} gauge\n", self.name));
        
        let values = self.values.read().unwrap();
        for (labels, value) in values.iter() {
            let val = value.load(Ordering::Relaxed);
            output.push_str(&format!("{}{} {}\n", 
                self.name, 
                labels.to_prometheus(),
                val));
        }
        
        output
    }
}

// =============================================================================
// Histogram
// =============================================================================

/// Histogram bucket boundaries.
pub fn default_buckets() -> Vec<f64> {
    vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
}

/// A histogram metric.
pub struct Histogram {
    name: String,
    help: String,
    buckets: Vec<f64>,
    values: RwLock<HashMap<Labels, HistogramData>>,
}

struct HistogramData {
    bucket_counts: Vec<AtomicU64>,
    sum: AtomicU64, // Stored as bits
    count: AtomicU64,
}

impl HistogramData {
    fn new(num_buckets: usize) -> Self {
        Self {
            bucket_counts: (0..num_buckets).map(|_| AtomicU64::new(0)).collect(),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }
}

impl Histogram {
    /// Create a new histogram with default buckets.
    pub fn new(name: &str, help: &str) -> Self {
        Self::with_buckets(name, help, default_buckets())
    }
    
    /// Create a histogram with custom buckets.
    pub fn with_buckets(name: &str, help: &str, buckets: Vec<f64>) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            buckets,
            values: RwLock::new(HashMap::new()),
        }
    }
    
    /// Observe a value.
    pub fn observe(&self, value: f64) {
        self.observe_with_labels(&Labels::new(), value);
    }
    
    /// Observe with labels.
    pub fn observe_with_labels(&self, labels: &Labels, value: f64) {
        let mut values = self.values.write().unwrap();
        let data = values
            .entry(labels.clone())
            .or_insert_with(|| HistogramData::new(self.buckets.len()));
        
        // Update buckets
        for (i, &bound) in self.buckets.iter().enumerate() {
            if value <= bound {
                data.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        
        // Update sum and count
        let bits = value.to_bits();
        let old_bits = data.sum.load(Ordering::Relaxed);
        let old_sum = f64::from_bits(old_bits);
        let new_sum = old_sum + value;
        data.sum.store(new_sum.to_bits(), Ordering::Relaxed);
        data.count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Create a timer that observes on drop.
    pub fn start_timer(&self) -> HistogramTimer {
        HistogramTimer {
            histogram: self,
            labels: Labels::new(),
            start: Instant::now(),
        }
    }
    
    /// Export to Prometheus format.
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        output.push_str(&format!("# TYPE {} histogram\n", self.name));
        
        let values = self.values.read().unwrap();
        for (labels, data) in values.iter() {
            let label_str = labels.to_prometheus();
            
            // Buckets
            let mut cumulative = 0u64;
            for (i, &bound) in self.buckets.iter().enumerate() {
                cumulative += data.bucket_counts[i].load(Ordering::Relaxed);
                let bucket_labels = if label_str.is_empty() {
                    format!("{{le=\"{}\"}}", bound)
                } else {
                    format!("{{le=\"{}\",{}}}", bound, &label_str[1..label_str.len()-1])
                };
                output.push_str(&format!("{}_bucket{} {}\n", 
                    self.name, bucket_labels, cumulative));
            }
            
            // +Inf bucket
            let count = data.count.load(Ordering::Relaxed);
            let inf_labels = if label_str.is_empty() {
                "{le=\"+Inf\"}".to_string()
            } else {
                format!("{{le=\"+Inf\",{}}}", &label_str[1..label_str.len()-1])
            };
            output.push_str(&format!("{}_bucket{} {}\n", 
                self.name, inf_labels, count));
            
            // Sum and count
            let sum = f64::from_bits(data.sum.load(Ordering::Relaxed));
            output.push_str(&format!("{}_sum{} {}\n", self.name, label_str, sum));
            output.push_str(&format!("{}_count{} {}\n", self.name, label_str, count));
        }
        
        output
    }
}

/// Timer for histogram observations.
pub struct HistogramTimer<'a> {
    histogram: &'a Histogram,
    labels: Labels,
    start: Instant,
}

impl<'a> HistogramTimer<'a> {
    /// Stop the timer and record the duration.
    pub fn stop(self) -> f64 {
        let duration = self.start.elapsed().as_secs_f64();
        self.histogram.observe_with_labels(&self.labels, duration);
        duration
    }
}

impl<'a> Drop for HistogramTimer<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.histogram.observe_with_labels(&self.labels, duration);
    }
}

// =============================================================================
// Registry
// =============================================================================

/// Metrics registry.
pub struct Registry {
    counters: RwLock<HashMap<String, Arc<Counter>>>,
    gauges: RwLock<HashMap<String, Arc<Gauge>>>,
    histograms: RwLock<HashMap<String, Arc<Histogram>>>,
}

impl Registry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        }
    }
    
    /// Register a counter.
    pub fn register_counter(&self, name: &str, help: &str) -> Arc<Counter> {
        let mut counters = self.counters.write().unwrap();
        let counter = Arc::new(Counter::new(name, help));
        counters.insert(name.to_string(), Arc::clone(&counter));
        counter
    }
    
    /// Get or create a counter.
    pub fn counter(&self, name: &str, help: &str) -> Arc<Counter> {
        {
            let counters = self.counters.read().unwrap();
            if let Some(c) = counters.get(name) {
                return Arc::clone(c);
            }
        }
        self.register_counter(name, help)
    }
    
    /// Register a gauge.
    pub fn register_gauge(&self, name: &str, help: &str) -> Arc<Gauge> {
        let mut gauges = self.gauges.write().unwrap();
        let gauge = Arc::new(Gauge::new(name, help));
        gauges.insert(name.to_string(), Arc::clone(&gauge));
        gauge
    }
    
    /// Get or create a gauge.
    pub fn gauge(&self, name: &str, help: &str) -> Arc<Gauge> {
        {
            let gauges = self.gauges.read().unwrap();
            if let Some(g) = gauges.get(name) {
                return Arc::clone(g);
            }
        }
        self.register_gauge(name, help)
    }
    
    /// Register a histogram.
    pub fn register_histogram(&self, name: &str, help: &str) -> Arc<Histogram> {
        let mut histograms = self.histograms.write().unwrap();
        let histogram = Arc::new(Histogram::new(name, help));
        histograms.insert(name.to_string(), Arc::clone(&histogram));
        histogram
    }
    
    /// Get or create a histogram.
    pub fn histogram(&self, name: &str, help: &str) -> Arc<Histogram> {
        {
            let histograms = self.histograms.read().unwrap();
            if let Some(h) = histograms.get(name) {
                return Arc::clone(h);
            }
        }
        self.register_histogram(name, help)
    }
    
    /// Export all metrics to Prometheus format.
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();
        
        // Counters
        let counters = self.counters.read().unwrap();
        for counter in counters.values() {
            output.push_str(&counter.to_prometheus());
            output.push('\n');
        }
        
        // Gauges
        let gauges = self.gauges.read().unwrap();
        for gauge in gauges.values() {
            output.push_str(&gauge.to_prometheus());
            output.push('\n');
        }
        
        // Histograms
        let histograms = self.histograms.read().unwrap();
        for histogram in histograms.values() {
            output.push_str(&histogram.to_prometheus());
            output.push('\n');
        }
        
        output
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Global Registry
// =============================================================================

lazy_static::lazy_static! {
    /// Global metrics registry.
    pub static ref REGISTRY: Registry = Registry::new();
}

/// Get the global registry.
pub fn global_registry() -> &'static Registry {
    &REGISTRY
}

// =============================================================================
// Convenience Macros/Functions
// =============================================================================

/// Create and register a counter.
pub fn counter(name: &str, help: &str) -> Arc<Counter> {
    global_registry().counter(name, help)
}

/// Create and register a gauge.
pub fn gauge(name: &str, help: &str) -> Arc<Gauge> {
    global_registry().gauge(name, help)
}

/// Create and register a histogram.
pub fn histogram(name: &str, help: &str) -> Arc<Histogram> {
    global_registry().histogram(name, help)
}

/// Export all global metrics.
pub fn export() -> String {
    global_registry().to_prometheus()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_counter() {
        let counter = Counter::new("test_counter", "A test counter");
        assert_eq!(counter.get(), 0);
        
        counter.inc();
        assert_eq!(counter.get(), 1);
        
        counter.inc_by(5);
        assert_eq!(counter.get(), 6);
    }
    
    #[test]
    fn test_counter_with_labels() {
        let counter = Counter::new("http_requests", "HTTP requests");
        
        let get_labels = Labels::from_pairs(&[("method", "GET"), ("status", "200")]);
        let post_labels = Labels::from_pairs(&[("method", "POST"), ("status", "201")]);
        
        counter.inc_with_labels(&get_labels, 10);
        counter.inc_with_labels(&post_labels, 5);
        
        assert_eq!(counter.get_with_labels(&get_labels), 10);
        assert_eq!(counter.get_with_labels(&post_labels), 5);
    }
    
    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test_gauge", "A test gauge");
        assert_eq!(gauge.get(), 0);
        
        gauge.set(42);
        assert_eq!(gauge.get(), 42);
        
        gauge.inc();
        assert_eq!(gauge.get(), 43);
        
        gauge.dec();
        assert_eq!(gauge.get(), 42);
        
        gauge.add(-10);
        assert_eq!(gauge.get(), 32);
    }
    
    #[test]
    fn test_histogram() {
        let histogram = Histogram::with_buckets(
            "test_histogram",
            "A test histogram",
            vec![1.0, 5.0, 10.0],
        );
        
        histogram.observe(0.5);
        histogram.observe(3.0);
        histogram.observe(7.0);
        histogram.observe(15.0);
        
        let output = histogram.to_prometheus();
        assert!(output.contains("test_histogram_bucket"));
        assert!(output.contains("test_histogram_sum"));
        assert!(output.contains("test_histogram_count"));
    }
    
    #[test]
    fn test_labels() {
        let mut labels = Labels::new();
        labels.add("method", "GET").add("status", "200");
        
        assert_eq!(labels.to_prometheus(), "{method=\"GET\",status=\"200\"}");
    }
    
    #[test]
    fn test_registry() {
        let registry = Registry::new();
        
        let counter = registry.counter("requests_total", "Total requests");
        counter.inc();
        
        let gauge = registry.gauge("active_connections", "Active connections");
        gauge.set(10);
        
        let output = registry.to_prometheus();
        assert!(output.contains("requests_total"));
        assert!(output.contains("active_connections"));
    }
    
    #[test]
    fn test_prometheus_format() {
        let counter = Counter::new("http_requests_total", "Total HTTP requests");
        counter.inc_by(42);
        
        let output = counter.to_prometheus();
        assert!(output.contains("# HELP http_requests_total Total HTTP requests"));
        assert!(output.contains("# TYPE http_requests_total counter"));
        assert!(output.contains("http_requests_total 42"));
    }
}

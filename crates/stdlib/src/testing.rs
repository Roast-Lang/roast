//! Testing framework.
//!
//! Assertions, test runners, and mocking utilities.

use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::coverage::{CoverageCollector, CoverageSummary, SharedCoverage};

/// Test result.
#[derive(Clone, Debug)]
pub enum TestResult {
    Passed,
    Failed(String),
    Skipped(String),
}

impl TestResult {
    pub fn is_passed(&self) -> bool {
        matches!(self, TestResult::Passed)
    }
    
    pub fn is_failed(&self) -> bool {
        matches!(self, TestResult::Failed(_))
    }
}

/// A single test case.
pub struct TestCase {
    pub name: String,
    pub func: Box<dyn Fn() + Send + Sync>,
    pub timeout: Option<Duration>,
    pub should_panic: bool,
}

impl TestCase {
    /// Create a new test case.
    pub fn new<F>(name: &str, func: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            name: name.to_string(),
            func: Box::new(func),
            timeout: None,
            should_panic: false,
        }
    }
    
    /// Set timeout.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
    
    /// Expect the test to panic.
    pub fn should_panic(mut self) -> Self {
        self.should_panic = true;
        self
    }
    
    /// Run the test.
    pub fn run(&self) -> TestResult {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            (self.func)();
        }));
        
        match (result, self.should_panic) {
            (Ok(()), false) => TestResult::Passed,
            (Ok(()), true) => TestResult::Failed("Expected panic but test passed".to_string()),
            (Err(e), true) => TestResult::Passed,
            (Err(e), false) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                TestResult::Failed(msg)
            }
        }
    }
}

/// Test suite containing multiple tests.
pub struct TestSuite {
    pub name: String,
    tests: Vec<TestCase>,
    before_each: Option<Box<dyn Fn() + Send + Sync>>,
    after_each: Option<Box<dyn Fn() + Send + Sync>>,
}

impl TestSuite {
    /// Create a new test suite.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tests: Vec::new(),
            before_each: None,
            after_each: None,
        }
    }
    
    /// Add a test.
    pub fn test<F>(&mut self, name: &str, func: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.tests.push(TestCase::new(name, func));
        self
    }
    
    /// Set before_each hook.
    pub fn before_each<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.before_each = Some(Box::new(func));
        self
    }
    
    /// Set after_each hook.
    pub fn after_each<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.after_each = Some(Box::new(func));
        self
    }
    
    /// Run all tests.
    pub fn run(&self) -> TestReport {
        let mut report = TestReport::new(&self.name);
        
        for test in &self.tests {
            // Before each
            if let Some(ref before) = self.before_each {
                before();
            }
            
            // Run test
            let start = Instant::now();
            let result = test.run();
            let duration = start.elapsed();
            
            report.add_result(&test.name, result, duration);
            
            // After each
            if let Some(ref after) = self.after_each {
                after();
            }
        }
        
        report
    }
}

/// Test report.
#[derive(Debug)]
pub struct TestReport {
    pub suite_name: String,
    pub results: Vec<(String, TestResult, Duration)>,
    pub total_duration: Duration,
}

impl TestReport {
    fn new(suite_name: &str) -> Self {
        Self {
            suite_name: suite_name.to_string(),
            results: Vec::new(),
            total_duration: Duration::ZERO,
        }
    }
    
    fn add_result(&mut self, name: &str, result: TestResult, duration: Duration) {
        self.results.push((name.to_string(), result, duration));
        self.total_duration += duration;
    }
    
    /// Get number of passed tests.
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|(_, r, _)| r.is_passed()).count()
    }
    
    /// Get number of failed tests.
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|(_, r, _)| r.is_failed()).count()
    }
    
    /// Get total number of tests.
    pub fn total(&self) -> usize {
        self.results.len()
    }
    
    /// Check if all tests passed.
    pub fn all_passed(&self) -> bool {
        self.failed() == 0
    }
    
    /// Print the report.
    pub fn print(&self) {
        println!("\n{}", "=".repeat(60));
        println!("Test Suite: {}", self.suite_name);
        println!("{}", "=".repeat(60));
        
        for (name, result, duration) in &self.results {
            let status = match result {
                TestResult::Passed => "✓ PASS",
                TestResult::Failed(_) => "✗ FAIL",
                TestResult::Skipped(_) => "○ SKIP",
            };
            println!("  {} {} ({:.2}ms)", status, name, duration.as_secs_f64() * 1000.0);
            
            if let TestResult::Failed(msg) = result {
                println!("    Error: {}", msg);
            }
        }
        
        println!("{}", "-".repeat(60));
        println!(
            "Results: {} passed, {} failed, {} total ({:.2}ms)",
            self.passed(),
            self.failed(),
            self.total(),
            self.total_duration.as_secs_f64() * 1000.0
        );
        println!("{}", "=".repeat(60));
    }
}

// Assertion macros (as functions)

/// Assert that a condition is true.
pub fn assert_true(condition: bool, message: Option<&str>) {
    if !condition {
        panic!("{}", message.unwrap_or("Assertion failed: expected true"));
    }
}

/// Assert that a condition is false.
pub fn assert_false(condition: bool, message: Option<&str>) {
    if condition {
        panic!("{}", message.unwrap_or("Assertion failed: expected false"));
    }
}

/// Assert that two values are equal.
pub fn assert_eq<T: PartialEq + std::fmt::Debug>(left: T, right: T) {
    if left != right {
        panic!("Assertion failed: {:?} != {:?}", left, right);
    }
}

/// Assert that two values are not equal.
pub fn assert_ne<T: PartialEq + std::fmt::Debug>(left: T, right: T) {
    if left == right {
        panic!("Assertion failed: {:?} == {:?} (expected not equal)", left, right);
    }
}

/// Assert that a value is None.
pub fn assert_none<T: std::fmt::Debug>(value: Option<T>) {
    if let Some(v) = value {
        panic!("Assertion failed: expected None, got {:?}", v);
    }
}

/// Assert that a value is Some.
pub fn assert_some<T>(value: &Option<T>) {
    if value.is_none() {
        panic!("Assertion failed: expected Some, got None");
    }
}

/// Assert that a value is Ok.
pub fn assert_ok<T, E: std::fmt::Debug>(value: &Result<T, E>) {
    if let Err(e) = value {
        panic!("Assertion failed: expected Ok, got Err({:?})", e);
    }
}

/// Assert that a value is Err.
pub fn assert_err<T: std::fmt::Debug, E>(value: &Result<T, E>) {
    if let Ok(v) = value {
        panic!("Assertion failed: expected Err, got Ok({:?})", v);
    }
}

/// Assert that a float is approximately equal.
pub fn assert_approx_eq(left: f64, right: f64, epsilon: f64) {
    if (left - right).abs() > epsilon {
        panic!("Assertion failed: {} ≈ {} (epsilon: {})", left, right, epsilon);
    }
}

/// Assert that code panics.
pub fn assert_panics<F: FnOnce()>(f: F) {
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    if result.is_ok() {
        panic!("Assertion failed: expected panic");
    }
}

/// Mock counter for tracking calls.
pub struct MockCounter {
    count: AtomicUsize,
}

impl MockCounter {
    pub fn new() -> Self {
        Self { count: AtomicUsize::new(0) }
    }
    
    pub fn increment(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
    
    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
    
    pub fn reset(&self) {
        self.count.store(0, Ordering::SeqCst);
    }
}

impl Default for MockCounter {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Test Runner
// =============================================================================

/// Test runner configuration.
#[derive(Clone, Debug)]
pub struct TestRunnerConfig {
    /// Run tests in parallel.
    pub parallel: bool,
    /// Number of threads.
    pub threads: usize,
    /// Verbose output.
    pub verbose: bool,
    /// Filter pattern.
    pub filter: Option<String>,
    /// Fail fast on first error.
    pub fail_fast: bool,
    /// Show timing for each test.
    pub show_timing: bool,
    /// Color output.
    pub color: bool,
    /// Enable coverage collection.
    pub coverage: bool,
    /// Coverage output directory.
    pub coverage_dir: Option<String>,
    /// Minimum coverage threshold (percentage).
    pub coverage_threshold: Option<f64>,
}

impl Default for TestRunnerConfig {
    fn default() -> Self {
        Self {
            parallel: true,
            threads: num_cpus(),
            verbose: false,
            filter: None,
            fail_fast: false,
            show_timing: true,
            color: true,
            coverage: false,
            coverage_dir: None,
            coverage_threshold: None,
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

/// Test runner.
pub struct TestRunner {
    config: TestRunnerConfig,
    suites: Vec<TestSuite>,
    coverage: Option<SharedCoverage>,
}

impl TestRunner {
    /// Create a new test runner.
    pub fn new(config: TestRunnerConfig) -> Self {
        let coverage = if config.coverage {
            let cov = SharedCoverage::new();
            cov.enable();
            Some(cov)
        } else {
            None
        };
        
        Self {
            config,
            suites: Vec::new(),
            coverage,
        }
    }
    
    /// Add a test suite.
    pub fn add_suite(&mut self, suite: TestSuite) -> &mut Self {
        self.suites.push(suite);
        self
    }
    
    /// Get the coverage collector.
    pub fn coverage(&self) -> Option<&SharedCoverage> {
        self.coverage.as_ref()
    }
    
    /// Run all tests.
    pub fn run(&self) -> Vec<TestReport> {
        let mut reports = Vec::new();
        
        for suite in &self.suites {
            let report = suite.run();
            
            if self.config.verbose {
                report.print();
            }
            
            let failed = report.failed() > 0;
            reports.push(report);
            
            if failed && self.config.fail_fast {
                break;
            }
        }
        
        // Print summary
        self.print_summary(&reports);
        
        // Print and save coverage if enabled
        if let Some(ref coverage) = self.coverage {
            self.print_coverage(coverage);
        }
        
        reports
    }
    
    fn print_coverage(&self, coverage: &SharedCoverage) {
        let summary = coverage.summary();
        
        println!("\n{}", "=".repeat(60));
        println!("Coverage Summary");
        println!("{}", "=".repeat(60));
        println!("  Lines:     {:>6.1}% ({}/{})",
            summary.line_percent(),
            summary.covered_lines,
            summary.total_lines);
        println!("  Branches:  {:>6.1}% ({}/{})",
            summary.branch_percent(),
            summary.covered_branches,
            summary.total_branches);
        println!("  Functions: {:>6.1}% ({}/{})",
            summary.function_percent(),
            summary.covered_functions,
            summary.total_functions);
        
        // Check threshold
        if let Some(threshold) = self.config.coverage_threshold {
            let line_pct = summary.line_percent();
            if line_pct < threshold {
                println!("\n⚠ Coverage {:.1}% is below threshold {:.1}%", line_pct, threshold);
            } else {
                println!("\n✓ Coverage {:.1}% meets threshold {:.1}%", line_pct, threshold);
            }
        }
        
        // Generate HTML report if directory specified
        if let Some(ref dir) = self.config.coverage_dir {
            let path = std::path::Path::new(dir);
            if let Err(e) = coverage.report_html(path) {
                eprintln!("Failed to write coverage report: {}", e);
            } else {
                println!("\nCoverage report written to: {}/index.html", dir);
            }
        }
        
        println!("{}", "=".repeat(60));
    }
    
    fn print_summary(&self, reports: &[TestReport]) {
        let total_passed: usize = reports.iter().map(|r| r.passed()).sum();
        let total_failed: usize = reports.iter().map(|r| r.failed()).sum();
        let total: usize = reports.iter().map(|r| r.total()).sum();
        let total_duration: Duration = reports.iter()
            .map(|r| r.total_duration)
            .sum();
        
        println!("\n{}", "=".repeat(60));
        println!("Test Summary");
        println!("{}", "=".repeat(60));
        println!("Suites: {}", reports.len());
        println!("Tests:  {} passed, {} failed, {} total", 
            total_passed, total_failed, total);
        println!("Time:   {:.2}s", total_duration.as_secs_f64());
        
        if total_failed > 0 {
            println!("\nFailed tests:");
            for report in reports {
                for (name, result, _) in &report.results {
                    if let TestResult::Failed(msg) = result {
                        println!("  {} - {}: {}", report.suite_name, name, msg);
                    }
                }
            }
        }
        
        println!("{}", "=".repeat(60));
    }
}

// =============================================================================
// Parameterized Tests
// =============================================================================

/// Parameterized test case.
pub struct ParameterizedTest<T> {
    pub name: String,
    pub params: Vec<T>,
    pub test_fn: Box<dyn Fn(&T) + Send + Sync>,
}

impl<T> ParameterizedTest<T> {
    /// Create a new parameterized test.
    pub fn new<F>(name: &str, params: Vec<T>, test_fn: F) -> Self
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        Self {
            name: name.to_string(),
            params,
            test_fn: Box::new(test_fn),
        }
    }
    
    /// Run the test with all parameters.
    pub fn run(&self) -> Vec<(String, TestResult)> {
        let mut results = Vec::new();
        
        for (i, param) in self.params.iter().enumerate() {
            let test_name = format!("{}[{}]", self.name, i);
            
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                (self.test_fn)(param);
            }));
            
            let test_result = match result {
                Ok(()) => TestResult::Passed,
                Err(e) => {
                    let msg = if let Some(s) = e.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };
                    TestResult::Failed(msg)
                }
            };
            
            results.push((test_name, test_result));
        }
        
        results
    }
}

// =============================================================================
// Fixtures
// =============================================================================

/// Test fixture trait.
pub trait Fixture: Send + Sync {
    /// Setup the fixture.
    fn setup(&mut self) {}
    
    /// Teardown the fixture.
    fn teardown(&mut self) {}
}

/// Simple fixture that uses closures.
pub struct SimpleFixture<T> {
    value: Option<T>,
    setup_fn: Box<dyn Fn() -> T + Send + Sync>,
    teardown_fn: Option<Box<dyn Fn(&T) + Send + Sync>>,
}

impl<T: Send + Sync> SimpleFixture<T> {
    /// Create a new fixture.
    pub fn new<S>(setup: S) -> Self
    where
        S: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            value: None,
            setup_fn: Box::new(setup),
            teardown_fn: None,
        }
    }
    
    /// Set teardown function.
    pub fn with_teardown<F>(mut self, teardown: F) -> Self
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.teardown_fn = Some(Box::new(teardown));
        self
    }
    
    /// Get the value.
    pub fn get(&mut self) -> &T {
        if self.value.is_none() {
            self.value = Some((self.setup_fn)());
        }
        self.value.as_ref().unwrap()
    }
    
    /// Cleanup.
    pub fn cleanup(&mut self) {
        if let (Some(ref value), Some(ref teardown)) = (&self.value, &self.teardown_fn) {
            teardown(value);
        }
        self.value = None;
    }
}

// =============================================================================
// Test Discovery
// =============================================================================

use std::path::{Path, PathBuf};
use std::fs;

/// Discovers test files in a directory.
pub fn discover_tests(dir: &Path) -> Vec<PathBuf> {
    let mut tests = Vec::new();
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            if path.is_dir() {
                // Recurse into subdirectories
                tests.extend(discover_tests(&path));
            } else if path.extension().map(|e| e == "roast").unwrap_or(false) {
                // Check if it's a test file
                if let Some(name) = path.file_stem() {
                    let name = name.to_string_lossy();
                    if name.starts_with("test_") || name.ends_with("_test") {
                        tests.push(path);
                    }
                }
            }
        }
    }
    
    tests
}

/// Parse test functions from source.
pub fn find_test_functions(source: &str) -> Vec<String> {
    let mut tests = Vec::new();
    
    for line in source.lines() {
        let trimmed = line.trim();
        
        // Look for @test decorator
        if trimmed.starts_with("@test") {
            // Next non-empty line should be function definition
            continue;
        }
        
        // Look for test function definitions
        if trimmed.starts_with("def test_") {
            if let Some(paren_idx) = trimmed.find('(') {
                let name = &trimmed[4..paren_idx]; // Skip "def "
                tests.push(name.to_string());
            }
        }
    }
    
    tests
}

// =============================================================================
// Additional Assertions
// =============================================================================

/// Assert that a string contains a substring.
pub fn assert_contains(haystack: &str, needle: &str) {
    if !haystack.contains(needle) {
        panic!("Assertion failed: '{}' does not contain '{}'", haystack, needle);
    }
}

/// Assert that a string starts with a prefix.
pub fn assert_starts_with(s: &str, prefix: &str) {
    if !s.starts_with(prefix) {
        panic!("Assertion failed: '{}' does not start with '{}'", s, prefix);
    }
}

/// Assert that a string ends with a suffix.
pub fn assert_ends_with(s: &str, suffix: &str) {
    if !s.ends_with(suffix) {
        panic!("Assertion failed: '{}' does not end with '{}'", s, suffix);
    }
}

/// Assert that a value is in a range.
pub fn assert_in_range<T: PartialOrd + std::fmt::Debug>(value: T, min: T, max: T) {
    if value < min || value > max {
        panic!("Assertion failed: {:?} not in range [{:?}, {:?}]", value, min, max);
    }
}

/// Assert that a collection has a specific length.
pub fn assert_len<T>(collection: &[T], expected: usize) {
    if collection.len() != expected {
        panic!("Assertion failed: length {} != expected {}", collection.len(), expected);
    }
}

/// Assert that a collection is empty.
pub fn assert_empty<T>(collection: &[T]) {
    if !collection.is_empty() {
        panic!("Assertion failed: collection is not empty (length: {})", collection.len());
    }
}

/// Assert that a collection is not empty.
pub fn assert_not_empty<T>(collection: &[T]) {
    if collection.is_empty() {
        panic!("Assertion failed: collection is empty");
    }
}

/// Assert that two collections are equal (order matters).
pub fn assert_eq_collections<T: PartialEq + std::fmt::Debug>(left: &[T], right: &[T]) {
    if left != right {
        panic!("Assertion failed: collections are not equal\n  left:  {:?}\n  right: {:?}", left, right);
    }
}

/// Assert that a collection contains all items (order doesn't matter).
pub fn assert_contains_all<T: PartialEq + std::fmt::Debug>(collection: &[T], items: &[T]) {
    for item in items {
        if !collection.contains(item) {
            panic!("Assertion failed: collection does not contain {:?}", item);
        }
    }
}

// =============================================================================
// Test Markers
// =============================================================================

/// Marker for skipping a test.
pub struct Skip {
    pub reason: String,
}

impl Skip {
    pub fn new(reason: &str) -> Self {
        Self { reason: reason.to_string() }
    }
}

/// Skip the current test.
pub fn skip(reason: &str) -> ! {
    panic!("SKIP: {}", reason);
}

/// Fail the current test.
pub fn fail(message: &str) -> ! {
    panic!("{}", message);
}

/// Mark a test as expected to fail.
pub struct ExpectedFailure {
    pub reason: String,
}

impl ExpectedFailure {
    pub fn new(reason: &str) -> Self {
        Self { reason: reason.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_assertions() {
        assert_true(true, None);
        assert_false(false, None);
        assert_eq(42, 42);
        assert_ne(1, 2);
        assert_none::<i32>(None);
        assert_some(&Some(42));
        assert_ok::<i32, &str>(&Ok(42));
        assert_err::<i32, &str>(&Err("error"));
        assert_approx_eq(3.14, 3.14159, 0.01);
    }
    
    #[test]
    fn test_suite() {
        let mut suite = TestSuite::new("Example");
        
        suite.test("addition", || {
            assert_eq(2 + 2, 4);
        });
        
        suite.test("subtraction", || {
            assert_eq(5 - 3, 2);
        });
        
        let report = suite.run();
        assert!(report.all_passed());
    }
    
    #[test]
    fn test_parameterized() {
        let test = ParameterizedTest::new(
            "squares",
            vec![(2, 4), (3, 9), (4, 16)],
            |&(input, expected)| {
                assert_eq(input * input, expected);
            },
        );
        
        let results = test.run();
        assert!(results.iter().all(|(_, r)| r.is_passed()));
    }
    
    #[test]
    fn test_additional_assertions() {
        assert_contains("hello world", "world");
        assert_starts_with("hello world", "hello");
        assert_ends_with("hello world", "world");
        assert_in_range(5, 1, 10);
        assert_len(&[1, 2, 3], 3);
        assert_not_empty(&[1]);
        assert_eq_collections(&[1, 2, 3], &[1, 2, 3]);
    }
}

// =============================================================================
// Parallel Test Execution
// =============================================================================

use std::sync::{Arc, Mutex};
use std::thread;

/// Run tests in parallel using a thread pool.
pub fn run_tests_parallel<F>(tests: Vec<F>, num_threads: usize) -> Vec<TestResult>
where
    F: Fn() -> TestResult + Send + Sync + 'static,
{
    let tests: Vec<Arc<dyn Fn() -> TestResult + Send + Sync>> = tests
        .into_iter()
        .map(|f| Arc::new(f) as Arc<dyn Fn() -> TestResult + Send + Sync>)
        .collect();
    
    let results = Arc::new(Mutex::new(Vec::new()));
    let test_index = Arc::new(AtomicUsize::new(0));
    let num_tests = tests.len();
    let tests = Arc::new(tests);
    
    let mut handles = Vec::new();
    
    for _ in 0..num_threads.min(num_tests) {
        let tests = Arc::clone(&tests);
        let test_index = Arc::clone(&test_index);
        let results = Arc::clone(&results);
        
        let handle = thread::spawn(move || {
            loop {
                let idx = test_index.fetch_add(1, Ordering::SeqCst);
                if idx >= tests.len() {
                    break;
                }
                
                let test = &tests[idx];
                let result = panic::catch_unwind(AssertUnwindSafe(|| test()))
                    .unwrap_or_else(|e| {
                        let msg = if let Some(s) = e.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = e.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "Unknown panic".to_string()
                        };
                        TestResult::Failed(msg)
                    });
                
                let mut results = results.lock().unwrap();
                results.push((idx, result));
            }
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().ok();
    }
    
    // Sort results by original index
    let mut results = match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    };
    results.sort_by_key(|(idx, _)| *idx);
    results.into_iter().map(|(_, r)| r).collect()
}

/// Parallel test suite runner.
pub struct ParallelTestRunner {
    tests: Vec<(String, Arc<dyn Fn() + Send + Sync>)>,
    num_threads: usize,
}

impl ParallelTestRunner {
    /// Create a new parallel test runner.
    pub fn new(num_threads: usize) -> Self {
        Self {
            tests: Vec::new(),
            num_threads,
        }
    }
    
    /// Add a test.
    pub fn add_test<F>(&mut self, name: &str, test: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.tests.push((name.to_string(), Arc::new(test)));
    }
    
    /// Run all tests in parallel.
    pub fn run(&self) -> Vec<(String, TestResult, Duration)> {
        let results: Arc<Mutex<Vec<(String, TestResult, Duration)>>> = 
            Arc::new(Mutex::new(Vec::new()));
        let test_index = Arc::new(AtomicUsize::new(0));
        
        let tests: Vec<_> = self.tests.iter()
            .map(|(name, f)| (name.clone(), Arc::clone(f)))
            .collect();
        let tests = Arc::new(tests);
        
        let mut handles = Vec::new();
        
        for _ in 0..self.num_threads.min(self.tests.len()) {
            let tests = Arc::clone(&tests);
            let test_index = Arc::clone(&test_index);
            let results = Arc::clone(&results);
            
            let handle = thread::spawn(move || {
                loop {
                    let idx = test_index.fetch_add(1, Ordering::SeqCst);
                    if idx >= tests.len() {
                        break;
                    }
                    
                    let (name, test) = &tests[idx];
                    let start = Instant::now();
                    
                    let result = panic::catch_unwind(AssertUnwindSafe(|| test()))
                        .map(|_| TestResult::Passed)
                        .unwrap_or_else(|e| {
                            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                                s.to_string()
                            } else if let Some(s) = e.downcast_ref::<String>() {
                                s.clone()
                            } else {
                                "Unknown panic".to_string()
                            };
                            TestResult::Failed(msg)
                        });
                    
                    let duration = start.elapsed();
                    let mut results = results.lock().unwrap();
                    results.push((name.clone(), result, duration));
                }
            });
            
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().ok();
        }
        
        match Arc::try_unwrap(results) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        }
    }
}

// =============================================================================
// Mocking Framework
// =============================================================================

use std::collections::VecDeque;
use std::any::Any;

/// A mock function that records calls and returns configured values.
pub struct MockFn<Args, Ret> {
    calls: Mutex<Vec<Args>>,
    returns: Mutex<VecDeque<Ret>>,
    default_return: Mutex<Option<Box<dyn Fn() -> Ret + Send + Sync>>>,
}

impl<Args: Clone, Ret: Clone + Default> MockFn<Args, Ret> {
    /// Create a new mock function.
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            returns: Mutex::new(VecDeque::new()),
            default_return: Mutex::new(None),
        }
    }
    
    /// Configure a return value.
    pub fn returns(&self, value: Ret) -> &Self {
        self.returns.lock().unwrap().push_back(value);
        self
    }
    
    /// Configure multiple return values.
    pub fn returns_many(&self, values: Vec<Ret>) -> &Self {
        let mut returns = self.returns.lock().unwrap();
        for value in values {
            returns.push_back(value);
        }
        self
    }
    
    /// Configure a default return value generator.
    pub fn returns_with<F>(&self, f: F) -> &Self
    where
        F: Fn() -> Ret + Send + Sync + 'static,
    {
        *self.default_return.lock().unwrap() = Some(Box::new(f));
        self
    }
    
    /// Call the mock function.
    pub fn call(&self, args: Args) -> Ret {
        self.calls.lock().unwrap().push(args);
        
        if let Some(value) = self.returns.lock().unwrap().pop_front() {
            return value;
        }
        
        if let Some(ref f) = *self.default_return.lock().unwrap() {
            return f();
        }
        
        Ret::default()
    }
    
    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    
    /// Get all calls.
    pub fn calls(&self) -> Vec<Args> {
        self.calls.lock().unwrap().clone()
    }
    
    /// Verify the mock was called exactly n times.
    pub fn verify_called(&self, n: usize) {
        let count = self.call_count();
        if count != n {
            panic!("Expected {} calls, got {}", n, count);
        }
    }
    
    /// Verify the mock was never called.
    pub fn verify_never_called(&self) {
        self.verify_called(0);
    }
    
    /// Verify the mock was called at least once.
    pub fn verify_called_once(&self) {
        self.verify_called(1);
    }
    
    /// Reset the mock.
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.returns.lock().unwrap().clear();
    }
}

impl<Args: Clone, Ret: Clone + Default> Default for MockFn<Args, Ret> {
    fn default() -> Self {
        Self::new()
    }
}

/// A spy that wraps a real implementation and records calls.
pub struct Spy<T, Args, Ret> {
    inner: T,
    calls: Mutex<Vec<Args>>,
    func: fn(&T, Args) -> Ret,
}

impl<T, Args: Clone, Ret> Spy<T, Args, Ret> {
    /// Create a new spy.
    pub fn new(inner: T, func: fn(&T, Args) -> Ret) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
            func,
        }
    }
    
    /// Call through to the real implementation.
    pub fn call(&self, args: Args) -> Ret {
        self.calls.lock().unwrap().push(args.clone());
        (self.func)(&self.inner, args)
    }
    
    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    
    /// Get all calls.
    pub fn calls(&self) -> Vec<Args> {
        self.calls.lock().unwrap().clone()
    }
    
    /// Get the inner value.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

/// A stub that returns fixed values.
pub struct Stub<Ret> {
    value: Ret,
    call_count: AtomicUsize,
}

impl<Ret: Clone> Stub<Ret> {
    /// Create a new stub.
    pub fn new(value: Ret) -> Self {
        Self {
            value,
            call_count: AtomicUsize::new(0),
        }
    }
    
    /// Call the stub.
    pub fn call(&self) -> Ret {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.value.clone()
    }
    
    /// Get call count.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

/// Builder for creating mock expectations.
pub struct MockBuilder<Args, Ret> {
    mock: Arc<MockFn<Args, Ret>>,
}

impl<Args: Clone + 'static, Ret: Clone + Default + 'static> MockBuilder<Args, Ret> {
    /// Create a new mock builder.
    pub fn new() -> Self {
        Self {
            mock: Arc::new(MockFn::new()),
        }
    }
    
    /// Configure expected return value.
    pub fn when_called_return(self, value: Ret) -> Self {
        self.mock.returns(value);
        self
    }
    
    /// Configure multiple expected returns.
    pub fn when_called_return_sequence(self, values: Vec<Ret>) -> Self {
        self.mock.returns_many(values);
        self
    }
    
    /// Build the mock.
    pub fn build(self) -> Arc<MockFn<Args, Ret>> {
        self.mock
    }
}

impl<Args: Clone + 'static, Ret: Clone + Default + 'static> Default for MockBuilder<Args, Ret> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for mockable types.
pub trait Mockable {
    /// The mock type.
    type Mock;
    
    /// Create a mock.
    fn mock() -> Self::Mock;
}

// =============================================================================
// Test Doubles
// =============================================================================

/// A recorded call for verification.
#[derive(Clone, Debug)]
pub struct RecordedCall {
    pub method: String,
    pub args: Vec<String>,
    pub timestamp: Instant,
}

/// Generic call recorder.
pub struct CallRecorder {
    calls: Mutex<Vec<RecordedCall>>,
}

impl CallRecorder {
    /// Create a new call recorder.
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
    
    /// Record a call.
    pub fn record(&self, method: &str, args: Vec<String>) {
        self.calls.lock().unwrap().push(RecordedCall {
            method: method.to_string(),
            args,
            timestamp: Instant::now(),
        });
    }
    
    /// Get all calls.
    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }
    
    /// Get calls to a specific method.
    pub fn calls_to(&self, method: &str) -> Vec<RecordedCall> {
        self.calls.lock().unwrap()
            .iter()
            .filter(|c| c.method == method)
            .cloned()
            .collect()
    }
    
    /// Verify a method was called.
    pub fn verify_called(&self, method: &str) {
        let calls = self.calls_to(method);
        if calls.is_empty() {
            panic!("Expected '{}' to be called, but it was never called", method);
        }
    }
    
    /// Verify a method was called n times.
    pub fn verify_called_times(&self, method: &str, n: usize) {
        let calls = self.calls_to(method);
        if calls.len() != n {
            panic!(
                "Expected '{}' to be called {} times, but it was called {} times",
                method, n, calls.len()
            );
        }
    }
    
    /// Verify a method was never called.
    pub fn verify_never_called(&self, method: &str) {
        self.verify_called_times(method, 0);
    }
    
    /// Reset the recorder.
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
    }
}

impl Default for CallRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod mock_tests {
    use super::*;
    
    #[test]
    fn test_mock_fn() {
        let mock: MockFn<i32, String> = MockFn::new();
        mock.returns("first".to_string())
            .returns("second".to_string());
        
        assert_eq!(mock.call(1), "first");
        assert_eq!(mock.call(2), "second");
        assert_eq!(mock.call(3), String::default()); // Default
        
        mock.verify_called(3);
        assert_eq!(mock.calls(), vec![1, 2, 3]);
    }
    
    #[test]
    fn test_stub() {
        let stub = Stub::new(42);
        
        assert_eq!(stub.call(), 42);
        assert_eq!(stub.call(), 42);
        assert_eq!(stub.call_count(), 2);
    }
    
    #[test]
    fn test_call_recorder() {
        let recorder = CallRecorder::new();
        
        recorder.record("get", vec!["key1".to_string()]);
        recorder.record("set", vec!["key2".to_string(), "value".to_string()]);
        recorder.record("get", vec!["key3".to_string()]);
        
        recorder.verify_called("get");
        recorder.verify_called_times("get", 2);
        recorder.verify_called_times("set", 1);
    }
    
    #[test]
    fn test_parallel_runner() {
        let mut runner = ParallelTestRunner::new(4);
        
        runner.add_test("test1", || {
            std::thread::sleep(Duration::from_millis(10));
        });
        runner.add_test("test2", || {
            std::thread::sleep(Duration::from_millis(10));
        });
        runner.add_test("test3", || {
            std::thread::sleep(Duration::from_millis(10));
        });
        
        let results = runner.run();
        assert_eq!(results.len(), 3);
        
        for (_, result, _) in &results {
            assert!(result.is_passed());
        }
    }
}

// =============================================================================
// Property-Based Testing / QuickCheck-style Testing
// =============================================================================

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Configuration for property-based tests.
#[derive(Clone, Debug)]
pub struct PropTestConfig {
    /// Number of test cases to generate.
    pub num_tests: usize,
    /// Maximum shrink iterations.
    pub max_shrink_iters: usize,
    /// Random seed for reproducibility.
    pub seed: u64,
    /// Maximum size hint for generators.
    pub max_size: usize,
    /// Whether to print progress.
    pub verbose: bool,
}

impl Default for PropTestConfig {
    fn default() -> Self {
        Self {
            num_tests: 100,
            max_shrink_iters: 1000,
            seed: 0,
            max_size: 100,
            verbose: false,
        }
    }
}

impl PropTestConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set number of tests.
    pub fn with_tests(mut self, n: usize) -> Self {
        self.num_tests = n;
        self
    }
    
    /// Set random seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
    
    /// Set max size hint.
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }
}

/// A simple PRNG for generating test data.
#[derive(Clone)]
pub struct TestRng {
    state: u64,
}

impl TestRng {
    /// Create a new RNG with a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_add(1) }
    }
    
    /// Generate next u64.
    pub fn next_u64(&mut self) -> u64 {
        // PCG-like generator
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        ((xorshifted >> rot) | (xorshifted << ((!rot).wrapping_add(1) & 31))) as u64
    }
    
    /// Generate a value in range [0, max).
    pub fn next_range(&mut self, max: u64) -> u64 {
        if max == 0 {
            return 0;
        }
        self.next_u64() % max
    }
    
    /// Generate a boolean.
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }
    
    /// Generate a float in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }
}

/// Trait for types that can be randomly generated.
pub trait Arbitrary: Sized + Clone {
    /// Generate a random value.
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self;
    
    /// Shrink a value to find minimal counterexample.
    fn shrink(&self) -> Vec<Self> {
        Vec::new() // Default: no shrinking
    }
}

// Implement Arbitrary for common types
impl Arbitrary for bool {
    fn arbitrary(rng: &mut TestRng, _size: usize) -> Self {
        rng.next_bool()
    }
    
    fn shrink(&self) -> Vec<Self> {
        if *self { vec![false] } else { vec![] }
    }
}

impl Arbitrary for i32 {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        let max = (size as i64).min(i32::MAX as i64) as i32;
        (rng.next_range((max as u64).saturating_mul(2).saturating_add(1)) as i32)
            .wrapping_sub(max)
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0 {
            return shrinks;
        }
        shrinks.push(0);
        if *self > 0 {
            shrinks.push(*self - 1);
            shrinks.push(*self / 2);
        } else {
            shrinks.push(*self + 1);
            shrinks.push(*self / 2);
        }
        shrinks
    }
}

impl Arbitrary for i64 {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        let max = (size as i64).min(1000);
        (rng.next_range((max as u64).saturating_mul(2).saturating_add(1)) as i64)
            .wrapping_sub(max)
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0 {
            return shrinks;
        }
        shrinks.push(0);
        if *self > 0 {
            shrinks.push(*self - 1);
            shrinks.push(*self / 2);
        } else {
            shrinks.push(*self + 1);
            shrinks.push(*self / 2);
        }
        shrinks
    }
}

impl Arbitrary for u32 {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        rng.next_range(size as u64 + 1) as u32
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0 {
            return shrinks;
        }
        shrinks.push(0);
        shrinks.push(*self - 1);
        shrinks.push(*self / 2);
        shrinks
    }
}

impl Arbitrary for u64 {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        rng.next_range(size as u64 + 1)
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0 {
            return shrinks;
        }
        shrinks.push(0);
        shrinks.push(*self - 1);
        shrinks.push(*self / 2);
        shrinks
    }
}

impl Arbitrary for usize {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        rng.next_range(size as u64 + 1) as usize
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0 {
            return shrinks;
        }
        shrinks.push(0);
        shrinks.push(*self - 1);
        shrinks.push(*self / 2);
        shrinks
    }
}

impl Arbitrary for f64 {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        let int_part = i32::arbitrary(rng, size) as f64;
        let frac_part = rng.next_f64();
        int_part + frac_part
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if *self == 0.0 {
            return shrinks;
        }
        shrinks.push(0.0);
        shrinks.push(self.trunc());
        shrinks.push(*self / 2.0);
        shrinks
    }
}

impl Arbitrary for String {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        let len = rng.next_range(size as u64 + 1) as usize;
        (0..len)
            .map(|_| {
                let c = rng.next_range(95) as u8 + 32; // Printable ASCII
                c as char
            })
            .collect()
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if self.is_empty() {
            return shrinks;
        }
        shrinks.push(String::new());
        if self.len() > 1 {
            shrinks.push(self[..self.len() / 2].to_string());
            shrinks.push(self[self.len() / 2..].to_string());
            shrinks.push(self[1..].to_string());
            shrinks.push(self[..self.len() - 1].to_string());
        }
        shrinks
    }
}

impl<T: Arbitrary> Arbitrary for Vec<T> {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        let len = rng.next_range(size as u64 + 1) as usize;
        (0..len).map(|_| T::arbitrary(rng, size)).collect()
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        if self.is_empty() {
            return shrinks;
        }
        
        // Empty vector
        shrinks.push(Vec::new());
        
        // Remove each element
        for i in 0..self.len() {
            let mut v = self.clone();
            v.remove(i);
            shrinks.push(v);
        }
        
        // Shrink each element
        for (i, elem) in self.iter().enumerate() {
            for shrunk in elem.shrink() {
                let mut v = self.clone();
                v[i] = shrunk;
                shrinks.push(v);
            }
        }
        
        shrinks
    }
}

impl<T: Arbitrary> Arbitrary for Option<T> {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        if rng.next_bool() {
            Some(T::arbitrary(rng, size))
        } else {
            None
        }
    }
    
    fn shrink(&self) -> Vec<Self> {
        match self {
            None => Vec::new(),
            Some(v) => {
                let mut shrinks = vec![None];
                for s in v.shrink() {
                    shrinks.push(Some(s));
                }
                shrinks
            }
        }
    }
}

impl<A: Arbitrary, B: Arbitrary> Arbitrary for (A, B) {
    fn arbitrary(rng: &mut TestRng, size: usize) -> Self {
        (A::arbitrary(rng, size), B::arbitrary(rng, size))
    }
    
    fn shrink(&self) -> Vec<Self> {
        let mut shrinks = Vec::new();
        for a in self.0.shrink() {
            shrinks.push((a, self.1.clone()));
        }
        for b in self.1.shrink() {
            shrinks.push((self.0.clone(), b));
        }
        shrinks
    }
}

/// Result of a property-based test.
#[derive(Clone, Debug)]
pub enum PropTestResult<T> {
    /// All tests passed.
    Passed {
        num_tests: usize,
    },
    /// A test failed.
    Failed {
        /// The minimal counterexample.
        counterexample: T,
        /// Original failing input.
        original: T,
        /// Number of tests before failure.
        tests_run: usize,
        /// Number of shrink steps.
        shrink_steps: usize,
        /// Error message.
        message: String,
    },
    /// Testing was exhausted (e.g., generator ran out of values).
    Exhausted {
        tests_run: usize,
    },
}

impl<T: std::fmt::Debug> PropTestResult<T> {
    /// Check if the test passed.
    pub fn is_passed(&self) -> bool {
        matches!(self, PropTestResult::Passed { .. })
    }
    
    /// Panic if the test failed.
    pub fn unwrap(self) {
        match self {
            PropTestResult::Passed { num_tests } => {
                // Success
            }
            PropTestResult::Failed { counterexample, original, tests_run, shrink_steps, message } => {
                panic!(
                    "Property test failed after {} tests!\n\
                     Counterexample: {:?}\n\
                     Original failing input: {:?}\n\
                     Shrink steps: {}\n\
                     Message: {}",
                    tests_run, counterexample, original, shrink_steps, message
                );
            }
            PropTestResult::Exhausted { tests_run } => {
                panic!("Property test exhausted after {} tests", tests_run);
            }
        }
    }
}

/// Run a property-based test.
pub fn prop_test<T, F>(config: &PropTestConfig, mut property: F) -> PropTestResult<T>
where
    T: Arbitrary + std::fmt::Debug,
    F: FnMut(&T) -> bool,
{
    let seed = if config.seed == 0 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345)
    } else {
        config.seed
    };
    
    let mut rng = TestRng::new(seed);
    
    for test_num in 0..config.num_tests {
        let size = (test_num * config.max_size / config.num_tests).max(1);
        let input = T::arbitrary(&mut rng, size);
        
        if !property(&input) {
            // Found a failing case - try to shrink it
            let (minimal, shrink_steps) = shrink_input(
                input.clone(),
                &mut property,
                config.max_shrink_iters,
            );
            
            return PropTestResult::Failed {
                counterexample: minimal,
                original: input,
                tests_run: test_num + 1,
                shrink_steps,
                message: "Property returned false".to_string(),
            };
        }
        
        if config.verbose && (test_num + 1) % 10 == 0 {
            eprintln!("Passed {}/{} tests", test_num + 1, config.num_tests);
        }
    }
    
    PropTestResult::Passed {
        num_tests: config.num_tests,
    }
}

/// Shrink a failing input to find a minimal counterexample.
fn shrink_input<T, F>(initial: T, property: &mut F, max_iters: usize) -> (T, usize)
where
    T: Arbitrary,
    F: FnMut(&T) -> bool,
{
    let mut current = initial;
    let mut steps = 0;
    
    for _ in 0..max_iters {
        let shrinks = current.shrink();
        if shrinks.is_empty() {
            break;
        }
        
        let mut found_smaller = false;
        for shrunk in shrinks {
            if !property(&shrunk) {
                current = shrunk;
                found_smaller = true;
                steps += 1;
                break;
            }
        }
        
        if !found_smaller {
            break;
        }
    }
    
    (current, steps)
}

/// Property test with panic catching.
pub fn prop_test_panics<T, F>(config: &PropTestConfig, mut property: F) -> PropTestResult<T>
where
    T: Arbitrary + std::fmt::Debug + 'static,
    F: FnMut(&T) + 'static,
{
    prop_test(config, move |input| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| property(input))).is_ok()
    })
}

/// Convenience function to run a simple property test.
pub fn quickcheck<T, F>(property: F) -> PropTestResult<T>
where
    T: Arbitrary + std::fmt::Debug,
    F: FnMut(&T) -> bool,
{
    prop_test(&PropTestConfig::default(), property)
}

/// Assert a property holds.
#[macro_export]
macro_rules! assert_property {
    ($property:expr) => {
        $crate::testing::quickcheck($property).unwrap()
    };
    ($property:expr, $config:expr) => {
        $crate::testing::prop_test(&$config, $property).unwrap()
    };
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    
    #[test]
    fn test_arbitrary_bool() {
        let mut rng = TestRng::new(42);
        let values: Vec<bool> = (0..100).map(|_| bool::arbitrary(&mut rng, 10)).collect();
        assert!(values.contains(&true));
        assert!(values.contains(&false));
    }
    
    #[test]
    fn test_arbitrary_i32() {
        let mut rng = TestRng::new(42);
        let values: Vec<i32> = (0..100).map(|_| i32::arbitrary(&mut rng, 100)).collect();
        assert!(values.iter().any(|&x| x > 0));
        assert!(values.iter().any(|&x| x < 0));
    }
    
    #[test]
    fn test_arbitrary_string() {
        let mut rng = TestRng::new(42);
        let values: Vec<String> = (0..100).map(|_| String::arbitrary(&mut rng, 20)).collect();
        assert!(values.iter().any(|s| !s.is_empty()));
        assert!(values.iter().all(|s| s.len() <= 20));
    }
    
    #[test]
    fn test_arbitrary_vec() {
        let mut rng = TestRng::new(42);
        let values: Vec<Vec<i32>> = (0..100).map(|_| Vec::<i32>::arbitrary(&mut rng, 10)).collect();
        assert!(values.iter().any(|v| !v.is_empty()));
    }
    
    #[test]
    fn test_shrink_i32() {
        let shrinks = 42i32.shrink();
        assert!(shrinks.contains(&0));
        assert!(shrinks.contains(&41));
        assert!(shrinks.contains(&21));
    }
    
    #[test]
    fn test_shrink_vec() {
        let v = vec![1, 2, 3];
        let shrinks = v.shrink();
        assert!(shrinks.contains(&vec![]));
        assert!(shrinks.contains(&vec![2, 3]));
        assert!(shrinks.contains(&vec![1, 3]));
        assert!(shrinks.contains(&vec![1, 2]));
    }
    
    #[test]
    fn test_prop_test_pass() {
        let result: PropTestResult<i32> = prop_test(
            &PropTestConfig::new().with_tests(50),
            |x| x + 0 == *x, // Identity property
        );
        assert!(result.is_passed());
    }
    
    #[test]
    fn test_prop_test_fail_with_shrink() {
        let result: PropTestResult<i32> = prop_test(
            &PropTestConfig::new().with_tests(100).with_seed(42),
            |x| *x < 50, // Will fail for some x >= 50
        );
        
        match result {
            PropTestResult::Failed { counterexample, .. } => {
                assert!(counterexample >= 50);
                // Shrinking should find the minimal counterexample: 50
                assert_eq!(counterexample, 50);
            }
            _ => panic!("Expected test to fail"),
        }
    }
    
    #[test]
    fn test_quickcheck() {
        // Test that addition is commutative
        let result: PropTestResult<(i32, i32)> = quickcheck(|(a, b): &(i32, i32)| {
            a.wrapping_add(*b) == b.wrapping_add(*a)
        });
        assert!(result.is_passed());
    }
    
    #[test]
    fn test_string_property() {
        // Test that reversing twice gives original
        let result: PropTestResult<String> = quickcheck(|s: &String| {
            let reversed: String = s.chars().rev().collect();
            let double_reversed: String = reversed.chars().rev().collect();
            *s == double_reversed
        });
        assert!(result.is_passed());
    }
    
    #[test]
    fn test_vec_property() {
        // Test that sorting is idempotent
        let result: PropTestResult<Vec<i32>> = quickcheck(|v: &Vec<i32>| {
            let mut sorted1 = v.clone();
            sorted1.sort();
            let mut sorted2 = sorted1.clone();
            sorted2.sort();
            sorted1 == sorted2
        });
        assert!(result.is_passed());
    }
}

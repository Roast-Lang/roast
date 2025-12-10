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


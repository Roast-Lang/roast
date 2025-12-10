//! Code coverage collection and reporting.
//!
//! Tracks which lines/branches of code are executed during test runs.

use std::collections::{HashMap, HashSet, BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::fs;

/// Coverage data for a single file.
#[derive(Debug, Clone, Default)]
pub struct FileCoverage {
    /// Path to the source file.
    pub path: PathBuf,
    /// Lines that were executed (line number -> hit count).
    pub lines_hit: HashMap<u32, u64>,
    /// Total executable lines in the file.
    pub executable_lines: HashSet<u32>,
    /// Branches that were executed (branch_id -> (true_count, false_count)).
    pub branches: HashMap<u32, (u64, u64)>,
    /// Function coverage (function_name -> hit_count).
    pub functions: HashMap<String, u64>,
}

impl FileCoverage {
    /// Create new file coverage tracker.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lines_hit: HashMap::new(),
            executable_lines: HashSet::new(),
            branches: HashMap::new(),
            functions: HashMap::new(),
        }
    }
    
    /// Record a line hit.
    pub fn hit_line(&mut self, line: u32) {
        *self.lines_hit.entry(line).or_insert(0) += 1;
    }
    
    /// Record a branch hit.
    pub fn hit_branch(&mut self, branch_id: u32, taken: bool) {
        let entry = self.branches.entry(branch_id).or_insert((0, 0));
        if taken {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    
    /// Record a function hit.
    pub fn hit_function(&mut self, name: &str) {
        *self.functions.entry(name.to_string()).or_insert(0) += 1;
    }
    
    /// Mark a line as executable.
    pub fn mark_executable(&mut self, line: u32) {
        self.executable_lines.insert(line);
    }
    
    /// Get line coverage percentage.
    pub fn line_coverage_percent(&self) -> f64 {
        if self.executable_lines.is_empty() {
            return 100.0;
        }
        let hit_count = self.executable_lines.iter()
            .filter(|line| self.lines_hit.contains_key(line))
            .count();
        (hit_count as f64 / self.executable_lines.len() as f64) * 100.0
    }
    
    /// Get branch coverage percentage.
    pub fn branch_coverage_percent(&self) -> f64 {
        if self.branches.is_empty() {
            return 100.0;
        }
        let covered = self.branches.values()
            .filter(|(t, f)| *t > 0 && *f > 0)
            .count();
        (covered as f64 / self.branches.len() as f64) * 100.0
    }
    
    /// Get function coverage percentage.
    pub fn function_coverage_percent(&self) -> f64 {
        if self.functions.is_empty() {
            return 100.0;
        }
        let hit = self.functions.values().filter(|&&c| c > 0).count();
        (hit as f64 / self.functions.len() as f64) * 100.0
    }
    
    /// Get lines that were not hit.
    pub fn uncovered_lines(&self) -> Vec<u32> {
        let mut lines: Vec<_> = self.executable_lines.iter()
            .filter(|line| !self.lines_hit.contains_key(line))
            .copied()
            .collect();
        lines.sort();
        lines
    }
    
    /// Get ranges of uncovered lines (for cleaner output).
    pub fn uncovered_line_ranges(&self) -> Vec<(u32, u32)> {
        let uncovered = self.uncovered_lines();
        if uncovered.is_empty() {
            return Vec::new();
        }
        
        let mut ranges = Vec::new();
        let mut start = uncovered[0];
        let mut end = start;
        
        for &line in &uncovered[1..] {
            if line == end + 1 {
                end = line;
            } else {
                ranges.push((start, end));
                start = line;
                end = line;
            }
        }
        ranges.push((start, end));
        
        ranges
    }
}

/// Coverage collector that aggregates coverage from multiple files.
#[derive(Debug, Default)]
pub struct CoverageCollector {
    /// Coverage data per file.
    files: HashMap<PathBuf, FileCoverage>,
    /// Whether collection is enabled.
    enabled: bool,
    /// Files to include (if empty, include all).
    include_patterns: Vec<String>,
    /// Files to exclude.
    exclude_patterns: Vec<String>,
}

impl CoverageCollector {
    /// Create a new coverage collector.
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            enabled: false,
            include_patterns: Vec::new(),
            exclude_patterns: vec![
                "**/test_*.roast".to_string(),
                "**/*_test.roast".to_string(),
            ],
        }
    }
    
    /// Enable coverage collection.
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    /// Disable coverage collection.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    
    /// Check if coverage is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Add include pattern.
    pub fn include(&mut self, pattern: &str) {
        self.include_patterns.push(pattern.to_string());
    }
    
    /// Add exclude pattern.
    pub fn exclude(&mut self, pattern: &str) {
        self.exclude_patterns.push(pattern.to_string());
    }
    
    /// Check if a file should be tracked.
    fn should_track(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        // Check excludes
        for pattern in &self.exclude_patterns {
            if matches_glob(pattern, &path_str) {
                return false;
            }
        }
        
        // If no includes specified, include all
        if self.include_patterns.is_empty() {
            return true;
        }
        
        // Check includes
        for pattern in &self.include_patterns {
            if matches_glob(pattern, &path_str) {
                return true;
            }
        }
        
        false
    }
    
    /// Get or create coverage for a file.
    pub fn get_file(&mut self, path: &Path) -> Option<&mut FileCoverage> {
        if !self.enabled || !self.should_track(path) {
            return None;
        }
        
        let path_buf = path.to_path_buf();
        if !self.files.contains_key(&path_buf) {
            self.files.insert(path_buf.clone(), FileCoverage::new(path_buf.clone()));
        }
        self.files.get_mut(&path_buf)
    }
    
    /// Record a line hit.
    pub fn hit_line(&mut self, path: &Path, line: u32) {
        if let Some(file) = self.get_file(path) {
            file.hit_line(line);
        }
    }
    
    /// Record a branch hit.
    pub fn hit_branch(&mut self, path: &Path, branch_id: u32, taken: bool) {
        if let Some(file) = self.get_file(path) {
            file.hit_branch(branch_id, taken);
        }
    }
    
    /// Record a function hit.
    pub fn hit_function(&mut self, path: &Path, name: &str) {
        if let Some(file) = self.get_file(path) {
            file.hit_function(name);
        }
    }
    
    /// Register executable lines for a file.
    pub fn register_executable_lines(&mut self, path: &Path, lines: &[u32]) {
        if let Some(file) = self.get_file(path) {
            for &line in lines {
                file.mark_executable(line);
            }
        }
    }
    
    /// Merge another collector into this one.
    pub fn merge(&mut self, other: &CoverageCollector) {
        for (path, other_file) in &other.files {
            if let Some(file) = self.files.get_mut(path) {
                // Merge line hits
                for (&line, &count) in &other_file.lines_hit {
                    *file.lines_hit.entry(line).or_insert(0) += count;
                }
                
                // Merge executable lines
                file.executable_lines.extend(&other_file.executable_lines);
                
                // Merge branches
                for (&branch_id, &(t, f)) in &other_file.branches {
                    let entry = file.branches.entry(branch_id).or_insert((0, 0));
                    entry.0 += t;
                    entry.1 += f;
                }
                
                // Merge functions
                for (name, &count) in &other_file.functions {
                    *file.functions.entry(name.clone()).or_insert(0) += count;
                }
            } else {
                self.files.insert(path.clone(), other_file.clone());
            }
        }
    }
    
    /// Get overall statistics.
    pub fn summary(&self) -> CoverageSummary {
        let mut total_lines = 0u64;
        let mut covered_lines = 0u64;
        let mut total_branches = 0u64;
        let mut covered_branches = 0u64;
        let mut total_functions = 0u64;
        let mut covered_functions = 0u64;
        
        for file in self.files.values() {
            total_lines += file.executable_lines.len() as u64;
            covered_lines += file.executable_lines.iter()
                .filter(|line| file.lines_hit.contains_key(line))
                .count() as u64;
            
            total_branches += file.branches.len() as u64;
            covered_branches += file.branches.values()
                .filter(|(t, f)| *t > 0 && *f > 0)
                .count() as u64;
            
            total_functions += file.functions.len() as u64;
            covered_functions += file.functions.values()
                .filter(|&&c| c > 0)
                .count() as u64;
        }
        
        CoverageSummary {
            files: self.files.len(),
            total_lines,
            covered_lines,
            total_branches,
            covered_branches,
            total_functions,
            covered_functions,
        }
    }
    
    /// Generate a text report.
    pub fn report_text(&self) -> String {
        let mut output = String::new();
        let summary = self.summary();
        
        output.push_str(&"=".repeat(80));
        output.push_str("\n                         COVERAGE REPORT\n");
        output.push_str(&"=".repeat(80));
        output.push('\n');
        
        // File details
        let mut paths: Vec<_> = self.files.keys().collect();
        paths.sort();
        
        output.push_str(&format!("{:<50} {:>8} {:>8} {:>8}\n", 
            "File", "Lines", "Branch", "Funcs"));
        output.push_str(&"-".repeat(80));
        output.push('\n');
        
        for path in paths {
            let file = &self.files[path];
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            
            output.push_str(&format!("{:<50} {:>7.1}% {:>7.1}% {:>7.1}%\n",
                filename,
                file.line_coverage_percent(),
                file.branch_coverage_percent(),
                file.function_coverage_percent(),
            ));
            
            // Show uncovered lines
            let uncovered = file.uncovered_line_ranges();
            if !uncovered.is_empty() {
                let ranges: Vec<String> = uncovered.iter()
                    .map(|(start, end)| {
                        if start == end {
                            format!("{}", start)
                        } else {
                            format!("{}-{}", start, end)
                        }
                    })
                    .collect();
                output.push_str(&format!("  Missing: {}\n", ranges.join(", ")));
            }
        }
        
        output.push_str(&"-".repeat(80));
        output.push('\n');
        
        // Summary
        output.push_str(&format!("{:<50} {:>7.1}% {:>7.1}% {:>7.1}%\n",
            "TOTAL",
            summary.line_percent(),
            summary.branch_percent(),
            summary.function_percent(),
        ));
        
        output.push_str(&format!("\nCovered: {} / {} lines, {} / {} branches, {} / {} functions\n",
            summary.covered_lines, summary.total_lines,
            summary.covered_branches, summary.total_branches,
            summary.covered_functions, summary.total_functions,
        ));
        
        output.push_str(&"=".repeat(80));
        output.push('\n');
        
        output
    }
    
    /// Generate an HTML report.
    pub fn report_html(&self, output_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(output_dir)?;
        
        let summary = self.summary();
        
        // Generate index.html
        let mut index = String::new();
        index.push_str(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Coverage Report</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
        h1 { color: #333; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f5f5f5; }
        tr:nth-child(even) { background-color: #fafafa; }
        .good { color: #22863a; }
        .warn { color: #b08800; }
        .bad { color: #cb2431; }
        .bar { height: 10px; background: #eee; border-radius: 5px; overflow: hidden; }
        .bar-fill { height: 100%; }
        .bar-good { background: #22863a; }
        .bar-warn { background: #b08800; }
        .bar-bad { background: #cb2431; }
        .summary { background: #f0f0f0; padding: 15px; border-radius: 5px; margin: 20px 0; }
    </style>
</head>
<body>
    <h1>Coverage Report</h1>
    <div class="summary">
        <strong>Summary:</strong>
"#);
        
        index.push_str(&format!(
            "Lines: {:.1}% ({}/{}) | Branches: {:.1}% ({}/{}) | Functions: {:.1}% ({}/{})",
            summary.line_percent(),
            summary.covered_lines, summary.total_lines,
            summary.branch_percent(),
            summary.covered_branches, summary.total_branches,
            summary.function_percent(),
            summary.covered_functions, summary.total_functions,
        ));
        
        index.push_str(r#"
    </div>
    <table>
        <tr>
            <th>File</th>
            <th>Lines</th>
            <th>Branches</th>
            <th>Functions</th>
        </tr>
"#);
        
        let mut paths: Vec<_> = self.files.keys().collect();
        paths.sort();
        
        for path in paths {
            let file = &self.files[path];
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            
            let line_pct = file.line_coverage_percent();
            let branch_pct = file.branch_coverage_percent();
            let func_pct = file.function_coverage_percent();
            
            let line_class = coverage_class(line_pct);
            let branch_class = coverage_class(branch_pct);
            let func_class = coverage_class(func_pct);
            
            index.push_str(&format!(
                r#"        <tr>
            <td><a href="{}.html">{}</a></td>
            <td class="{}">
                <div class="bar"><div class="bar-fill bar-{}" style="width: {:.0}%"></div></div>
                {:.1}%
            </td>
            <td class="{}">
                <div class="bar"><div class="bar-fill bar-{}" style="width: {:.0}%"></div></div>
                {:.1}%
            </td>
            <td class="{}">
                <div class="bar"><div class="bar-fill bar-{}" style="width: {:.0}%"></div></div>
                {:.1}%
            </td>
        </tr>
"#,
                filename, filename,
                line_class, line_class, line_pct, line_pct,
                branch_class, branch_class, branch_pct, branch_pct,
                func_class, func_class, func_pct, func_pct,
            ));
            
            // Generate per-file HTML
            self.generate_file_html(file, output_dir)?;
        }
        
        index.push_str("    </table>\n</body>\n</html>");
        
        fs::write(output_dir.join("index.html"), index)?;
        
        Ok(())
    }
    
    /// Generate HTML for a single file.
    fn generate_file_html(&self, file: &FileCoverage, output_dir: &Path) -> std::io::Result<()> {
        let filename = file.path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.path.to_string_lossy().to_string());
        
        // Try to read source file
        let source = fs::read_to_string(&file.path).unwrap_or_default();
        
        let mut html = String::new();
        html.push_str(&format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>{} - Coverage</title>
    <style>
        body {{ font-family: monospace; margin: 20px; }}
        h1 {{ font-family: sans-serif; }}
        .source {{ border: 1px solid #ddd; }}
        .line {{ display: flex; }}
        .line-no {{ width: 50px; text-align: right; padding-right: 10px; color: #999; background: #f5f5f5; user-select: none; }}
        .line-hits {{ width: 50px; text-align: right; padding-right: 10px; color: #666; background: #f5f5f5; }}
        .line-code {{ flex: 1; padding-left: 10px; white-space: pre; }}
        .covered {{ background: #e6ffed; }}
        .uncovered {{ background: #ffeef0; }}
        .not-executable {{ background: #fff; }}
        a {{ color: #0366d6; }}
    </style>
</head>
<body>
    <h1><a href="index.html">Coverage</a> / {}</h1>
    <p>Lines: {:.1}% | Branches: {:.1}% | Functions: {:.1}%</p>
    <div class="source">
"#, filename, filename,
    file.line_coverage_percent(),
    file.branch_coverage_percent(),
    file.function_coverage_percent(),
        ));
        
        for (i, line) in source.lines().enumerate() {
            let line_no = (i + 1) as u32;
            let hits = file.lines_hit.get(&line_no).copied().unwrap_or(0);
            let is_executable = file.executable_lines.contains(&line_no);
            
            let class = if !is_executable {
                "not-executable"
            } else if hits > 0 {
                "covered"
            } else {
                "uncovered"
            };
            
            let hits_str = if is_executable {
                if hits > 0 { format!("{}x", hits) } else { "!".to_string() }
            } else {
                String::new()
            };
            
            let escaped_line = html_escape(line);
            
            html.push_str(&format!(
                r#"        <div class="line {}">
            <span class="line-no">{}</span>
            <span class="line-hits">{}</span>
            <span class="line-code">{}</span>
        </div>
"#,
                class, line_no, hits_str, escaped_line,
            ));
        }
        
        html.push_str("    </div>\n</body>\n</html>");
        
        fs::write(output_dir.join(format!("{}.html", filename)), html)?;
        
        Ok(())
    }
    
    /// Generate LCOV format report.
    pub fn report_lcov(&self) -> String {
        let mut output = String::new();
        
        for (path, file) in &self.files {
            output.push_str(&format!("SF:{}\n", path.display()));
            
            // Functions
            for (name, &count) in &file.functions {
                output.push_str(&format!("FN:0,{}\n", name));
                output.push_str(&format!("FNDA:{},{}\n", count, name));
            }
            output.push_str(&format!("FNF:{}\n", file.functions.len()));
            output.push_str(&format!("FNH:{}\n", 
                file.functions.values().filter(|&&c| c > 0).count()));
            
            // Branches
            for (&branch_id, &(t, f)) in &file.branches {
                output.push_str(&format!("BRDA:0,{},0,{}\n", branch_id, t));
                output.push_str(&format!("BRDA:0,{},1,{}\n", branch_id, f));
            }
            output.push_str(&format!("BRF:{}\n", file.branches.len() * 2));
            output.push_str(&format!("BRH:{}\n",
                file.branches.values()
                    .map(|(t, f)| (if *t > 0 { 1 } else { 0 }) + (if *f > 0 { 1 } else { 0 }))
                    .sum::<usize>()));
            
            // Lines
            for &line in &file.executable_lines {
                let hits = file.lines_hit.get(&line).copied().unwrap_or(0);
                output.push_str(&format!("DA:{},{}\n", line, hits));
            }
            output.push_str(&format!("LF:{}\n", file.executable_lines.len()));
            output.push_str(&format!("LH:{}\n",
                file.executable_lines.iter()
                    .filter(|l| file.lines_hit.contains_key(l))
                    .count()));
            
            output.push_str("end_of_record\n");
        }
        
        output
    }
    
    /// Clear all coverage data.
    pub fn clear(&mut self) {
        self.files.clear();
    }
}

/// Coverage summary statistics.
#[derive(Debug, Clone)]
pub struct CoverageSummary {
    pub files: usize,
    pub total_lines: u64,
    pub covered_lines: u64,
    pub total_branches: u64,
    pub covered_branches: u64,
    pub total_functions: u64,
    pub covered_functions: u64,
}

impl CoverageSummary {
    /// Get line coverage percentage.
    pub fn line_percent(&self) -> f64 {
        if self.total_lines == 0 {
            return 100.0;
        }
        (self.covered_lines as f64 / self.total_lines as f64) * 100.0
    }
    
    /// Get branch coverage percentage.
    pub fn branch_percent(&self) -> f64 {
        if self.total_branches == 0 {
            return 100.0;
        }
        (self.covered_branches as f64 / self.total_branches as f64) * 100.0
    }
    
    /// Get function coverage percentage.
    pub fn function_percent(&self) -> f64 {
        if self.total_functions == 0 {
            return 100.0;
        }
        (self.covered_functions as f64 / self.total_functions as f64) * 100.0
    }
}

/// Thread-safe coverage collector.
#[derive(Debug, Default, Clone)]
pub struct SharedCoverage {
    inner: Arc<RwLock<CoverageCollector>>,
}

impl SharedCoverage {
    /// Create a new shared coverage collector.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CoverageCollector::new())),
        }
    }
    
    /// Enable coverage collection.
    pub fn enable(&self) {
        self.inner.write().unwrap().enable();
    }
    
    /// Disable coverage collection.
    pub fn disable(&self) {
        self.inner.write().unwrap().disable();
    }
    
    /// Check if coverage is enabled.
    pub fn is_enabled(&self) -> bool {
        self.inner.read().unwrap().is_enabled()
    }
    
    /// Record a line hit.
    pub fn hit_line(&self, path: &Path, line: u32) {
        self.inner.write().unwrap().hit_line(path, line);
    }
    
    /// Record a branch hit.
    pub fn hit_branch(&self, path: &Path, branch_id: u32, taken: bool) {
        self.inner.write().unwrap().hit_branch(path, branch_id, taken);
    }
    
    /// Record a function hit.
    pub fn hit_function(&self, path: &Path, name: &str) {
        self.inner.write().unwrap().hit_function(path, name);
    }
    
    /// Register executable lines.
    pub fn register_executable_lines(&self, path: &Path, lines: &[u32]) {
        self.inner.write().unwrap().register_executable_lines(path, lines);
    }
    
    /// Get summary.
    pub fn summary(&self) -> CoverageSummary {
        self.inner.read().unwrap().summary()
    }
    
    /// Generate text report.
    pub fn report_text(&self) -> String {
        self.inner.read().unwrap().report_text()
    }
    
    /// Generate HTML report.
    pub fn report_html(&self, output_dir: &Path) -> std::io::Result<()> {
        self.inner.read().unwrap().report_html(output_dir)
    }
    
    /// Generate LCOV report.
    pub fn report_lcov(&self) -> String {
        self.inner.read().unwrap().report_lcov()
    }
    
    /// Clear coverage data.
    pub fn clear(&self) {
        self.inner.write().unwrap().clear();
    }
}

// Helper functions

fn coverage_class(percent: f64) -> &'static str {
    if percent >= 80.0 {
        "good"
    } else if percent >= 50.0 {
        "warn"
    } else {
        "bad"
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn matches_glob(pattern: &str, path: &str) -> bool {
    // Simple glob matching - supports * and **
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    
    matches_glob_parts(&pattern_parts, &path_parts)
}

fn matches_glob_parts(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() && path.is_empty() {
        return true;
    }
    if pattern.is_empty() {
        return false;
    }
    
    let p = pattern[0];
    
    if p == "**" {
        // ** matches zero or more directories
        // Try matching with zero directories consumed
        if matches_glob_parts(&pattern[1..], path) {
            return true;
        }
        // Try matching with one or more directories consumed
        for i in 0..path.len() {
            if matches_glob_parts(&pattern[1..], &path[i..]) {
                return true;
            }
        }
        return false;
    }
    
    if path.is_empty() {
        return false;
    }
    
    // Match current segment
    if matches_segment(p, path[0]) {
        matches_glob_parts(&pattern[1..], &path[1..])
    } else {
        false
    }
}

fn matches_segment(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    
    // Simple wildcard matching
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            // Pattern like "*.rs" or "test_*"
            let (prefix, suffix) = (parts[0], parts[1]);
            return segment.starts_with(prefix) && segment.ends_with(suffix);
        }
    }
    
    pattern == segment
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_file_coverage() {
        let mut cov = FileCoverage::new(PathBuf::from("test.roast"));
        
        cov.mark_executable(1);
        cov.mark_executable(2);
        cov.mark_executable(3);
        cov.mark_executable(4);
        
        cov.hit_line(1);
        cov.hit_line(2);
        cov.hit_line(2);
        
        assert_eq!(cov.line_coverage_percent(), 50.0);
        assert_eq!(cov.uncovered_lines(), vec![3, 4]);
    }
    
    #[test]
    fn test_coverage_collector() {
        let mut collector = CoverageCollector::new();
        collector.enable();
        
        let path = Path::new("example.roast");
        collector.register_executable_lines(path, &[1, 2, 3, 4, 5]);
        collector.hit_line(path, 1);
        collector.hit_line(path, 2);
        collector.hit_line(path, 3);
        
        let summary = collector.summary();
        assert_eq!(summary.total_lines, 5);
        assert_eq!(summary.covered_lines, 3);
        assert_eq!(summary.line_percent(), 60.0);
    }
    
    #[test]
    fn test_uncovered_ranges() {
        let mut cov = FileCoverage::new(PathBuf::from("test.roast"));
        
        for i in 1..=20 {
            cov.mark_executable(i);
        }
        
        // Hit some lines, leave gaps
        for i in [1, 2, 3, 7, 8, 15, 16, 17, 18, 19, 20] {
            cov.hit_line(i);
        }
        
        let ranges = cov.uncovered_line_ranges();
        assert_eq!(ranges, vec![(4, 6), (9, 14)]);
    }
}

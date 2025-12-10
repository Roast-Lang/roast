//! Breakpoint management.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Breakpoint ID.
pub type BreakpointId = i64;

/// Breakpoint state.
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Unique ID.
    pub id: BreakpointId,
    /// Source file.
    pub source: PathBuf,
    /// Line number (1-based).
    pub line: i64,
    /// Column (optional).
    pub column: Option<i64>,
    /// Condition expression.
    pub condition: Option<String>,
    /// Hit condition.
    pub hit_condition: Option<String>,
    /// Log message (logpoint).
    pub log_message: Option<String>,
    /// Is verified.
    pub verified: bool,
    /// Hit count.
    pub hit_count: u64,
    /// Is enabled.
    pub enabled: bool,
}

impl Breakpoint {
    /// Create a new breakpoint.
    pub fn new(id: BreakpointId, source: PathBuf, line: i64) -> Self {
        Self {
            id,
            source,
            line,
            column: None,
            condition: None,
            hit_condition: None,
            log_message: None,
            verified: false,
            hit_count: 0,
            enabled: true,
        }
    }
    
    /// Check if breakpoint matches location.
    pub fn matches(&self, source: &PathBuf, line: i64) -> bool {
        self.enabled && self.source == *source && self.line == line
    }
    
    /// Increment hit count and check hit condition.
    pub fn hit(&mut self) -> bool {
        self.hit_count += 1;
        
        // Check hit condition
        if let Some(ref cond) = self.hit_condition {
            // Parse simple conditions like ">10", "==5", ">=3"
            if let Some(result) = self.check_hit_condition(cond) {
                return result;
            }
        }
        
        true
    }
    
    fn check_hit_condition(&self, cond: &str) -> Option<bool> {
        let cond = cond.trim();
        
        if let Some(n) = cond.strip_prefix(">=") {
            n.trim().parse::<u64>().ok().map(|n| self.hit_count >= n)
        } else if let Some(n) = cond.strip_prefix("<=") {
            n.trim().parse::<u64>().ok().map(|n| self.hit_count <= n)
        } else if let Some(n) = cond.strip_prefix("==") {
            n.trim().parse::<u64>().ok().map(|n| self.hit_count == n)
        } else if let Some(n) = cond.strip_prefix('>') {
            n.trim().parse::<u64>().ok().map(|n| self.hit_count > n)
        } else if let Some(n) = cond.strip_prefix('<') {
            n.trim().parse::<u64>().ok().map(|n| self.hit_count < n)
        } else if let Some(n) = cond.strip_prefix('%') {
            // Modulo: break every N hits
            n.trim().parse::<u64>().ok().map(|n| n > 0 && self.hit_count % n == 0)
        } else {
            // Try as plain number (break on Nth hit)
            cond.parse::<u64>().ok().map(|n| self.hit_count == n)
        }
    }
}

/// Manages breakpoints.
pub struct BreakpointManager {
    /// Next breakpoint ID.
    next_id: AtomicI64,
    /// Breakpoints by ID.
    breakpoints: HashMap<BreakpointId, Breakpoint>,
    /// Index by source file.
    by_source: HashMap<PathBuf, Vec<BreakpointId>>,
}

impl BreakpointManager {
    /// Create a new manager.
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
            breakpoints: HashMap::new(),
            by_source: HashMap::new(),
        }
    }
    
    /// Add a breakpoint.
    pub fn add(&mut self, source: PathBuf, line: i64) -> &Breakpoint {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let bp = Breakpoint::new(id, source.clone(), line);
        
        self.by_source
            .entry(source)
            .or_insert_with(Vec::new)
            .push(id);
        
        self.breakpoints.insert(id, bp);
        self.breakpoints.get(&id).unwrap()
    }
    
    /// Add a conditional breakpoint.
    pub fn add_conditional(
        &mut self,
        source: PathBuf,
        line: i64,
        condition: Option<String>,
        hit_condition: Option<String>,
        log_message: Option<String>,
    ) -> &Breakpoint {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut bp = Breakpoint::new(id, source.clone(), line);
        bp.condition = condition;
        bp.hit_condition = hit_condition;
        bp.log_message = log_message;
        
        self.by_source
            .entry(source)
            .or_insert_with(Vec::new)
            .push(id);
        
        self.breakpoints.insert(id, bp);
        self.breakpoints.get(&id).unwrap()
    }
    
    /// Remove a breakpoint.
    pub fn remove(&mut self, id: BreakpointId) -> Option<Breakpoint> {
        if let Some(bp) = self.breakpoints.remove(&id) {
            if let Some(ids) = self.by_source.get_mut(&bp.source) {
                ids.retain(|&i| i != id);
            }
            Some(bp)
        } else {
            None
        }
    }
    
    /// Clear all breakpoints for a source.
    pub fn clear_source(&mut self, source: &PathBuf) -> Vec<Breakpoint> {
        let mut removed = Vec::new();
        
        if let Some(ids) = self.by_source.remove(source) {
            for id in ids {
                if let Some(bp) = self.breakpoints.remove(&id) {
                    removed.push(bp);
                }
            }
        }
        
        removed
    }
    
    /// Clear all breakpoints.
    pub fn clear_all(&mut self) {
        self.breakpoints.clear();
        self.by_source.clear();
    }
    
    /// Get a breakpoint by ID.
    pub fn get(&self, id: BreakpointId) -> Option<&Breakpoint> {
        self.breakpoints.get(&id)
    }
    
    /// Get mutable breakpoint by ID.
    pub fn get_mut(&mut self, id: BreakpointId) -> Option<&mut Breakpoint> {
        self.breakpoints.get_mut(&id)
    }
    
    /// Get breakpoints for a source.
    pub fn get_for_source(&self, source: &PathBuf) -> Vec<&Breakpoint> {
        self.by_source
            .get(source)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.breakpoints.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Check if there's a breakpoint at location.
    pub fn check(&self, source: &PathBuf, line: i64) -> Option<&Breakpoint> {
        self.get_for_source(source)
            .into_iter()
            .find(|bp| bp.matches(source, line))
    }
    
    /// Verify a breakpoint.
    pub fn verify(&mut self, id: BreakpointId, verified: bool) {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.verified = verified;
        }
    }
    
    /// Enable/disable a breakpoint.
    pub fn set_enabled(&mut self, id: BreakpointId, enabled: bool) {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.enabled = enabled;
        }
    }
    
    /// All breakpoints.
    pub fn all(&self) -> impl Iterator<Item = &Breakpoint> {
        self.breakpoints.values()
    }
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_breakpoint_hit_condition() {
        let mut bp = Breakpoint::new(1, PathBuf::from("test.roast"), 10);
        bp.hit_condition = Some(">3".to_string());
        
        assert!(!bp.hit()); // hit_count = 1
        assert!(!bp.hit()); // hit_count = 2
        assert!(!bp.hit()); // hit_count = 3
        assert!(bp.hit());  // hit_count = 4, > 3
    }
    
    #[test]
    fn test_manager() {
        let mut mgr = BreakpointManager::new();
        
        let source = PathBuf::from("main.roast");
        let bp1 = mgr.add(source.clone(), 10);
        let id1 = bp1.id;
        
        let bp2 = mgr.add(source.clone(), 20);
        let id2 = bp2.id;
        
        assert_eq!(mgr.get_for_source(&source).len(), 2);
        
        mgr.remove(id1);
        assert_eq!(mgr.get_for_source(&source).len(), 1);
        
        mgr.clear_all();
        assert_eq!(mgr.get_for_source(&source).len(), 0);
    }
}


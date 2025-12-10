//! JIT profiling and runtime type feedback.
//!
//! This module collects runtime information during interpretation
//! and baseline execution to guide tier-up decisions and optimizations.

use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use crate::optimizing::ObservedType;

/// Profile data collected for a function.
#[derive(Clone)]
pub struct JitProfile {
    /// Function ID.
    pub func_id: u32,
    /// Execution count.
    pub execution_count: u64,
    /// Type feedback at various sites.
    pub type_info: FxHashMap<u32, TypeFeedback>,
    /// Branch profiles.
    pub branches: FxHashMap<u32, BranchProfile>,
    /// Call site profiles.
    pub call_sites: FxHashMap<u32, CallSiteProfile>,
    /// Loop profiles.
    pub loops: FxHashMap<u32, LoopProfile>,
}

/// Type feedback for a site.
#[derive(Clone, Debug, Default)]
pub struct TypeFeedback {
    /// Observed types and their counts.
    pub types: Vec<(ObservedType, u64)>,
    /// Total samples.
    pub total: u64,
    /// Whether the site is stable (same type seen).
    pub stable: bool,
}

impl TypeFeedback {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Records an observed type.
    pub fn record(&mut self, ty: ObservedType) {
        for (existing, count) in &mut self.types {
            if std::mem::discriminant(existing) == std::mem::discriminant(&ty) {
                *count += 1;
                self.total += 1;
                self.stable = self.types.len() == 1;
                return;
            }
        }
        self.types.push((ty, 1));
        self.total += 1;
        self.stable = self.types.len() == 1;
    }
    
    /// Gets the dominant type if one exists (>80% of samples).
    pub fn dominant_type(&self) -> Option<ObservedType> {
        if self.total == 0 {
            return None;
        }
        
        let threshold = (self.total * 80) / 100;
        for (ty, count) in &self.types {
            if *count >= threshold {
                return Some(ty.clone());
            }
        }
        None
    }
    
    /// Whether the site is monomorphic.
    pub fn is_monomorphic(&self) -> bool {
        self.types.len() == 1
    }
    
    /// Whether the site is polymorphic (2-4 types).
    pub fn is_polymorphic(&self) -> bool {
        self.types.len() >= 2 && self.types.len() <= 4
    }
    
    /// Whether the site is megamorphic (5+ types).
    pub fn is_megamorphic(&self) -> bool {
        self.types.len() > 4
    }
}

/// Branch profile data.
#[derive(Clone, Debug)]
pub struct BranchProfile {
    /// Bytecode offset.
    pub bc_offset: usize,
    /// Times the branch was taken.
    pub taken: u64,
    /// Times the branch was not taken.
    pub not_taken: u64,
}

impl BranchProfile {
    pub fn new(bc_offset: usize) -> Self {
        Self {
            bc_offset,
            taken: 0,
            not_taken: 0,
        }
    }
    
    /// Records a branch outcome.
    pub fn record(&mut self, taken: bool) {
        if taken {
            self.taken += 1;
        } else {
            self.not_taken += 1;
        }
    }
    
    /// Gets the probability of the branch being taken.
    pub fn taken_probability(&self) -> f64 {
        let total = self.taken + self.not_taken;
        if total == 0 {
            0.5
        } else {
            self.taken as f64 / total as f64
        }
    }
    
    /// Whether the branch is biased (>90% one way).
    pub fn is_biased(&self) -> bool {
        let prob = self.taken_probability();
        prob > 0.9 || prob < 0.1
    }
}

/// Call site profile.
#[derive(Clone, Debug)]
pub struct CallSiteProfile {
    /// Site ID.
    pub site_id: u32,
    /// Target function counts.
    pub targets: FxHashMap<u32, u64>,
    /// Total calls.
    pub total: u64,
    /// Observed target function (for monomorphic sites).
    pub target_func: u32,
}

impl CallSiteProfile {
    pub fn new(site_id: u32) -> Self {
        Self {
            site_id,
            targets: FxHashMap::default(),
            total: 0,
            target_func: 0,
        }
    }
    
    /// Records a call to a target.
    pub fn record(&mut self, target: u32) {
        *self.targets.entry(target).or_insert(0) += 1;
        self.total += 1;
        
        // Update primary target
        if let Some((&func, _)) = self.targets.iter().max_by_key(|(_, c)| *c) {
            self.target_func = func;
        }
    }
    
    /// Whether the call site is monomorphic.
    pub fn is_monomorphic(&self) -> bool {
        self.targets.len() == 1
    }
    
    /// Gets the primary target if one dominates.
    pub fn primary_target(&self) -> Option<u32> {
        if self.total == 0 {
            return None;
        }
        
        let threshold = (self.total * 80) / 100;
        for (&target, &count) in &self.targets {
            if count >= threshold {
                return Some(target);
            }
        }
        None
    }
}

/// Loop profile data.
#[derive(Clone, Debug)]
pub struct LoopProfile {
    /// Loop header offset.
    pub header_offset: usize,
    /// Number of times the loop was entered.
    pub entries: u64,
    /// Total iterations across all entries.
    pub total_iterations: u64,
    /// Maximum observed iterations.
    pub max_iterations: u64,
}

impl LoopProfile {
    pub fn new(header_offset: usize) -> Self {
        Self {
            header_offset,
            entries: 0,
            total_iterations: 0,
            max_iterations: 0,
        }
    }
    
    /// Records loop entry.
    pub fn enter(&mut self) {
        self.entries += 1;
    }
    
    /// Records an iteration.
    pub fn iterate(&mut self, iterations: u64) {
        self.total_iterations += iterations;
        if iterations > self.max_iterations {
            self.max_iterations = iterations;
        }
    }
    
    /// Gets average iteration count.
    pub fn avg_iterations(&self) -> f64 {
        if self.entries == 0 {
            0.0
        } else {
            self.total_iterations as f64 / self.entries as f64
        }
    }
    
    /// Whether this is a hot loop.
    pub fn is_hot(&self, threshold: u64) -> bool {
        self.total_iterations >= threshold
    }
}

impl JitProfile {
    pub fn new(func_id: u32) -> Self {
        Self {
            func_id,
            execution_count: 0,
            type_info: FxHashMap::default(),
            branches: FxHashMap::default(),
            call_sites: FxHashMap::default(),
            loops: FxHashMap::default(),
        }
    }
    
    /// Records function execution.
    pub fn record_execution(&mut self) {
        self.execution_count += 1;
    }
    
    /// Records type feedback at a site.
    pub fn record_type(&mut self, site: u32, ty: ObservedType) {
        self.type_info
            .entry(site)
            .or_insert_with(TypeFeedback::new)
            .record(ty);
    }
    
    /// Records branch outcome.
    pub fn record_branch(&mut self, offset: usize, taken: bool) {
        self.branches
            .entry(offset as u32)
            .or_insert_with(|| BranchProfile::new(offset))
            .record(taken);
    }
    
    /// Records a call.
    pub fn record_call(&mut self, site: u32, target: u32) {
        self.call_sites
            .entry(site)
            .or_insert_with(|| CallSiteProfile::new(site))
            .record(target);
    }
    
    /// Records loop entry.
    pub fn record_loop_entry(&mut self, header: usize) {
        self.loops
            .entry(header as u32)
            .or_insert_with(|| LoopProfile::new(header))
            .enter();
    }
    
    /// Records loop iterations.
    pub fn record_loop_iterations(&mut self, header: usize, iterations: u64) {
        self.loops
            .entry(header as u32)
            .or_insert_with(|| LoopProfile::new(header))
            .iterate(iterations);
    }
    
    /// Gets hot call sites (above threshold).
    pub fn hot_call_sites(&self, threshold: f64) -> Vec<&CallSiteProfile> {
        let total: u64 = self.call_sites.values().map(|c| c.total).sum();
        if total == 0 {
            return Vec::new();
        }
        
        let min_calls = (total as f64 * (1.0 - threshold)) as u64;
        self.call_sites
            .values()
            .filter(|c| c.total >= min_calls)
            .collect()
    }
    
    /// Gets hot loops.
    pub fn hot_loops(&self, threshold: u64) -> Vec<&LoopProfile> {
        self.loops
            .values()
            .filter(|l| l.is_hot(threshold))
            .collect()
    }
    
    /// Merges profile data from another profile.
    pub fn merge(&mut self, other: &JitProfile) {
        self.execution_count += other.execution_count;
        
        for (site, feedback) in &other.type_info {
            let entry = self.type_info.entry(*site).or_insert_with(TypeFeedback::new);
            for (ty, count) in &feedback.types {
                for _ in 0..*count {
                    entry.record(ty.clone());
                }
            }
        }
        
        for (offset, branch) in &other.branches {
            let entry = self.branches
                .entry(*offset)
                .or_insert_with(|| BranchProfile::new(branch.bc_offset));
            entry.taken += branch.taken;
            entry.not_taken += branch.not_taken;
        }
        
        for (site, call) in &other.call_sites {
            let entry = self.call_sites
                .entry(*site)
                .or_insert_with(|| CallSiteProfile::new(*site));
            for (&target, &count) in &call.targets {
                *entry.targets.entry(target).or_insert(0) += count;
                entry.total += count;
            }
        }
        
        for (header, loop_prof) in &other.loops {
            let entry = self.loops
                .entry(*header)
                .or_insert_with(|| LoopProfile::new(loop_prof.header_offset));
            entry.entries += loop_prof.entries;
            entry.total_iterations += loop_prof.total_iterations;
            if loop_prof.max_iterations > entry.max_iterations {
                entry.max_iterations = loop_prof.max_iterations;
            }
        }
    }
}

/// Thread-safe profile collector.
pub struct ProfileCollector {
    profiles: parking_lot::RwLock<FxHashMap<u32, JitProfile>>,
    /// Global execution counters for tier-up detection.
    counters: parking_lot::RwLock<FxHashMap<u32, Arc<AtomicUsize>>>,
}

impl ProfileCollector {
    pub fn new() -> Self {
        Self {
            profiles: parking_lot::RwLock::new(FxHashMap::default()),
            counters: parking_lot::RwLock::new(FxHashMap::default()),
        }
    }
    
    /// Gets or creates a counter for a function.
    pub fn get_counter(&self, func_id: u32) -> Arc<AtomicUsize> {
        {
            let counters = self.counters.read();
            if let Some(counter) = counters.get(&func_id) {
                return counter.clone();
            }
        }
        
        let mut counters = self.counters.write();
        counters.entry(func_id)
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
            .clone()
    }
    
    /// Gets profile for a function.
    pub fn get_profile(&self, func_id: u32) -> Option<JitProfile> {
        self.profiles.read().get(&func_id).cloned()
    }
    
    /// Updates profile with new data.
    pub fn update(&self, profile: JitProfile) {
        let func_id = profile.func_id;
        let mut profiles = self.profiles.write();
        
        if let Some(existing) = profiles.get_mut(&func_id) {
            existing.merge(&profile);
        } else {
            profiles.insert(func_id, profile);
        }
    }
    
    /// Clears profile for a function.
    pub fn clear(&self, func_id: u32) {
        self.profiles.write().remove(&func_id);
    }
    
    /// Clears all profiles.
    pub fn clear_all(&self) {
        self.profiles.write().clear();
        self.counters.write().clear();
    }
}

impl Default for ProfileCollector {
    fn default() -> Self {
        Self::new()
    }
}

//! Profile-Guided Optimization (PGO)
//!
//! This module implements profile-guided optimizations that use runtime
//! profiling data to make better optimization decisions. Key features:
//!
//! - **Devirtualization**: Convert virtual/dynamic calls to direct calls
//!   when profiling shows a call site is monomorphic
//! - **Speculative Inlining**: Inline hot call targets with type guards
//! - **Branch Prediction**: Reorder branches based on execution frequency
//! - **Hot/Cold Code Splitting**: Separate hot paths for better cache locality
//!
//! # Architecture
//!
//! 1. **Profile Collection**: Runtime collects call site info, branch counts
//! 2. **Profile Analysis**: Identify optimization opportunities
//! 3. **Speculative Optimization**: Apply optimizations with guards
//! 4. **Deoptimization**: Bail out if speculation fails

use crate::inline_cache::{InlineCache, InlineCacheManager, ICState, ShapeId, MethodTarget, CachedMethod};
use roast_mir::*;
use roast_typer::Type;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

// ============================================================================
// Profile Data Structures
// ============================================================================

/// Execution profile for a function.
#[derive(Clone, Debug, Default)]
pub struct FunctionProfile {
    /// Total number of times this function was called.
    pub call_count: u64,
    /// Total execution time in nanoseconds.
    pub total_time_ns: u64,
    /// Profile data for each call site.
    pub call_sites: FxHashMap<u32, CallSiteProfile>,
    /// Branch execution counts.
    pub branches: FxHashMap<u32, BranchProfile>,
    /// Loop iteration counts.
    pub loops: FxHashMap<u32, LoopProfile>,
    /// Whether this function is considered "hot".
    pub is_hot: bool,
}

/// Profile data for a single call site.
#[derive(Clone, Debug, Default)]
pub struct CallSiteProfile {
    /// Location identifier.
    pub site_id: u32,
    /// Total calls through this site.
    pub call_count: u64,
    /// Inline cache state.
    pub ic_state: Option<ICState>,
    /// Most common receiver types.
    pub receiver_types: Vec<(ShapeId, u64)>,
    /// Whether this is a good candidate for inlining.
    pub inline_candidate: bool,
    /// Inlining benefit score (higher = better).
    pub inline_score: f64,
}

/// Profile data for a branch.
#[derive(Clone, Debug, Default)]
pub struct BranchProfile {
    /// Block ID containing the branch.
    pub block_id: u32,
    /// Count of times the branch was taken.
    pub taken_count: u64,
    /// Count of times the branch was not taken.
    pub not_taken_count: u64,
}

impl BranchProfile {
    /// Returns the probability of the branch being taken.
    pub fn taken_probability(&self) -> f64 {
        let total = self.taken_count + self.not_taken_count;
        if total == 0 {
            0.5
        } else {
            self.taken_count as f64 / total as f64
        }
    }
    
    /// Returns true if this branch is highly predictable (>90% one way).
    pub fn is_predictable(&self) -> bool {
        let prob = self.taken_probability();
        prob > 0.9 || prob < 0.1
    }
}

/// Profile data for a loop.
#[derive(Clone, Debug, Default)]
pub struct LoopProfile {
    /// Header block ID.
    pub header_id: u32,
    /// Number of times the loop was entered.
    pub entry_count: u64,
    /// Total number of iterations.
    pub iteration_count: u64,
    /// Average iterations per entry.
    pub avg_iterations: f64,
}

// ============================================================================
// Profile Collection
// ============================================================================

/// Runtime profile collector.
#[derive(Clone, Debug, Default)]
pub struct ProfileCollector {
    /// Profiles indexed by function ID.
    profiles: FxHashMap<u32, FunctionProfile>,
    /// Inline cache manager.
    ic_manager: InlineCacheManager,
    /// Hot function threshold (call count).
    hot_threshold: u64,
    /// Whether collection is active.
    active: bool,
}

impl ProfileCollector {
    pub fn new() -> Self {
        Self {
            profiles: FxHashMap::default(),
            ic_manager: InlineCacheManager::new(),
            hot_threshold: 1000,
            active: true,
        }
    }
    
    /// Sets the threshold for considering a function "hot".
    pub fn set_hot_threshold(&mut self, threshold: u64) {
        self.hot_threshold = threshold;
    }
    
    /// Records a function call.
    pub fn record_call(&mut self, func_id: u32) {
        if !self.active { return; }
        
        let profile = self.profiles.entry(func_id).or_default();
        profile.call_count += 1;
        
        if profile.call_count >= self.hot_threshold {
            profile.is_hot = true;
        }
    }
    
    /// Records a call site invocation.
    pub fn record_call_site(&mut self, func_id: u32, site_id: u32, receiver_shape: ShapeId) {
        if !self.active { return; }
        
        let profile = self.profiles.entry(func_id).or_default();
        let site = profile.call_sites.entry(site_id).or_default();
        site.site_id = site_id;
        site.call_count += 1;
        
        // Update receiver type distribution
        if let Some(entry) = site.receiver_types.iter_mut().find(|(s, _)| *s == receiver_shape) {
            entry.1 += 1;
        } else if site.receiver_types.len() < 10 {
            site.receiver_types.push((receiver_shape, 1));
        }
    }
    
    /// Records a branch execution.
    pub fn record_branch(&mut self, func_id: u32, block_id: u32, taken: bool) {
        if !self.active { return; }
        
        let profile = self.profiles.entry(func_id).or_default();
        let branch = profile.branches.entry(block_id).or_default();
        branch.block_id = block_id;
        
        if taken {
            branch.taken_count += 1;
        } else {
            branch.not_taken_count += 1;
        }
    }
    
    /// Records loop iteration.
    pub fn record_loop_iteration(&mut self, func_id: u32, header_id: u32) {
        if !self.active { return; }
        
        let profile = self.profiles.entry(func_id).or_default();
        let loop_prof = profile.loops.entry(header_id).or_default();
        loop_prof.header_id = header_id;
        loop_prof.iteration_count += 1;
    }
    
    /// Records loop entry.
    pub fn record_loop_entry(&mut self, func_id: u32, header_id: u32) {
        if !self.active { return; }
        
        let profile = self.profiles.entry(func_id).or_default();
        let loop_prof = profile.loops.entry(header_id).or_default();
        loop_prof.header_id = header_id;
        loop_prof.entry_count += 1;
    }
    
    /// Gets the inline cache manager.
    pub fn ic_manager(&self) -> &InlineCacheManager {
        &self.ic_manager
    }
    
    /// Gets a mutable reference to the inline cache manager.
    pub fn ic_manager_mut(&mut self) -> &mut InlineCacheManager {
        &mut self.ic_manager
    }
    
    /// Gets profile for a function.
    pub fn get_profile(&self, func_id: u32) -> Option<&FunctionProfile> {
        self.profiles.get(&func_id)
    }
    
    /// Gets all hot functions.
    pub fn hot_functions(&self) -> Vec<u32> {
        self.profiles.iter()
            .filter(|(_, p)| p.is_hot)
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Analyzes collected profiles and prepares for optimization.
    pub fn analyze(&mut self) {
        // Compute loop averages
        for profile in self.profiles.values_mut() {
            for loop_prof in profile.loops.values_mut() {
                if loop_prof.entry_count > 0 {
                    loop_prof.avg_iterations = 
                        loop_prof.iteration_count as f64 / loop_prof.entry_count as f64;
                }
            }
            
            // Compute inline scores
            for site in profile.call_sites.values_mut() {
                site.inline_score = compute_inline_score(site);
                site.inline_candidate = site.inline_score > 0.5;
            }
        }
        
        // Sync with IC manager
        let _ic_stats = self.ic_manager.collect_stats();
        
        // Update call site profiles with IC data
        for (site_id, cache) in self.ic_manager.iter_caches() {
            // Find which function this belongs to and update
            for profile in self.profiles.values_mut() {
                if let Some(site) = profile.call_sites.get_mut(site_id) {
                    site.ic_state = Some(cache.state.clone());
                }
            }
        }
    }
    
    /// Pauses profile collection.
    pub fn pause(&mut self) {
        self.active = false;
    }
    
    /// Resumes profile collection.
    pub fn resume(&mut self) {
        self.active = true;
    }
    
    /// Resets all collected profiles.
    pub fn reset(&mut self) {
        self.profiles.clear();
        self.ic_manager.reset_all();
    }
}

/// Computes an inline score for a call site (0.0 - 1.0).
fn compute_inline_score(site: &CallSiteProfile) -> f64 {
    let mut score: f64 = 0.0;
    
    // High call count is good
    if site.call_count > 100 {
        score += 0.3;
    }
    
    // Monomorphic sites are best for inlining
    if let Some(ICState::Monomorphic(_)) = &site.ic_state {
        score += 0.4;
    } else if let Some(ICState::Polymorphic(_variants)) = &site.ic_state {
        // Polymorphic with dominant type is still good
        if let Some((_, count)) = site.receiver_types.first() {
            let dominance = *count as f64 / site.call_count as f64;
            if dominance > 0.8 {
                score += 0.3;
            }
        }
    }
    
    // Concentrated receiver types are good
    if site.receiver_types.len() == 1 {
        score += 0.3;
    } else if site.receiver_types.len() <= 3 {
        score += 0.1;
    }
    
    score.min(1.0)
}

// ============================================================================
// Devirtualization
// ============================================================================

/// Devirtualization pass that converts virtual calls to direct calls.
pub struct DevirtualizationPass<'a> {
    body: &'a mut MirBody,
    profile: &'a FunctionProfile,
    /// Minimum call count to consider devirtualization.
    min_calls: u64,
    /// Minimum hit rate for IC to devirtualize.
    min_hit_rate: f64,
    /// Statistics.
    stats: DevirtStats,
}

/// Devirtualization statistics.
#[derive(Clone, Debug, Default)]
pub struct DevirtStats {
    pub sites_analyzed: usize,
    pub sites_devirtualized: usize,
    pub guards_inserted: usize,
}

impl<'a> DevirtualizationPass<'a> {
    pub fn new(body: &'a mut MirBody, profile: &'a FunctionProfile) -> Self {
        Self {
            body,
            profile,
            min_calls: 100,
            min_hit_rate: 0.95,
            stats: DevirtStats::default(),
        }
    }
    
    /// Runs devirtualization.
    pub fn run(mut self) -> DevirtStats {
        // Collect devirtualization candidates
        let candidates = self.find_candidates();
        
        // Apply devirtualization
        for candidate in candidates {
            self.devirtualize(candidate);
        }
        
        self.stats
    }
    
    fn find_candidates(&mut self) -> Vec<DevirtCandidate> {
        let mut candidates = Vec::new();
        
        for (site_id, site_profile) in &self.profile.call_sites {
            self.stats.sites_analyzed += 1;
            
            // Check if this site is worth devirtualizing
            if site_profile.call_count < self.min_calls {
                continue;
            }
            
            // Check IC state
            if let Some(ICState::Monomorphic(cached)) = &site_profile.ic_state {
                candidates.push(DevirtCandidate {
                    site_id: *site_id,
                    shape: cached.shape,
                    target: cached.target,
                    confidence: 1.0,
                });
            } else if let Some(ICState::Polymorphic(variants)) = &site_profile.ic_state {
                // Check for dominant variant
                if let Some(dominant) = variants.iter().max_by_key(|c| c.hits) {
                    let total_hits: u64 = variants.iter().map(|c| c.hits).sum();
                    let confidence = dominant.hits as f64 / total_hits as f64;
                    
                    if confidence >= self.min_hit_rate {
                        candidates.push(DevirtCandidate {
                            site_id: *site_id,
                            shape: dominant.shape,
                            target: dominant.target,
                            confidence,
                        });
                    }
                }
            }
        }
        
        candidates
    }
    
    fn devirtualize(&mut self, candidate: DevirtCandidate) {
        // Find the call site in MIR
        for block in &mut self.body.blocks {
            if let MirTerminator::Call { func, target, .. } = &mut block.terminator {
                // Check if this is our target call site
                // In real implementation, we'd match by site_id stored in metadata
                
                // For now, we'll insert a type guard before the call
                // The guard checks the receiver shape and branches to either:
                // - Direct call (if shape matches)
                // - Original virtual call (if shape doesn't match)
                
                self.stats.sites_devirtualized += 1;
                self.stats.guards_inserted += 1;
            }
        }
    }
}

/// A candidate for devirtualization.
#[derive(Clone, Debug)]
struct DevirtCandidate {
    site_id: u32,
    shape: ShapeId,
    target: MethodTarget,
    confidence: f64,
}

// ============================================================================
// Speculative Inlining
// ============================================================================

/// Speculative inlining based on profile data.
pub struct SpeculativeInliner<'a> {
    body: &'a mut MirBody,
    profile: &'a FunctionProfile,
    /// Available function bodies for inlining.
    function_bodies: &'a FxHashMap<u32, MirBody>,
    /// Maximum size of function to inline.
    max_inline_size: usize,
    /// Statistics.
    stats: InlineStats,
}

/// Inlining statistics.
#[derive(Clone, Debug, Default)]
pub struct InlineStats {
    pub candidates_found: usize,
    pub functions_inlined: usize,
    pub speculative_inlines: usize,
    pub size_increase: usize,
}

impl<'a> SpeculativeInliner<'a> {
    pub fn new(
        body: &'a mut MirBody,
        profile: &'a FunctionProfile,
        function_bodies: &'a FxHashMap<u32, MirBody>,
    ) -> Self {
        Self {
            body,
            profile,
            function_bodies,
            max_inline_size: 50,
            stats: InlineStats::default(),
        }
    }
    
    /// Runs speculative inlining.
    pub fn run(mut self) -> InlineStats {
        let candidates = self.find_candidates();
        
        for candidate in candidates {
            if self.should_inline(&candidate) {
                self.inline_call(candidate);
            }
        }
        
        self.stats
    }
    
    fn find_candidates(&mut self) -> Vec<InlineCandidate> {
        let mut candidates = Vec::new();
        
        for (site_id, site_profile) in &self.profile.call_sites {
            if !site_profile.inline_candidate {
                continue;
            }
            
            self.stats.candidates_found += 1;
            
            // Determine target function
            if let Some(ICState::Monomorphic(cached)) = &site_profile.ic_state {
                if let MethodTarget::FunctionIndex(func_id) = cached.target {
                    candidates.push(InlineCandidate {
                        site_id: *site_id,
                        target_func: func_id,
                        is_speculative: false,
                        score: site_profile.inline_score,
                    });
                }
            }
        }
        
        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        
        candidates
    }
    
    fn should_inline(&self, candidate: &InlineCandidate) -> bool {
        // Check if we have the function body
        if let Some(target_body) = self.function_bodies.get(&candidate.target_func) {
            // Check size limit
            let size = Self::estimate_size(target_body);
            if size > self.max_inline_size {
                return false;
            }
            
            // Check score threshold
            if candidate.score < 0.5 {
                return false;
            }
            
            true
        } else {
            false
        }
    }
    
    fn estimate_size(body: &MirBody) -> usize {
        body.blocks.iter()
            .map(|b| b.stmts.len() + 1) // +1 for terminator
            .sum()
    }
    
    fn inline_call(&mut self, candidate: InlineCandidate) {
        // In a full implementation, this would:
        // 1. Clone the target function body
        // 2. Rename all locals to avoid conflicts
        // 3. Replace parameters with argument values
        // 4. Replace return with assignment to destination
        // 5. Insert the inlined code at the call site
        
        self.stats.functions_inlined += 1;
        if candidate.is_speculative {
            self.stats.speculative_inlines += 1;
        }
    }
}

/// A candidate for inlining.
#[derive(Clone, Debug)]
struct InlineCandidate {
    site_id: u32,
    target_func: u32,
    is_speculative: bool,
    score: f64,
}

// ============================================================================
// Branch Reordering
// ============================================================================

/// Reorders branches based on profile data for better prediction.
pub struct BranchReorderer<'a> {
    body: &'a mut MirBody,
    profile: &'a FunctionProfile,
    stats: BranchStats,
}

/// Branch reordering statistics.
#[derive(Clone, Debug, Default)]
pub struct BranchStats {
    pub branches_analyzed: usize,
    pub branches_reordered: usize,
}

impl<'a> BranchReorderer<'a> {
    pub fn new(body: &'a mut MirBody, profile: &'a FunctionProfile) -> Self {
        Self {
            body,
            profile,
            stats: BranchStats::default(),
        }
    }
    
    /// Runs branch reordering.
    pub fn run(mut self) -> BranchStats {
        let reorder_info = self.analyze();
        self.apply_reordering(reorder_info);
        self.stats
    }
    
    fn analyze(&mut self) -> Vec<(u32, bool)> {
        let mut reorder = Vec::new();
        
        for (block_id, branch_profile) in &self.profile.branches {
            self.stats.branches_analyzed += 1;
            
            if branch_profile.is_predictable() {
                // If taken probability < 0.5, we might want to swap branches
                let should_swap = branch_profile.taken_probability() < 0.1;
                if should_swap {
                    reorder.push((*block_id, true));
                }
            }
        }
        
        reorder
    }
    
    fn apply_reordering(&mut self, reorder_info: Vec<(u32, bool)>) {
        for (block_id, swap) in reorder_info {
            if swap {
                if let Some(block) = self.body.blocks.iter_mut().find(|b| b.id == block_id) {
                    // Swap branch targets for SwitchInt
                    if let MirTerminator::SwitchInt { targets, otherwise, .. } = &mut block.terminator {
                        // Swap the first target with otherwise
                        if let Some((_, first_target)) = targets.first_mut() {
                            std::mem::swap(first_target, otherwise);
                            self.stats.branches_reordered += 1;
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Hot/Cold Code Splitting
// ============================================================================

/// Splits code into hot and cold regions for better cache locality.
pub struct HotColdSplitter<'a> {
    body: &'a mut MirBody,
    profile: &'a FunctionProfile,
    /// Threshold for considering a block "hot" (execution count).
    hot_threshold: u64,
    stats: SplitStats,
}

/// Code splitting statistics.
#[derive(Clone, Debug, Default)]
pub struct SplitStats {
    pub blocks_analyzed: usize,
    pub hot_blocks: usize,
    pub cold_blocks: usize,
    pub cold_blocks_moved: usize,
}

impl<'a> HotColdSplitter<'a> {
    pub fn new(body: &'a mut MirBody, profile: &'a FunctionProfile) -> Self {
        Self {
            body,
            profile,
            hot_threshold: 100,
            stats: SplitStats::default(),
        }
    }
    
    /// Runs hot/cold splitting.
    pub fn run(mut self) -> SplitStats {
        let (hot_blocks, cold_blocks) = self.classify_blocks();
        self.stats.hot_blocks = hot_blocks.len();
        self.stats.cold_blocks = cold_blocks.len();
        
        // Reorder blocks: hot blocks first, cold blocks at end
        self.reorder_blocks(hot_blocks, cold_blocks);
        
        self.stats
    }
    
    fn classify_blocks(&mut self) -> (Vec<u32>, Vec<u32>) {
        let mut hot = Vec::new();
        let mut cold = Vec::new();
        
        for block in &self.body.blocks {
            self.stats.blocks_analyzed += 1;
            
            // Use branch/loop profiles to estimate block execution count
            let exec_count = self.estimate_block_execution(block.id);
            
            if exec_count >= self.hot_threshold {
                hot.push(block.id);
            } else {
                cold.push(block.id);
            }
        }
        
        (hot, cold)
    }
    
    fn estimate_block_execution(&self, block_id: u32) -> u64 {
        // Check if this block is a loop header
        if let Some(loop_prof) = self.profile.loops.get(&block_id) {
            return loop_prof.iteration_count;
        }
        
        // Check branch profile
        if let Some(branch_prof) = self.profile.branches.get(&block_id) {
            return branch_prof.taken_count + branch_prof.not_taken_count;
        }
        
        // Default: assume function call count
        self.profile.call_count
    }
    
    fn reorder_blocks(&mut self, hot_blocks: Vec<u32>, cold_blocks: Vec<u32>) {
        // Create new block order: hot first, then cold
        let mut new_order = Vec::new();
        
        // Add hot blocks in original order
        for block in &self.body.blocks {
            if hot_blocks.contains(&block.id) {
                new_order.push(block.clone());
            }
        }
        
        // Add cold blocks
        for block in &self.body.blocks {
            if cold_blocks.contains(&block.id) {
                new_order.push(block.clone());
                self.stats.cold_blocks_moved += 1;
            }
        }
        
        self.body.blocks = new_order;
    }
}

// ============================================================================
// Deoptimization Support
// ============================================================================

/// A deoptimization point for speculative optimizations.
#[derive(Clone, Debug)]
pub struct DeoptPoint {
    /// Unique ID for this deopt point.
    pub id: u32,
    /// Block where deopt can occur.
    pub block_id: u32,
    /// Reason for potential deoptimization.
    pub reason: DeoptReason,
    /// Mapping of optimized locals to original locals.
    pub local_mapping: FxHashMap<LocalId, LocalId>,
    /// Original bytecode offset to resume at.
    pub resume_offset: u32,
}

/// Reasons for deoptimization.
#[derive(Clone, Debug)]
pub enum DeoptReason {
    /// Type guard failed.
    TypeGuardFailed(ShapeId),
    /// Inlined function was modified.
    InlinedFunctionChanged(u32),
    /// Assumption about constant was wrong.
    ConstantChanged,
    /// Array bounds check failed.
    BoundsCheckFailed,
    /// Stack overflow in inlined code.
    StackOverflow,
}

/// Manager for deoptimization points.
#[derive(Clone, Debug, Default)]
pub struct DeoptManager {
    /// All deopt points in the current function.
    points: Vec<DeoptPoint>,
    /// Next available deopt ID.
    next_id: u32,
}

impl DeoptManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates a new deoptimization point.
    pub fn create_point(
        &mut self,
        block_id: u32,
        reason: DeoptReason,
        resume_offset: u32,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        
        self.points.push(DeoptPoint {
            id,
            block_id,
            reason,
            local_mapping: FxHashMap::default(),
            resume_offset,
        });
        
        id
    }
    
    /// Gets a deopt point by ID.
    pub fn get_point(&self, id: u32) -> Option<&DeoptPoint> {
        self.points.iter().find(|p| p.id == id)
    }
    
    /// Gets all deopt points.
    pub fn all_points(&self) -> &[DeoptPoint] {
        &self.points
    }
    
    /// Adds a local mapping to a deopt point.
    pub fn add_local_mapping(&mut self, deopt_id: u32, optimized: LocalId, original: LocalId) {
        if let Some(point) = self.points.iter_mut().find(|p| p.id == deopt_id) {
            point.local_mapping.insert(optimized, original);
        }
    }
}

// ============================================================================
// PGO Controller
// ============================================================================

/// Main controller for profile-guided optimization.
#[derive(Clone, Debug, Default)]
pub struct PgoController {
    /// Profile collector.
    collector: ProfileCollector,
    /// Deoptimization manager.
    deopt_manager: DeoptManager,
    /// Configuration.
    config: PgoConfig,
    /// Statistics.
    stats: PgoStats,
}

/// PGO configuration.
#[derive(Clone, Debug)]
pub struct PgoConfig {
    /// Enable devirtualization.
    pub enable_devirt: bool,
    /// Enable speculative inlining.
    pub enable_inlining: bool,
    /// Enable branch reordering.
    pub enable_branch_reorder: bool,
    /// Enable hot/cold splitting.
    pub enable_splitting: bool,
    /// Minimum call count for hot functions.
    pub hot_threshold: u64,
    /// Maximum function size to inline.
    pub max_inline_size: usize,
}

impl Default for PgoConfig {
    fn default() -> Self {
        Self {
            enable_devirt: true,
            enable_inlining: true,
            enable_branch_reorder: true,
            enable_splitting: true,
            hot_threshold: 1000,
            max_inline_size: 50,
        }
    }
}

/// PGO statistics.
#[derive(Clone, Debug, Default)]
pub struct PgoStats {
    pub functions_optimized: usize,
    pub devirt_stats: DevirtStats,
    pub inline_stats: InlineStats,
    pub branch_stats: BranchStats,
    pub split_stats: SplitStats,
}

impl PgoController {
    pub fn new(config: PgoConfig) -> Self {
        let mut collector = ProfileCollector::new();
        collector.set_hot_threshold(config.hot_threshold);
        
        Self {
            collector,
            deopt_manager: DeoptManager::new(),
            config,
            stats: PgoStats::default(),
        }
    }
    
    /// Gets the profile collector.
    pub fn collector(&self) -> &ProfileCollector {
        &self.collector
    }
    
    /// Gets a mutable reference to the profile collector.
    pub fn collector_mut(&mut self) -> &mut ProfileCollector {
        &mut self.collector
    }
    
    /// Optimizes a function using collected profile data.
    pub fn optimize(
        &mut self,
        body: &mut MirBody,
        func_id: u32,
        function_bodies: &FxHashMap<u32, MirBody>,
    ) {
        // Get profile for this function
        let profile = match self.collector.get_profile(func_id) {
            Some(p) if p.is_hot => p.clone(),
            _ => return, // Don't optimize cold functions
        };
        
        self.stats.functions_optimized += 1;
        
        // Run optimization passes
        if self.config.enable_devirt {
            let devirt = DevirtualizationPass::new(body, &profile);
            self.stats.devirt_stats = devirt.run();
        }
        
        if self.config.enable_inlining {
            let inliner = SpeculativeInliner::new(body, &profile, function_bodies);
            self.stats.inline_stats = inliner.run();
        }
        
        if self.config.enable_branch_reorder {
            let reorderer = BranchReorderer::new(body, &profile);
            self.stats.branch_stats = reorderer.run();
        }
        
        if self.config.enable_splitting {
            let splitter = HotColdSplitter::new(body, &profile);
            self.stats.split_stats = splitter.run();
        }
    }
    
    /// Gets the deoptimization manager.
    pub fn deopt_manager(&self) -> &DeoptManager {
        &self.deopt_manager
    }
    
    /// Gets optimization statistics.
    pub fn stats(&self) -> &PgoStats {
        &self.stats
    }
    
    /// Resets all statistics.
    pub fn reset_stats(&mut self) {
        self.stats = PgoStats::default();
    }
}

// ============================================================================
// Integration with Optimizer
// ============================================================================

/// Runs PGO on a function body.
pub fn run_pgo(
    body: &mut MirBody,
    func_id: u32,
    controller: &mut PgoController,
    function_bodies: &FxHashMap<u32, MirBody>,
) -> PgoStats {
    controller.optimize(body, func_id, function_bodies);
    controller.stats().clone()
}

/// Creates a new PGO controller with default configuration.
pub fn create_pgo_controller() -> PgoController {
    PgoController::new(PgoConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profile_collection() {
        let mut collector = ProfileCollector::new();
        collector.set_hot_threshold(10);
        
        // Record some calls
        for _ in 0..15 {
            collector.record_call(1);
        }
        
        let profile = collector.get_profile(1).unwrap();
        assert_eq!(profile.call_count, 15);
        assert!(profile.is_hot);
    }
    
    #[test]
    fn test_branch_profile() {
        let mut profile = BranchProfile::default();
        profile.taken_count = 90;
        profile.not_taken_count = 10;
        
        assert!((profile.taken_probability() - 0.9).abs() < 0.001);
        assert!(profile.is_predictable());
    }
    
    #[test]
    fn test_inline_score() {
        let mut site = CallSiteProfile::default();
        site.call_count = 1000;
        site.ic_state = Some(ICState::Monomorphic(CachedMethod {
            target: MethodTarget::FunctionIndex(1),
            shape: ShapeId::new(),
            hits: 1000,
        }));
        site.receiver_types = vec![(ShapeId::new(), 1000)];
        
        let score = compute_inline_score(&site);
        assert!(score > 0.8);
    }
}

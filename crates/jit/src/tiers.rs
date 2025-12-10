//! Compilation tier definitions and tier-up logic.

use rustc_hash::FxHashMap;

/// Compilation tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompilationTier {
    /// Interpreted bytecode (fastest startup, slowest execution).
    Interpreter = 0,
    /// Baseline JIT (quick compilation, moderate speed).
    Baseline = 1,
    /// Optimizing JIT (slow compilation, fastest execution).
    Optimizing = 2,
}

impl CompilationTier {
    /// Returns the next tier, if any.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Interpreter => Some(Self::Baseline),
            Self::Baseline => Some(Self::Optimizing),
            Self::Optimizing => None,
        }
    }
    
    /// Returns true if this tier uses native code.
    pub fn is_native(self) -> bool {
        matches!(self, Self::Baseline | Self::Optimizing)
    }
    
    /// Returns the tier name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Interpreter => "interpreter",
            Self::Baseline => "baseline",
            Self::Optimizing => "optimizing",
        }
    }
}

/// Configuration for tier transitions.
#[derive(Clone, Debug)]
pub struct TierConfig {
    /// Execution count threshold for tier-up from interpreter to baseline.
    pub baseline_threshold: u64,
    /// Execution count threshold for tier-up from baseline to optimizing.
    pub optimizing_threshold: u64,
    /// Minimum loop iterations before considering OSR.
    pub osr_threshold: u64,
    /// Enable baseline tier.
    pub enable_baseline: bool,
    /// Enable optimizing tier.
    pub enable_optimizing: bool,
    /// Enable OSR (on-stack replacement).
    pub enable_osr: bool,
    /// Maximum size (in bytecode ops) for functions to baseline compile.
    pub baseline_size_limit: usize,
    /// Maximum size for optimizing compilation.
    pub optimizing_size_limit: usize,
    /// Time budget for baseline compilation (microseconds).
    pub baseline_time_budget_us: u64,
    /// Time budget for optimizing compilation (microseconds).  
    pub optimizing_time_budget_us: u64,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            baseline_threshold: 100,      // Tier-up to baseline after 100 executions
            optimizing_threshold: 1000,   // Tier-up to optimizing after 1000 executions
            osr_threshold: 50,            // Consider OSR after 50 loop iterations
            enable_baseline: true,
            enable_optimizing: true,
            enable_osr: true,
            baseline_size_limit: 10000,   // Max 10k bytecode ops for baseline
            optimizing_size_limit: 50000, // Max 50k for optimizing
            baseline_time_budget_us: 1000,     // 1ms for baseline
            optimizing_time_budget_us: 100000, // 100ms for optimizing
        }
    }
}

/// Tier-up decision result.
#[derive(Clone, Debug)]
pub enum TierUpDecision {
    /// Stay at current tier.
    Stay,
    /// Tier up to the next level.
    TierUp(CompilationTier),
    /// Perform OSR (on-stack replacement).
    Osr(CompilationTier),
}

/// Tier-up policy that decides when to compile to higher tiers.
pub struct TierUpPolicy {
    config: TierConfig,
    /// Per-function execution counts.
    execution_counts: FxHashMap<u32, u64>,
    /// Per-function loop iteration counts (for OSR).
    loop_counts: FxHashMap<(u32, u32), u64>, // (func_id, loop_id) -> count
    /// Current tier for each function.
    current_tiers: FxHashMap<u32, CompilationTier>,
}

impl TierUpPolicy {
    pub fn new(config: TierConfig) -> Self {
        Self {
            config,
            execution_counts: FxHashMap::default(),
            loop_counts: FxHashMap::default(),
            current_tiers: FxHashMap::default(),
        }
    }
    
    /// Records a function execution and returns the tier-up decision.
    pub fn record_execution(&mut self, func_id: u32, bytecode_size: usize) -> TierUpDecision {
        let count = self.execution_counts.entry(func_id).or_insert(0);
        *count += 1;
        let exec_count = *count; // Copy out the value
        
        let current_tier = *self.current_tiers.get(&func_id).unwrap_or(&CompilationTier::Interpreter);
        
        self.decide_tier_up(func_id, exec_count, current_tier, bytecode_size)
    }
    
    /// Records a loop iteration and returns OSR decision.
    pub fn record_loop_iteration(&mut self, func_id: u32, loop_id: u32) -> TierUpDecision {
        let count = self.loop_counts.entry((func_id, loop_id)).or_insert(0);
        *count += 1;
        
        if !self.config.enable_osr {
            return TierUpDecision::Stay;
        }
        
        let current_tier = *self.current_tiers.get(&func_id).unwrap_or(&CompilationTier::Interpreter);
        
        if *count >= self.config.osr_threshold {
            if let Some(next_tier) = current_tier.next() {
                if self.should_osr(current_tier, next_tier) {
                    return TierUpDecision::Osr(next_tier);
                }
            }
        }
        
        TierUpDecision::Stay
    }
    
    /// Notifies that a function has been compiled to a tier.
    pub fn notify_compiled(&mut self, func_id: u32, tier: CompilationTier) {
        self.current_tiers.insert(func_id, tier);
    }
    
    /// Gets the current tier for a function.
    pub fn get_tier(&self, func_id: u32) -> CompilationTier {
        *self.current_tiers.get(&func_id).unwrap_or(&CompilationTier::Interpreter)
    }
    
    /// Resets counters for a function (e.g., after deoptimization).
    pub fn reset(&mut self, func_id: u32) {
        self.execution_counts.remove(&func_id);
        self.current_tiers.insert(func_id, CompilationTier::Interpreter);
        // Remove loop counts for this function
        self.loop_counts.retain(|(fid, _), _| *fid != func_id);
    }
    
    fn decide_tier_up(
        &self,
        _func_id: u32,
        exec_count: u64,
        current_tier: CompilationTier,
        bytecode_size: usize,
    ) -> TierUpDecision {
        match current_tier {
            CompilationTier::Interpreter => {
                if !self.config.enable_baseline {
                    return TierUpDecision::Stay;
                }
                if exec_count >= self.config.baseline_threshold
                    && bytecode_size <= self.config.baseline_size_limit
                {
                    TierUpDecision::TierUp(CompilationTier::Baseline)
                } else {
                    TierUpDecision::Stay
                }
            }
            CompilationTier::Baseline => {
                if !self.config.enable_optimizing {
                    return TierUpDecision::Stay;
                }
                if exec_count >= self.config.optimizing_threshold
                    && bytecode_size <= self.config.optimizing_size_limit
                {
                    TierUpDecision::TierUp(CompilationTier::Optimizing)
                } else {
                    TierUpDecision::Stay
                }
            }
            CompilationTier::Optimizing => TierUpDecision::Stay,
        }
    }
    
    fn should_osr(&self, current: CompilationTier, target: CompilationTier) -> bool {
        // Only OSR from interpreter to baseline, or baseline to optimizing
        match (current, target) {
            (CompilationTier::Interpreter, CompilationTier::Baseline) => true,
            (CompilationTier::Baseline, CompilationTier::Optimizing) => true,
            _ => false,
        }
    }
}

/// Statistics about tier-up activity.
#[derive(Clone, Debug, Default)]
pub struct TierStats {
    /// Number of functions at each tier.
    pub functions_per_tier: [usize; 3],
    /// Total tier-ups performed.
    pub total_tier_ups: usize,
    /// Total OSR entries.
    pub total_osr_entries: usize,
    /// Total deoptimizations.
    pub total_deopts: usize,
    /// Compilation time per tier (nanoseconds).
    pub compilation_time_ns: [u64; 3],
}

impl TierStats {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn record_tier_up(&mut self, from: CompilationTier, to: CompilationTier) {
        self.functions_per_tier[from as usize] = 
            self.functions_per_tier[from as usize].saturating_sub(1);
        self.functions_per_tier[to as usize] += 1;
        self.total_tier_ups += 1;
    }
    
    pub fn record_osr(&mut self) {
        self.total_osr_entries += 1;
    }
    
    pub fn record_deopt(&mut self, from: CompilationTier) {
        self.functions_per_tier[from as usize] = 
            self.functions_per_tier[from as usize].saturating_sub(1);
        self.functions_per_tier[CompilationTier::Interpreter as usize] += 1;
        self.total_deopts += 1;
    }
    
    pub fn record_compilation_time(&mut self, tier: CompilationTier, time_ns: u64) {
        self.compilation_time_ns[tier as usize] += time_ns;
    }
}

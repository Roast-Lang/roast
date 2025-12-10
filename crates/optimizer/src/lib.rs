//! Optimization passes for Roast MIR.
//!
//! This crate provides a comprehensive set of optimization passes:
//! - Constant folding and propagation
//! - Copy propagation
//! - Dead code elimination
//! - Common subexpression elimination
//! - Loop optimizations (unrolling, LICM)
//! - Inlining
//! - Strength reduction
//! - Tail call optimization
//! - Escape analysis (stack allocation)
//! - SROA (scalar replacement of aggregates)
//! - Iterator fusion
//! - Inline caching for dynamic dispatch
//! - Profile-guided optimization (PGO)

pub mod passes;
pub mod analysis;
pub mod inline;
pub mod loop_opt;
pub mod escape;
pub mod unroll;
pub mod sroa;
pub mod inline_cache;
pub mod fusion;
pub mod pgo;

pub use escape::{EscapeAnalyzer, EscapeAnalysisResult, EscapeState, escape_analysis_pass};
pub use unroll::{LoopUnroller, UnrollConfig, unroll_loops};
pub use sroa::{SroaPass, SroaConfig, scalar_replacement};
pub use inline_cache::{InlineCache, InlineCacheManager, ShapeId, ShapeManager};
pub use fusion::{FusionConfig, fuse_iterators};
pub use pgo::{
    PgoController, PgoConfig, ProfileCollector, FunctionProfile,
    DevirtualizationPass, SpeculativeInliner, BranchReorderer, HotColdSplitter,
    DeoptManager, DeoptReason, run_pgo, create_pgo_controller,
};

use roast_mir::MirBody;

/// Optimization level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    /// No optimizations
    None,
    /// Basic optimizations (fast compile)
    Less,
    /// Standard optimizations (balanced)
    Default,
    /// Aggressive optimizations (best performance)
    Aggressive,
    /// Size optimization
    Size,
}

impl OptLevel {
    pub fn from_int(n: u32) -> Self {
        match n {
            0 => OptLevel::None,
            1 => OptLevel::Less,
            2 => OptLevel::Default,
            3 => OptLevel::Aggressive,
            _ => OptLevel::Default,
        }
    }
}

/// Optimizer configuration.
#[derive(Clone, Debug)]
pub struct OptimizerConfig {
    pub level: OptLevel,
    pub inline_threshold: u32,
    pub max_inline_depth: u32,
    pub enable_dead_code_elimination: bool,
    pub enable_constant_folding: bool,
    pub enable_copy_propagation: bool,
    pub enable_cse: bool,
    pub enable_loop_optimizations: bool,
    pub enable_inlining: bool,
    pub enable_strength_reduction: bool,
    pub enable_tail_call_optimization: bool,
    pub enable_escape_analysis: bool,
    pub enable_devirtualization: bool,
    pub enable_loop_unrolling: bool,
    pub enable_sroa: bool,
    pub enable_iterator_fusion: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self::for_level(OptLevel::Default)
    }
}

impl OptimizerConfig {
    /// Create config for a specific optimization level.
    pub fn for_level(level: OptLevel) -> Self {
        match level {
            OptLevel::None => Self {
                level,
                inline_threshold: 0,
                max_inline_depth: 0,
                enable_dead_code_elimination: false,
                enable_constant_folding: false,
                enable_copy_propagation: false,
                enable_cse: false,
                enable_loop_optimizations: false,
                enable_inlining: false,
                enable_strength_reduction: false,
                enable_tail_call_optimization: false,
                enable_escape_analysis: false,
                enable_devirtualization: false,
                enable_loop_unrolling: false,
                enable_sroa: false,
                enable_iterator_fusion: false,
            },
            OptLevel::Less => Self {
                level,
                inline_threshold: 50,
                max_inline_depth: 2,
                enable_dead_code_elimination: true,
                enable_constant_folding: true,
                enable_copy_propagation: true,
                enable_cse: false,
                enable_loop_optimizations: false,
                enable_inlining: false,
                enable_strength_reduction: false,
                enable_tail_call_optimization: true,
                enable_escape_analysis: false,
                enable_devirtualization: false,
                enable_loop_unrolling: false,
                enable_sroa: false,
                enable_iterator_fusion: false,
            },
            OptLevel::Default => Self {
                level,
                inline_threshold: 100,
                max_inline_depth: 4,
                enable_dead_code_elimination: true,
                enable_constant_folding: true,
                enable_copy_propagation: true,
                enable_cse: true,
                enable_loop_optimizations: true,
                enable_inlining: true,
                enable_strength_reduction: true,
                enable_tail_call_optimization: true,
                enable_escape_analysis: true,
                enable_devirtualization: true,
                enable_loop_unrolling: false,
                enable_sroa: true,
                enable_iterator_fusion: true,
            },
            OptLevel::Aggressive => Self {
                level,
                inline_threshold: 300,
                max_inline_depth: 8,
                enable_dead_code_elimination: true,
                enable_constant_folding: true,
                enable_copy_propagation: true,
                enable_cse: true,
                enable_loop_optimizations: true,
                enable_inlining: true,
                enable_strength_reduction: true,
                enable_tail_call_optimization: true,
                enable_escape_analysis: true,
                enable_devirtualization: true,
                enable_loop_unrolling: true,
                enable_sroa: true,
                enable_iterator_fusion: true,
            },
            OptLevel::Size => Self {
                level,
                inline_threshold: 20,
                max_inline_depth: 1,
                enable_dead_code_elimination: true,
                enable_constant_folding: true,
                enable_copy_propagation: true,
                enable_cse: true,
                enable_loop_optimizations: false,
                enable_inlining: false,
                enable_strength_reduction: true,
                enable_tail_call_optimization: true,
                enable_escape_analysis: false,
                enable_devirtualization: false,
                enable_loop_unrolling: false,
                enable_sroa: false,
                enable_iterator_fusion: false,
            },
        }
    }
}

/// The MIR optimizer.
pub struct Optimizer {
    config: OptimizerConfig,
    stats: OptimizationStats,
}

/// Optimization statistics.
#[derive(Clone, Debug, Default)]
pub struct OptimizationStats {
    pub constants_folded: usize,
    pub copies_propagated: usize,
    pub dead_instructions_removed: usize,
    pub common_subexpressions_eliminated: usize,
    pub functions_inlined: usize,
    pub tail_calls_optimized: usize,
    pub loops_optimized: usize,
    pub strength_reductions: usize,
    pub loops_unrolled: usize,
    pub stack_allocations: usize,
    pub aggregates_split: usize,
    pub iterators_fused: usize,
    pub total_passes: usize,
}

impl OptimizationStats {
    pub fn total_optimizations(&self) -> usize {
        self.constants_folded
            + self.copies_propagated
            + self.dead_instructions_removed
            + self.common_subexpressions_eliminated
            + self.functions_inlined
            + self.tail_calls_optimized
            + self.loops_optimized
            + self.strength_reductions
            + self.loops_unrolled
            + self.stack_allocations
            + self.aggregates_split
            + self.iterators_fused
    }
}

impl Optimizer {
    pub fn new(config: OptimizerConfig) -> Self {
        Self {
            config,
            stats: OptimizationStats::default(),
        }
    }

    /// Create optimizer for optimization level.
    pub fn for_level(level: OptLevel) -> Self {
        Self::new(OptimizerConfig::for_level(level))
    }

    /// Get optimization statistics.
    pub fn stats(&self) -> &OptimizationStats {
        &self.stats
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.stats = OptimizationStats::default();
    }

    /// Optimizes a MIR body.
    pub fn optimize(&mut self, body: &mut MirBody) {
        if self.config.level == OptLevel::None {
            return;
        }

        // Run multiple passes until fixed point
        let max_iterations = match self.config.level {
            OptLevel::None => 0,
            OptLevel::Less => 2,
            OptLevel::Default => 4,
            OptLevel::Aggressive => 8,
            OptLevel::Size => 3,
        };

        for _ in 0..max_iterations {
            let before = self.stats.total_optimizations();
            self.run_passes(body);
            self.stats.total_passes += 1;

            // Fixed point reached
            if self.stats.total_optimizations() == before {
                break;
            }
        }
    }

    fn run_passes(&mut self, body: &mut MirBody) {
        // Constant folding (first, enables other opts)
        if self.config.enable_constant_folding {
            let folded = passes::constant_folding(body);
            self.stats.constants_folded += folded;
        }

        // Copy propagation
        if self.config.enable_copy_propagation {
            let propagated = passes::copy_propagation(body);
            self.stats.copies_propagated += propagated;
        }

        // Common subexpression elimination
        if self.config.enable_cse {
            let eliminated = passes::common_subexpression_elimination(body);
            self.stats.common_subexpressions_eliminated += eliminated;
        }

        // Strength reduction
        if self.config.enable_strength_reduction {
            let reduced = passes::strength_reduction(body);
            self.stats.strength_reductions += reduced;
        }

        // Loop optimizations (LICM, etc.)
        if self.config.enable_loop_optimizations {
            let optimized = loop_opt::optimize_loops(body);
            self.stats.loops_optimized += optimized;
        }

        // Loop unrolling
        if self.config.enable_loop_unrolling {
            let unrolled = unroll::unroll_loops(body);
            self.stats.loops_unrolled += unrolled;
        }

        // Escape analysis (mark non-escaping allocations for stack)
        if self.config.enable_escape_analysis {
            let stack_allocs = escape::escape_analysis_pass(body);
            self.stats.stack_allocations += stack_allocs;
        }

        // SROA (Scalar Replacement of Aggregates)
        if self.config.enable_sroa {
            let split = sroa::scalar_replacement(body);
            self.stats.aggregates_split += split;
        }

        // Iterator fusion
        if self.config.enable_iterator_fusion {
            let fused = fusion::fuse_iterators(body);
            self.stats.iterators_fused += fused;
        }

        // Tail call optimization
        if self.config.enable_tail_call_optimization {
            let optimized = passes::tail_call_optimization(body);
            self.stats.tail_calls_optimized += optimized;
        }

        // Dead code elimination (last, cleans up)
        if self.config.enable_dead_code_elimination {
            let eliminated = passes::dead_code_elimination(body);
            self.stats.dead_instructions_removed += eliminated;
        }
    }
}

/// Convenience function for optimizing at a specific level.
pub fn optimize(body: &mut MirBody, level: OptLevel) {
    let mut optimizer = Optimizer::for_level(level);
    optimizer.optimize(body);
}

/// Optimize with default settings.
pub fn optimize_default(body: &mut MirBody) {
    optimize(body, OptLevel::Default);
}


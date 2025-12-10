//! JIT Compilation System for Roast
//!
//! This crate implements a tiered JIT compilation system similar to V8 and HotSpot:
//!
//! # Compilation Tiers
//!
//! - **Tier 0 (Interpreter)**: Direct bytecode interpretation, fast startup
//! - **Tier 1 (Baseline)**: Quick template-based compilation, moderate speed
//! - **Tier 2 (Optimizing)**: Full optimization using profile data, maximum speed
//!
//! # Key Features
//!
//! - **Adaptive Compilation**: Functions start interpreted and tier-up based on hotness
//! - **Profile-Guided**: Uses runtime profiling to guide optimization decisions
//! - **On-Stack Replacement (OSR)**: Can tier-up while a function is executing
//! - **Deoptimization**: Can bail out of optimized code when assumptions fail
//! - **Code Cache**: Caches compiled code for reuse
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Bytecode   │────>│  Baseline   │────>│ Optimizing  │
//! │ Interpreter │     │  Compiler   │     │  Compiler   │
//! │  (Tier 0)   │     │  (Tier 1)   │     │  (Tier 2)   │
//! └─────────────┘     └─────────────┘     └─────────────┘
//!        │                   │                   │
//!        │                   │                   │
//!        v                   v                   v
//! ┌─────────────────────────────────────────────────────┐
//! │              Profile Collector                      │
//! └─────────────────────────────────────────────────────┘
//! ```

pub mod tiers;
pub mod baseline;
pub mod optimizing;
pub mod profile;
pub mod code_cache;

pub use tiers::{CompilationTier, TierConfig, TierUpPolicy, TierUpDecision, TierStats};
pub use baseline::{BaselineCompiler, BaselineConfig, BaselineCode};
pub use optimizing::{OptimizingCompiler, OptConfig, OptimizedCode, ObservedType};
pub use profile::{JitProfile, TypeFeedback, ProfileCollector};
pub use code_cache::{CodeCache, CacheConfig, CacheEntry, CacheStats, DeoptHandle};

use thiserror::Error;

/// JIT compilation errors.
#[derive(Error, Debug)]
pub enum JitError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    
    #[error("Tier-up failed: {0}")]
    TierUpFailed(String),
    
    #[error("OSR failed: {0}")]
    OsrFailed(String),
    
    #[error("Deoptimization failed: {0}")]
    DeoptFailed(String),
    
    #[error("Code cache full")]
    CacheFull,
    
    #[error("Code cache error: {0}")]
    CacheError(String),
    
    #[error("Invalid bytecode: {0}")]
    InvalidBytecode(String),
    
    #[error("Cranelift error: {0}")]
    CraneliftError(String),
}

/// Result type for JIT operations.
pub type JitResult<T> = Result<T, JitError>;

/// A compiled function ready for execution.
#[derive(Clone)]
pub struct CompiledFunction {
    /// Function identifier.
    pub func_id: u32,
    /// Compilation tier.
    pub tier: CompilationTier,
    /// Native code bytes.
    pub code: Vec<u8>,
    /// Entry point offset within code.
    pub entry_offset: usize,
    /// Stack frame size.
    pub frame_size: usize,
}

impl CompiledFunction {
    /// Gets the entry point for the compiled code.
    pub fn entry_point(&self) -> *const u8 {
        unsafe { self.code.as_ptr().add(self.entry_offset) }
    }
}

/// The main JIT engine that coordinates compilation.
pub struct JitEngine {
    /// Tier-up policy.
    tier_policy: TierUpPolicy,
    /// Baseline compiler.
    baseline: BaselineCompiler,
    /// Optimizing compiler.
    optimizing: OptimizingCompiler,
    /// Profile collector.
    profiles: ProfileCollector,
    /// Code cache.
    cache: CodeCache,
    /// Configuration.
    config: JitConfig,
}

/// JIT engine configuration.
#[derive(Clone, Debug)]
pub struct JitConfig {
    /// Enable JIT compilation.
    pub enabled: bool,
    /// Enable baseline compilation.
    pub enable_baseline: bool,
    /// Enable optimizing compilation.
    pub enable_optimizing: bool,
    /// Threshold for baseline tier-up.
    pub baseline_threshold: u64,
    /// Threshold for optimizing tier-up.
    pub optimizing_threshold: u64,
    /// Maximum code cache size.
    pub max_cache_size: usize,
}

impl Default for JitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_baseline: true,
            enable_optimizing: true,
            baseline_threshold: 100,
            optimizing_threshold: 10000,
            max_cache_size: 64 * 1024 * 1024,
        }
    }
}

impl JitEngine {
    /// Creates a new JIT engine.
    pub fn new(config: JitConfig) -> Self {
        let tier_config = TierConfig {
            baseline_threshold: config.baseline_threshold,
            optimizing_threshold: config.optimizing_threshold,
            enable_baseline: config.enable_baseline,
            enable_optimizing: config.enable_optimizing,
            enable_osr: true,
            osr_threshold: config.baseline_threshold * 10,
            ..Default::default()
        };
        
        let cache_config = CacheConfig {
            max_size: config.max_cache_size,
            ..Default::default()
        };
        
        Self {
            tier_policy: TierUpPolicy::new(tier_config),
            baseline: BaselineCompiler::new(BaselineConfig::default()),
            optimizing: OptimizingCompiler::new(OptConfig::default()),
            profiles: ProfileCollector::new(),
            cache: CodeCache::new(cache_config),
            config,
        }
    }
    
    /// Gets the current tier for a function.
    pub fn get_tier(&self, func_id: u32) -> CompilationTier {
        self.cache.get_tier(func_id)
    }
    
    /// Records function execution and checks for tier-up.
    pub fn record_execution(&mut self, func_id: u32, bytecode_size: usize) -> TierUpDecision {
        if !self.config.enabled {
            return TierUpDecision::Stay;
        }
        
        self.cache.record_execution(func_id);
        self.tier_policy.record_execution(func_id, bytecode_size)
    }
    
    /// Compiles a function at the baseline tier.
    pub fn compile_baseline(
        &mut self,
        func_id: u32,
        bytecode: &roast_codegen::Bytecode,
    ) -> JitResult<CompiledFunction> {
        let code = self.baseline.compile(func_id, bytecode)?;
        
        self.cache.insert_baseline(func_id, code.clone())?;
        self.tier_policy.notify_compiled(func_id, CompilationTier::Baseline);
        
        Ok(CompiledFunction {
            func_id,
            tier: CompilationTier::Baseline,
            code: code.code,
            entry_offset: code.entry_offset,
            frame_size: code.frame_size,
        })
    }
    
    /// Compiles a function at the optimizing tier.
    pub fn compile_optimizing(
        &mut self,
        func_id: u32,
        bytecode: &roast_codegen::Bytecode,
    ) -> JitResult<CompiledFunction> {
        let profile = self.profiles.get_profile(func_id)
            .unwrap_or_else(|| JitProfile::new(func_id));
        
        let code = self.optimizing.compile(func_id, bytecode, &profile)?;
        
        self.cache.insert_optimized(func_id, code.clone())?;
        self.tier_policy.notify_compiled(func_id, CompilationTier::Optimizing);
        
        Ok(CompiledFunction {
            func_id,
            tier: CompilationTier::Optimizing,
            code: code.code,
            entry_offset: code.entry_offset,
            frame_size: code.frame_size,
        })
    }
    
    /// Gets cached compiled code if available.
    pub fn get_cached(&self, func_id: u32) -> Option<CompiledFunction> {
        let entry = self.cache.get(func_id)?;
        
        let (code, tier) = match entry.tier {
            CompilationTier::Baseline => {
                let baseline = entry.baseline?;
                (baseline.code.clone(), CompilationTier::Baseline)
            }
            CompilationTier::Optimizing => {
                let optimized = entry.optimized?;
                (optimized.code.clone(), CompilationTier::Optimizing)
            }
            CompilationTier::Interpreter => return None,
        };
        
        Some(CompiledFunction {
            func_id,
            tier,
            code,
            entry_offset: 0,
            frame_size: 0,
        })
    }
    
    /// Invalidates compiled code.
    pub fn invalidate(&mut self, func_id: u32) {
        self.cache.invalidate(func_id);
        self.profiles.clear(func_id);
    }
    
    /// Gets cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }
    
    /// Gets profile data for a function.
    pub fn get_profile(&self, func_id: u32) -> Option<JitProfile> {
        self.profiles.get_profile(func_id)
    }
    
    /// Updates profile data.
    pub fn update_profile(&self, profile: JitProfile) {
        self.profiles.update(profile);
    }
}

impl Default for JitEngine {
    fn default() -> Self {
        Self::new(JitConfig::default())
    }
}

//! JIT compilation integration for the VM.
//!
//! This module provides integration between the bytecode interpreter
//! and the JIT compiler, enabling transparent tier-up of hot functions.

#[cfg(feature = "jit")]
use roast_jit::{
    JitEngine, JitConfig, TierUpDecision, CompilationTier, 
    CompiledFunction, JitProfile, ObservedType
};
use roast_codegen::Bytecode;
use std::sync::Arc;
use rustc_hash::FxHashMap;

/// JIT state for a function.
#[derive(Clone)]
pub struct FunctionJitState {
    /// Function ID.
    pub func_id: u32,
    /// Current compilation tier.
    pub tier: JitTier,
    /// Execution counter.
    pub executions: u64,
    /// Whether compilation is pending.
    pub compiling: bool,
}

/// Simplified tier enum for when JIT feature is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JitTier {
    Interpreter,
    Baseline,
    Optimized,
}

impl Default for JitTier {
    fn default() -> Self {
        Self::Interpreter
    }
}

/// Configuration for JIT in the VM.
#[derive(Clone, Debug)]
pub struct VMJitConfig {
    /// Enable JIT compilation.
    pub enabled: bool,
    /// Threshold for baseline tier-up.
    pub baseline_threshold: u64,
    /// Threshold for optimizing tier-up.
    pub optimize_threshold: u64,
    /// Enable On-Stack Replacement.
    pub enable_osr: bool,
    /// Maximum code cache size in bytes.
    pub max_cache_size: usize,
}

impl Default for VMJitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            baseline_threshold: 100,
            optimize_threshold: 10000,
            enable_osr: true,
            max_cache_size: 64 * 1024 * 1024, // 64 MB
        }
    }
}

/// JIT manager for the VM.
/// 
/// This struct manages JIT compilation for the VM, including:
/// - Tracking function execution counts
/// - Deciding when to tier-up
/// - Managing compiled code
pub struct JitManager {
    config: VMJitConfig,
    /// Function states.
    states: FxHashMap<u32, FunctionJitState>,
    /// Next function ID.
    next_func_id: u32,
    /// Map from bytecode pointer to function ID.
    func_ids: FxHashMap<usize, u32>,
    
    #[cfg(feature = "jit")]
    engine: JitEngine,
}

impl JitManager {
    /// Creates a new JIT manager.
    pub fn new(config: VMJitConfig) -> Self {
        #[cfg(feature = "jit")]
        let engine = {
            let jit_config = JitConfig {
                enabled: config.enabled,
                enable_baseline: true,
                enable_optimizing: true,
                baseline_threshold: config.baseline_threshold,
                optimizing_threshold: config.optimize_threshold,
                max_cache_size: config.max_cache_size,
            };
            JitEngine::new(jit_config)
        };
        
        Self {
            config,
            states: FxHashMap::default(),
            next_func_id: 0,
            func_ids: FxHashMap::default(),
            #[cfg(feature = "jit")]
            engine,
        }
    }
    
    /// Gets or creates a function ID for a bytecode object.
    pub fn get_func_id(&mut self, bytecode: &Arc<Bytecode>) -> u32 {
        let ptr = Arc::as_ptr(bytecode) as usize;
        
        if let Some(&id) = self.func_ids.get(&ptr) {
            return id;
        }
        
        let id = self.next_func_id;
        self.next_func_id += 1;
        self.func_ids.insert(ptr, id);
        
        self.states.insert(id, FunctionJitState {
            func_id: id,
            tier: JitTier::Interpreter,
            executions: 0,
            compiling: false,
        });
        
        id
    }
    
    /// Records function entry and checks for tier-up.
    pub fn on_function_entry(&mut self, func_id: u32, bytecode: &Bytecode) -> TierUpAction {
        if !self.config.enabled {
            return TierUpAction::Continue;
        }
        
        let state = match self.states.get_mut(&func_id) {
            Some(s) => s,
            None => return TierUpAction::Continue,
        };
        
        state.executions += 1;
        
        #[cfg(feature = "jit")]
        {
            let bytecode_size = bytecode.instructions.len();
            match self.engine.record_execution(func_id, bytecode_size) {
                TierUpDecision::Stay => TierUpAction::Continue,
                TierUpDecision::TierUp(tier) => {
                    state.compiling = true;
                    match tier {
                        CompilationTier::Baseline => TierUpAction::CompileBaseline,
                        CompilationTier::Optimizing => TierUpAction::CompileOptimizing,
                        _ => TierUpAction::Continue,
                    }
                }
                TierUpDecision::Osr(tier) => {
                    match tier {
                        CompilationTier::Baseline => TierUpAction::OsrBaseline,
                        CompilationTier::Optimizing => TierUpAction::OsrOptimizing,
                        _ => TierUpAction::Continue,
                    }
                }
            }
        }
        
        #[cfg(not(feature = "jit"))]
        {
            let _ = bytecode;
            // Without JIT, just track executions
            if state.tier == JitTier::Interpreter 
                && state.executions >= self.config.baseline_threshold 
            {
                // Would tier-up, but JIT is not available
                TierUpAction::Continue
            } else {
                TierUpAction::Continue
            }
        }
    }
    
    /// Records a loop back-edge for OSR consideration.
    pub fn on_loop_back_edge(&mut self, func_id: u32, loop_id: u32) -> TierUpAction {
        if !self.config.enabled || !self.config.enable_osr {
            return TierUpAction::Continue;
        }
        
        #[cfg(feature = "jit")]
        {
            // Record the loop iteration - OSR decision is made by the JIT engine
            // For now, we just track it
            let _ = (func_id, loop_id);
            TierUpAction::Continue
        }
        
        #[cfg(not(feature = "jit"))]
        {
            let _ = (func_id, loop_id);
            TierUpAction::Continue
        }
    }
    
    /// Compiles a function to baseline tier.
    #[cfg(feature = "jit")]
    pub fn compile_baseline(&mut self, func_id: u32, bytecode: &Bytecode) -> Result<(), String> {
        match self.engine.compile_baseline(func_id, bytecode) {
            Ok(_compiled) => {
                if let Some(state) = self.states.get_mut(&func_id) {
                    state.tier = JitTier::Baseline;
                    state.compiling = false;
                }
                Ok(())
            }
            Err(e) => {
                if let Some(state) = self.states.get_mut(&func_id) {
                    state.compiling = false;
                }
                Err(format!("Baseline compilation failed: {}", e))
            }
        }
    }
    
    /// Compiles a function to optimizing tier.
    #[cfg(feature = "jit")]
    pub fn compile_optimizing(&mut self, func_id: u32, bytecode: &Bytecode) -> Result<(), String> {
        match self.engine.compile_optimizing(func_id, bytecode) {
            Ok(_compiled) => {
                if let Some(state) = self.states.get_mut(&func_id) {
                    state.tier = JitTier::Optimized;
                    state.compiling = false;
                }
                Ok(())
            }
            Err(e) => {
                if let Some(state) = self.states.get_mut(&func_id) {
                    state.compiling = false;
                }
                Err(format!("Optimizing compilation failed: {}", e))
            }
        }
    }
    
    /// Gets the current tier for a function.
    pub fn get_tier(&self, func_id: u32) -> JitTier {
        self.states.get(&func_id)
            .map(|s| s.tier)
            .unwrap_or(JitTier::Interpreter)
    }
    
    /// Gets compiled code for a function, if available.
    #[cfg(feature = "jit")]
    pub fn get_compiled(&self, func_id: u32) -> Option<CompiledFunction> {
        self.engine.get_cached(func_id)
    }
    
    /// Records type feedback for a site.
    #[cfg(feature = "jit")]
    pub fn record_type(&mut self, func_id: u32, site: u32, ty: TypeObservation) {
        let observed_type = match ty {
            TypeObservation::Int => ObservedType::Int,
            TypeObservation::Float => ObservedType::Float,
            TypeObservation::String => ObservedType::String,
            TypeObservation::List => ObservedType::List,
            TypeObservation::Dict => ObservedType::Dict,
            TypeObservation::Object(class_id) => ObservedType::Object { class_id },
        };
        
        let mut profile = self.engine.get_profile(func_id)
            .unwrap_or_else(|| JitProfile::new(func_id));
        profile.record_type(site, observed_type);
        self.engine.update_profile(profile);
    }
    
    /// Invalidates compiled code for a function.
    pub fn invalidate(&mut self, func_id: u32) {
        if let Some(state) = self.states.get_mut(&func_id) {
            state.tier = JitTier::Interpreter;
            state.executions = 0;
        }
        
        #[cfg(feature = "jit")]
        self.engine.invalidate(func_id);
    }
    
    /// Gets JIT statistics.
    pub fn stats(&self) -> JitStats {
        let mut stats = JitStats::default();
        
        for state in self.states.values() {
            match state.tier {
                JitTier::Interpreter => stats.interpreted += 1,
                JitTier::Baseline => stats.baseline += 1,
                JitTier::Optimized => stats.optimized += 1,
            }
        }
        
        #[cfg(feature = "jit")]
        {
            let cache_stats = self.engine.cache_stats();
            stats.cache_hits = cache_stats.hits;
            stats.cache_misses = cache_stats.misses;
            stats.compilations = cache_stats.compilations;
        }
        
        stats
    }
}

impl Default for JitManager {
    fn default() -> Self {
        Self::new(VMJitConfig::default())
    }
}

/// Action to take after tier-up check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TierUpAction {
    /// Continue with current tier.
    Continue,
    /// Compile to baseline.
    CompileBaseline,
    /// Compile to optimizing.
    CompileOptimizing,
    /// On-stack replace to baseline.
    OsrBaseline,
    /// On-stack replace to optimizing.
    OsrOptimizing,
}

/// Type observation for profiling.
#[derive(Clone, Copy, Debug)]
pub enum TypeObservation {
    Int,
    Float,
    String,
    List,
    Dict,
    Object(u32),
}

/// JIT statistics.
#[derive(Clone, Debug, Default)]
pub struct JitStats {
    /// Number of functions at interpreter tier.
    pub interpreted: usize,
    /// Number of functions at baseline tier.
    pub baseline: usize,
    /// Number of functions at optimized tier.
    pub optimized: usize,
    /// Cache hits.
    pub cache_hits: u64,
    /// Cache misses.
    pub cache_misses: u64,
    /// Total compilations.
    pub compilations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_jit_manager_creation() {
        let manager = JitManager::new(VMJitConfig::default());
        assert_eq!(manager.next_func_id, 0);
    }
    
    #[test]
    fn test_func_id_allocation() {
        let mut manager = JitManager::new(VMJitConfig::default());
        let bytecode = Arc::new(Bytecode::new("test"));
        
        let id1 = manager.get_func_id(&bytecode);
        let id2 = manager.get_func_id(&bytecode);
        
        // Same bytecode should get same ID
        assert_eq!(id1, id2);
        
        // Different bytecode should get different ID
        let bytecode2 = Arc::new(Bytecode::new("test2"));
        let id3 = manager.get_func_id(&bytecode2);
        assert_ne!(id1, id3);
    }
    
    #[test]
    fn test_tier_tracking() {
        let mut manager = JitManager::new(VMJitConfig::default());
        let bytecode = Arc::new(Bytecode::new("test"));
        let func_id = manager.get_func_id(&bytecode);
        
        assert_eq!(manager.get_tier(func_id), JitTier::Interpreter);
    }
}

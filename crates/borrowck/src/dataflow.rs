//! Dataflow analysis framework for borrow checking.
//!
//! Provides generic dataflow analysis infrastructure.

use crate::loans::Location;
use petgraph::graph::{DiGraph, NodeIndex};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::hash::Hash;

/// A dataflow analysis.
pub trait DataflowAnalysis {
    /// The domain of the analysis (what we're tracking).
    type Domain: Clone + Eq + Debug;

    /// The initial value at the entry point.
    fn initial_value(&self) -> Self::Domain;

    /// The bottom value (for meet operation).
    fn bottom(&self) -> Self::Domain;

    /// Transfer function: how does the domain change at a statement?
    fn transfer(&self, state: &mut Self::Domain, location: Location, statement: &Statement);

    /// Meet operation: combine states from multiple predecessors.
    fn meet(&self, a: &Self::Domain, b: &Self::Domain) -> Self::Domain;
}

/// A generic statement for dataflow analysis.
#[derive(Clone, Debug)]
pub enum Statement {
    /// Assignment to a place.
    Assign { dest: u32, src: StatementRhs },
    /// Borrow of a place.
    Borrow { dest: u32, place: u32, mutable: bool },
    /// Move of a place.
    Move { dest: u32, src: u32 },
    /// Copy of a place.
    Copy { dest: u32, src: u32 },
    /// Drop of a place.
    Drop { place: u32 },
    /// Function call.
    Call { dest: Option<u32>, func: u32, args: Vec<u32> },
    /// No operation.
    Nop,
}

/// Right-hand side of an assignment.
#[derive(Clone, Debug)]
pub enum StatementRhs {
    Use(u32),
    Ref(u32, bool),
    BinaryOp(u32, u32),
    UnaryOp(u32),
    Constant,
    Aggregate(Vec<u32>),
}

/// A basic block for dataflow analysis.
#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub id: u32,
    pub statements: Vec<Statement>,
    pub successors: Vec<u32>,
    pub predecessors: Vec<u32>,
}

impl BasicBlock {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            statements: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
        }
    }
}

/// Control flow graph for dataflow analysis.
pub struct ControlFlowGraph {
    blocks: Vec<BasicBlock>,
    entry: u32,
    exits: Vec<u32>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            entry: 0,
            exits: Vec::new(),
        }
    }

    /// Adds a basic block.
    pub fn add_block(&mut self, block: BasicBlock) {
        self.blocks.push(block);
    }

    /// Sets the entry block.
    pub fn set_entry(&mut self, entry: u32) {
        self.entry = entry;
    }

    /// Adds an exit block.
    pub fn add_exit(&mut self, exit: u32) {
        self.exits.push(exit);
    }

    /// Gets a block by ID.
    pub fn block(&self, id: u32) -> Option<&BasicBlock> {
        self.blocks.get(id as usize)
    }

    /// Gets a mutable block by ID.
    pub fn block_mut(&mut self, id: u32) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(id as usize)
    }

    /// Returns all blocks.
    pub fn blocks(&self) -> &[BasicBlock] {
        &self.blocks
    }

    /// Returns the entry block ID.
    pub fn entry(&self) -> u32 {
        self.entry
    }

    /// Returns exit block IDs.
    pub fn exits(&self) -> &[u32] {
        &self.exits
    }

    /// Computes the number of locations (statements) in the CFG.
    pub fn num_locations(&self) -> usize {
        self.blocks.iter().map(|b| b.statements.len()).sum()
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a dataflow analysis.
pub struct DataflowResult<D> {
    /// State at the entry of each block.
    pub entry_states: FxHashMap<u32, D>,
    /// State at the exit of each block.
    pub exit_states: FxHashMap<u32, D>,
    /// State before each statement.
    pub before_statement: FxHashMap<Location, D>,
    /// State after each statement.
    pub after_statement: FxHashMap<Location, D>,
}

impl<D: Clone + Eq + Debug> DataflowResult<D> {
    pub fn new() -> Self {
        Self {
            entry_states: FxHashMap::default(),
            exit_states: FxHashMap::default(),
            before_statement: FxHashMap::default(),
            after_statement: FxHashMap::default(),
        }
    }

    /// Gets the state before a statement.
    pub fn state_before(&self, location: Location) -> Option<&D> {
        self.before_statement.get(&location)
    }

    /// Gets the state after a statement.
    pub fn state_after(&self, location: Location) -> Option<&D> {
        self.after_statement.get(&location)
    }
}

impl<D: Clone + Eq + Debug> Default for DataflowResult<D> {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs a forward dataflow analysis.
pub fn run_forward<A: DataflowAnalysis>(
    analysis: &A,
    cfg: &ControlFlowGraph,
) -> DataflowResult<A::Domain> {
    let mut result = DataflowResult::new();
    let mut worklist: VecDeque<u32> = VecDeque::new();

    // Initialize entry states
    for block in cfg.blocks() {
        if block.id == cfg.entry() {
            result.entry_states.insert(block.id, analysis.initial_value());
        } else {
            result.entry_states.insert(block.id, analysis.bottom());
        }
        worklist.push_back(block.id);
    }

    // Iterate until fixpoint
    while let Some(block_id) = worklist.pop_front() {
        let block = cfg.block(block_id).unwrap();
        let mut state = result.entry_states[&block_id].clone();

        // Process each statement
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            let location = Location::new(block_id, stmt_idx as u32);
            result.before_statement.insert(location, state.clone());
            analysis.transfer(&mut state, location, stmt);
            result.after_statement.insert(location, state.clone());
        }

        let old_exit = result.exit_states.get(&block_id).cloned();
        result.exit_states.insert(block_id, state.clone());

        // If exit state changed, add successors to worklist
        if old_exit.as_ref() != Some(&state) {
            for &succ in &block.successors {
                // Merge into successor's entry state
                let succ_entry = result.entry_states.get(&succ).unwrap().clone();
                let merged = analysis.meet(&succ_entry, &state);
                if merged != succ_entry {
                    result.entry_states.insert(succ, merged);
                    if !worklist.contains(&succ) {
                        worklist.push_back(succ);
                    }
                }
            }
        }
    }

    result
}

/// Runs a backward dataflow analysis.
pub fn run_backward<A: DataflowAnalysis>(
    analysis: &A,
    cfg: &ControlFlowGraph,
) -> DataflowResult<A::Domain> {
    let mut result = DataflowResult::new();
    let mut worklist: VecDeque<u32> = VecDeque::new();

    // Initialize exit states
    for block in cfg.blocks() {
        if cfg.exits().contains(&block.id) {
            result.exit_states.insert(block.id, analysis.initial_value());
        } else {
            result.exit_states.insert(block.id, analysis.bottom());
        }
        worklist.push_back(block.id);
    }

    // Iterate until fixpoint
    while let Some(block_id) = worklist.pop_front() {
        let block = cfg.block(block_id).unwrap();
        let mut state = result.exit_states[&block_id].clone();

        // Process statements in reverse
        for (stmt_idx, stmt) in block.statements.iter().enumerate().rev() {
            let location = Location::new(block_id, stmt_idx as u32);
            result.after_statement.insert(location, state.clone());
            analysis.transfer(&mut state, location, stmt);
            result.before_statement.insert(location, state.clone());
        }

        let old_entry = result.entry_states.get(&block_id).cloned();
        result.entry_states.insert(block_id, state.clone());

        // If entry state changed, add predecessors to worklist
        if old_entry.as_ref() != Some(&state) {
            for &pred in &block.predecessors {
                let pred_exit = result.exit_states.get(&pred).unwrap().clone();
                let merged = analysis.meet(&pred_exit, &state);
                if merged != pred_exit {
                    result.exit_states.insert(pred, merged);
                    if !worklist.contains(&pred) {
                        worklist.push_back(pred);
                    }
                }
            }
        }
    }

    result
}

// === Specific Analyses ===

/// Liveness analysis: which variables are live at each point.
#[derive(Clone, Debug, Default)]
pub struct LiveVariables {
    pub live: FxHashSet<u32>,
}

impl LiveVariables {
    pub fn new() -> Self {
        Self {
            live: FxHashSet::default(),
        }
    }

    pub fn insert(&mut self, var: u32) {
        self.live.insert(var);
    }

    pub fn remove(&mut self, var: u32) {
        self.live.remove(&var);
    }

    pub fn is_live(&self, var: u32) -> bool {
        self.live.contains(&var)
    }
}

impl PartialEq for LiveVariables {
    fn eq(&self, other: &Self) -> bool {
        self.live == other.live
    }
}

impl Eq for LiveVariables {}

use rustc_hash::FxHashSet;

/// Liveness analysis implementation.
pub struct LivenessAnalysis;

impl DataflowAnalysis for LivenessAnalysis {
    type Domain = LiveVariables;

    fn initial_value(&self) -> Self::Domain {
        LiveVariables::new()
    }

    fn bottom(&self) -> Self::Domain {
        LiveVariables::new()
    }

    fn transfer(&self, state: &mut Self::Domain, _location: Location, statement: &Statement) {
        match statement {
            Statement::Assign { dest, src } => {
                state.remove(*dest);
                match src {
                    StatementRhs::Use(v) => state.insert(*v),
                    StatementRhs::Ref(v, _) => state.insert(*v),
                    StatementRhs::BinaryOp(a, b) => {
                        state.insert(*a);
                        state.insert(*b);
                    }
                    StatementRhs::UnaryOp(v) => state.insert(*v),
                    StatementRhs::Aggregate(vs) => {
                        for v in vs {
                            state.insert(*v);
                        }
                    }
                    StatementRhs::Constant => {}
                }
            }
            Statement::Borrow { dest, place, .. } => {
                state.remove(*dest);
                state.insert(*place);
            }
            Statement::Move { dest, src } | Statement::Copy { dest, src } => {
                state.remove(*dest);
                state.insert(*src);
            }
            Statement::Drop { place } => {
                state.remove(*place);
            }
            Statement::Call { dest, func, args } => {
                if let Some(d) = dest {
                    state.remove(*d);
                }
                state.insert(*func);
                for arg in args {
                    state.insert(*arg);
                }
            }
            Statement::Nop => {}
        }
    }

    fn meet(&self, a: &Self::Domain, b: &Self::Domain) -> Self::Domain {
        let mut result = a.clone();
        for v in &b.live {
            result.insert(*v);
        }
        result
    }
}


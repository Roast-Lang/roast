//! MIR node definitions.

use id_arena::Id;
use roast_common::{Span, Symbol};
use roast_typer::Type;
use smallvec::SmallVec;

/// A MIR function/body.
#[derive(Clone, Debug)]
pub struct MirBody {
    pub name: Symbol,
    pub params: Vec<MirParam>,
    pub return_ty: Type,
    pub locals: Vec<MirLocal>,
    pub blocks: Vec<MirBlock>,
    pub is_async: bool,
    pub span: Span,
}

/// A parameter with kind information.
#[derive(Clone, Debug)]
pub struct MirParam {
    pub local: MirLocal,
    pub kind: MirParamKind,
}

/// Parameter kind for *args/**kwargs support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MirParamKind {
    #[default]
    Regular,
    /// *args - collects positional arguments into tuple
    VarPositional,
    /// **kwargs - collects keyword arguments into dict
    VarKeyword,
}

/// A local variable.
#[derive(Clone, Debug)]
pub struct MirLocal {
    pub id: LocalId,
    pub name: Option<Symbol>,
    pub ty: Type,
    pub mutable: bool,
}

/// Local variable ID.
pub type LocalId = u32;

/// Block ID.
pub type BlockId = u32;

/// A basic block.
#[derive(Clone, Debug)]
pub struct MirBlock {
    pub id: BlockId,
    pub stmts: Vec<MirStmt>,
    pub terminator: MirTerminator,
}

/// A MIR statement.
#[derive(Clone, Debug)]
pub struct MirStmt {
    pub kind: MirStmtKind,
    pub span: Span,
}

/// Statement kinds.
#[derive(Clone, Debug)]
pub enum MirStmtKind {
    /// Assign to a place.
    Assign { place: MirPlace, value: MirRvalue },
    /// Storage live marker.
    StorageLive(LocalId),
    /// Storage dead marker.
    StorageDead(LocalId),
    /// Append value to list (for list comprehensions).
    ListAppend { list: MirPlace, value: MirOperand },
    /// Set attribute on object: obj.attr = value
    SetAttr { object: MirOperand, attr: Symbol, value: MirOperand },
    /// Set indexed element of an attribute: obj.attr[idx] = value
    SetAttrIndex { object: MirOperand, attr: Symbol, index: MirOperand, value: MirOperand },
    /// End of try block - pops exception frame
    TryEnd,
    /// No-op.
    Nop,
}

/// A place (l-value).
#[derive(Clone, Debug)]
pub struct MirPlace {
    pub local: LocalId,
    pub projections: SmallVec<[MirProjection; 2]>,
}

impl MirPlace {
    pub fn local(id: LocalId) -> Self {
        Self {
            local: id,
            projections: SmallVec::new(),
        }
    }
}

/// Place projection.
#[derive(Clone, Debug)]
pub enum MirProjection {
    Field(u32),
    Index(LocalId),
    /// Slice projection with (lower, upper, step) - each is a LocalId storing the value (or None marker)
    Slice { lower: LocalId, upper: LocalId, step: LocalId },
    Deref,
}

/// An r-value (right-hand side of assignment).
#[derive(Clone, Debug)]
pub enum MirRvalue {
    /// Use a place.
    Use(MirOperand),
    /// Unary operation.
    UnaryOp(MirUnaryOp, MirOperand),
    /// Binary operation.
    BinaryOp(MirBinOp, MirOperand, MirOperand),
    /// Create a reference.
    Ref(MirPlace, bool),
    /// Create aggregate (tuple, struct).
    Aggregate(MirAggregateKind, Vec<MirOperand>),
    /// Length of array/slice.
    Len(MirPlace),
    /// Cast.
    Cast(MirOperand, Type),
    /// Attribute access.
    Attr(MirOperand, Symbol),
    /// Await an async value.
    Await(MirOperand),
}

/// Aggregate kind.
#[derive(Clone, Debug)]
pub enum MirAggregateKind {
    Tuple,
    List,
    Set,
    Dict,
    Struct(Symbol),
    /// Slice with (lower, upper, step) - None values represented as separate flags
    Slice,
    /// Lambda function with parameter names and body expression
    Lambda {
        params: Vec<Symbol>,
        body: Box<MirBody>,
    },
}

/// An operand.
#[derive(Clone, Debug)]
pub enum MirOperand {
    Copy(MirPlace),
    Move(MirPlace),
    Constant(MirConstant),
    /// Global/builtin function reference.
    Global(Symbol),
}

/// A constant value.
#[derive(Clone, Debug)]
pub enum MirConstant {
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    None,
    Unit,
}

/// Block terminator.
#[derive(Clone, Debug)]
pub enum MirTerminator {
    /// Return from function. Optionally carries the return value operand.
    Return(Option<MirOperand>),
    /// Unconditional jump.
    Goto(BlockId),
    /// Conditional branch.
    SwitchInt {
        discr: MirOperand,
        targets: Vec<(i128, BlockId)>,
        otherwise: BlockId,
    },
    /// Function call.
    Call {
        func: MirOperand,
        args: Vec<MirOperand>,
        destination: MirPlace,
        target: Option<BlockId>,
        unwind: Option<BlockId>,
    },
    /// Method call on an object (uses static dispatch based on receiver type)
    MethodCall {
        receiver: MirOperand,       // The object to call method on
        receiver_class: Option<String>,  // The class name of the receiver (for static dispatch)
        receiver_type: Option<Type>,     // The actual type of the receiver (for builtin method detection)
        method: Symbol,             // Method name symbol
        args: Vec<MirOperand>,      // Arguments (not including self)
        destination: MirPlace,
        target: Option<BlockId>,
    },
    /// For loop iteration - calls GetIter on iter and ForIter with proper jump
    ForIter {
        iter: MirPlace,       // The iterator local
        loop_var: LocalId,    // Where to store the next value
        body: BlockId,        // Block for loop body
        exit: BlockId,        // Block for loop exit
    },
    /// Assert condition.
    Assert {
        cond: MirOperand,
        expected: bool,
        target: BlockId,
        msg: String,
    },
    /// Drop value.
    Drop {
        place: MirPlace,
        target: BlockId,
        unwind: Option<BlockId>,
    },
    /// Try block - attempts to execute body, catches exceptions
    TryBegin {
        body: BlockId,           // Block to try executing
        handlers: Vec<MirExceptHandler>, // Exception handlers
        finally: Option<BlockId>, // Finally block (always executed)
        exit: BlockId,           // Normal exit block
    },
    /// Raise an exception
    Raise {
        exc: Option<MirOperand>, // Exception value (None = re-raise)
    },
    /// Unreachable.
    Unreachable,
}

/// Exception handler for MIR try/except
#[derive(Clone, Debug)]
pub struct MirExceptHandler {
    pub exc_type: Option<Symbol>,  // Exception type name (None = catch all)
    pub exc_var: Option<LocalId>,  // Variable to bind exception to
    pub body: BlockId,             // Handler body block
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MirUnaryOp {
    Neg,
    Not,
    BitNot,
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MirBinOp {
    Add, Sub, Mul, Div, FloorDiv, Rem, Pow,
    BitAnd, BitOr, BitXor, Shl, Shr,
    Eq, Ne, Lt, Le, Gt, Ge,
    In, NotIn,
}

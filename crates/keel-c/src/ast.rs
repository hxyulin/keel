//! The typed syntax tree the parser builds. Every expression has its C
//! type, and the parser has made every implicit conversion an explicit
//! `Cast`: the integer promotions, the usual arithmetic conversions,
//! conversion on assignment, array and function decay (`Addr`). Code
//! generation only has to follow it.

use crate::Loc;
use crate::arena::BodyArena;
use crate::types::{BitField, FuncTy, Sugar, Ty};
use keel_ir::{Intrinsic, OverflowOp, RmwOp};

pub type LocalId = usize;

#[derive(Copy, Clone, Debug)]
pub struct Expr<'a> {
    pub kind: ExprKind<'a>,
    /// Qualifier/type shape retained through lvalues and pointer values.
    pub sugar: Sugar<'a>,
    pub ty: Ty<'a>,
    pub loc: Loc,
}

impl<'a> Expr<'a> {
    pub fn with_sugar(mut self, sugar: Sugar<'a>) -> Self {
        self.sugar = sugar;
        self
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ExprKind<'a> {
    /// An integer constant, as the bits of its type (sign-extended).
    Int(i64),
    Float(f64),
    /// A string literal (an array lvalue), by index in the string table.
    Str(usize),
    /// GNU's `&&label`: the label's number in its function's
    /// [`FuncDef::label_addrs`], from 1, as a `void *`.
    LabelAddr(usize),
    Local(LocalId),
    /// A function or an object with static storage, by index in the
    /// global table.
    Global(usize),
    Unary(UnOp, &'a Expr<'a>),
    /// Operands of one type (the result's, or for comparisons the type they
    /// are compared as); shifts convert the amount to the left's type.
    Binary(BinOp, &'a Expr<'a>, &'a Expr<'a>),
    /// `ptr + index * size`, `index` a `long`.
    PtrAdd(&'a Expr<'a>, &'a Expr<'a>, u64),
    /// `(a - b) / size`, a `long`.
    PtrDiff(&'a Expr<'a>, &'a Expr<'a>, u64),
    LogAnd(&'a Expr<'a>, &'a Expr<'a>),
    LogOr(&'a Expr<'a>, &'a Expr<'a>),
    /// The right side is already converted to the left's type.
    Assign(&'a Expr<'a>, &'a Expr<'a>),
    /// `lhs op= rhs`: the left is converted to the right's type, combined
    /// with the right, and converted back.
    OpAssign {
        op: BinOp,
        lhs: &'a Expr<'a>,
        rhs: &'a Expr<'a>,
    },
    /// `p += n` / `p -= n` on a pointer: `n` is a `long`, already negated
    /// for `-=`; `size` is the element size.
    PtrOpAssign {
        lhs: &'a Expr<'a>,
        rhs: &'a Expr<'a>,
        size: u64,
    },
    /// `++` and `--`, prefix or postfix.
    IncDec {
        lhs: &'a Expr<'a>,
        inc: bool,
        prefix: bool,
    },
    Cond(&'a Expr<'a>, &'a Expr<'a>, &'a Expr<'a>),
    /// GNU's statement expression `({ ...; e; })`: the statements, in
    /// a block scope, then the value of the last one if it is an
    /// expression.
    StmtExpr {
        scope: usize,
        stmts: &'a [Stmt<'a>],
        value: Option<&'a Expr<'a>>,
    },
    Comma(&'a Expr<'a>, &'a Expr<'a>),
    /// Calls a function pointer. Arguments are converted to the parameter
    /// types, and extra ones promoted.
    Call(&'a Expr<'a>, &'a [Expr<'a>]),
    /// Converts the operand to this expression's type.
    Cast(&'a Expr<'a>),
    Deref(&'a Expr<'a>),
    Addr(&'a Expr<'a>),
    /// A member of a struct-typed operand, `offset` bytes in.
    Member {
        base: &'a Expr<'a>,
        offset: u64,
        bits: Option<BitField>,
        align: u64,
    },
    /// `va_start(ap)` and `va_arg(ap)` (of this expression's type), given
    /// the address of the `va_list` object. `va_copy` is an assignment, of
    /// the pointer on Apple arm64 and of the struct on SysV.
    VaStart(&'a Expr<'a>),
    VaArg(&'a Expr<'a>),
    Builtin(Builtin, &'a [Expr<'a>]),
    /// A compound literal in a function: a local initialized here.
    CompoundLit(LocalId, &'a Init<'a>),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UnOp {
    Neg,
    BitNot,
    /// `!`: an `int` 0 or 1.
    Not,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Builtin {
    /// Explicit C memory builtin; three operands, or four for its checked
    /// runtime variant. Both return the destination pointer.
    Memory(Intrinsic),
    Bswap,
    Fabs,
    /// The sign bit of a float, as an `int`: 1 for -0.0 and negative NaNs
    /// too.
    Signbit,
    Clz,
    Ctz,
    Popcount,
    /// `(a, b, result_ptr)`: stores the wrapped result and returns `_Bool`.
    Overflow(OverflowOp),
    /// Mixed integer types, exact mathematical representability.
    GenericOverflow(OverflowOp),
    /// Evaluates its arguments once and returns the first value. Branch
    /// likelihood metadata is validated by the parser, then ignored.
    Hint,
    /// Pointer alignment contract; optional offset remains a runtime operand.
    AssumeAligned(u64),
    Alloca,
    Trap,
    /// A resumable debugger breakpoint, unlike the terminating `Trap`.
    Debugtrap,
    /// `__builtin_frame_address(0)`.
    FrameAddress,
    /// The atomic builtins (`__atomic_*`, `__sync_*`, `__c11_atomic_*`),
    /// with their arguments put in one shape, the C11 memory order last
    /// (`__ATOMIC_RELAXED` 0 to `__ATOMIC_SEQ_CST` 5; a non-constant one
    /// is taken as seq_cst).
    ///
    /// `(ptr, order)`: the value.
    AtomicLoad,
    /// `(ptr, value, order)`: the value stored, if the type is not void.
    AtomicStore,
    /// `(ptr, value, order)`: the old value, or with `new` the result.
    AtomicRmw {
        op: RmwOp,
        new: bool,
    },
    /// `(ptr, expected, desired, order)`: the old value.
    AtomicCas,
    /// `(ptr, expected, desired, order)`: whether it swapped.
    AtomicCasBool,
    /// `(ptr, expected_ptr, desired, order)`: whether it swapped; if not,
    /// `*expected_ptr` gets the value found.
    AtomicCompareExchange,
    /// `(order)`.
    Fence,
}

/// An initializer, flattened: the object is zero except for these items.
#[derive(Copy, Clone, Debug, Default)]
pub struct Init<'a> {
    pub items: &'a [InitItem<'a>],
}

#[derive(Copy, Clone, Debug)]
pub enum InitItem<'a> {
    /// A scalar (converted to the member's type) or a struct-typed
    /// expression to copy, at a byte offset.
    Expr {
        offset: u64,
        expr: &'a Expr<'a>,
        bits: Option<BitField>,
    },
    /// The bytes of a string literal initializing a character array.
    Bytes { offset: u64, bytes: &'a [u8] },
}

#[derive(Copy, Clone, Debug)]
pub enum Stmt<'a> {
    Expr(Expr<'a>),
    /// A local's initialization, at its declaration.
    Init(LocalId, Init<'a>, Loc),
    /// A local declared without an initializer. Each time its declaration
    /// is reached it is a new object with an indeterminate value, so its
    /// old value (from the last trip round a loop) is not needed.
    Declare(LocalId),
    Block(&'a [Stmt<'a>]),
    /// Statements in a block scope (a compound statement or a `for`), by
    /// its index in the function's scopes.
    Scoped(usize, &'a [Stmt<'a>]),
    If(Expr<'a>, &'a Stmt<'a>, Option<&'a Stmt<'a>>),
    While(Expr<'a>, &'a Stmt<'a>),
    DoWhile(&'a Stmt<'a>, Expr<'a>),
    /// The expressions are boxed, as are a switch's, to keep statements
    /// (which blocks hold by value) small.
    For {
        init: Option<&'a Stmt<'a>>,
        cond: Option<&'a Expr<'a>>,
        step: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
    },
    /// The cases are the values of the `Case` labels in the body, by their
    /// index, and which one is `default`.
    Switch {
        cond: &'a Expr<'a>,
        body: &'a Stmt<'a>,
        cases: &'a [(i64, usize)],
        default: Option<usize>,
        labels: usize,
    },
    /// The label of a switch case, by index in its switch.
    Case(usize),
    Break,
    Continue,
    Return(Option<Expr<'a>>, Loc),
    Goto(&'a str),
    /// GNU's `goto *p`, to a label whose address was taken.
    GotoIndirect(Expr<'a>),
    Label(&'a str),
}

#[derive(Clone, Debug)]
pub struct Local<'a> {
    /// The source name; made-up ones, which are not identifiers, have a
    /// `.` (`compound literal.3`).
    pub name: &'a str,
    pub ty: Ty<'a>,
    pub sugar: Sugar<'a>,
    pub align: u64,
    /// The block scope it is declared in, `None` for the function's
    /// outermost one.
    pub scope: Option<usize>,
    /// Its address is taken (or it is an aggregate, or `volatile`), so it
    /// lives in memory rather than in an SSA variable.
    pub in_memory: bool,
}

#[derive(Debug)]
pub struct FuncDef<'a> {
    /// Index in the global table.
    pub global: usize,
    pub ty: &'a FuncTy<'a>,
    pub params: Vec<LocalId>,
    pub locals: Vec<Local<'a>>,
    /// The parent of each block scope; `None` for the outermost.
    pub scopes: Vec<Option<usize>>,
    /// The labels whose address is taken (`&&label`), numbered from 1.
    pub label_addrs: Vec<&'a str>,
    pub body: &'a [Stmt<'a>],
    pub loc: Loc,
    /// Where the body's expressions and statements are. They are freed
    /// with the definition, all at once.
    #[allow(dead_code, reason = "owned for its drop")]
    pub nodes: BodyArena,
}

/// A file-scope name: a function or an object with static storage
/// (including `static` locals and file-scope compound literals).
#[derive(Clone, Debug)]
pub struct Global<'a> {
    pub name: &'a str,
    /// The linker name (`__asm("_" "realpath" "$DARWIN_EXTSN")` renames).
    pub symbol: &'a str,
    pub ty: Ty<'a>,
    /// From the last declaration.
    pub sugar: Sugar<'a>,
    pub align: u64,
    /// `static`, or a C99 `inline` definition without `extern`.
    pub internal: bool,
    /// Defined in this file: a function body, an initializer, or a
    /// tentative definition (`int x;`).
    pub defined: bool,
    pub init: Option<Init<'a>>,
    pub is_const: bool,
    pub thread_local: bool,
    pub returns_twice: bool,
    pub function_attrs: keel_ir::FunctionAttrs,
    /// Index in the function list, for a defined function.
    pub func: Option<usize>,
}

//! The typed syntax tree the parser builds. Every expression has its C
//! type, and the parser has made every implicit conversion an explicit
//! `Cast`: the integer promotions, the usual arithmetic conversions,
//! conversion on assignment, array and function decay (`Addr`). Code
//! generation only has to follow it.

use crate::Loc;
use crate::types::{BitField, FuncTy, Sugar, Ty};
use keel_ir::{OverflowOp, RmwOp};
use std::rc::Rc;

pub type LocalId = usize;

#[derive(Clone, Debug)]
pub struct Expr<'a> {
    pub kind: ExprKind<'a>,
    /// Qualifier/type shape retained through lvalues and pointer values.
    pub sugar: Sugar,
    pub ty: Ty,
    pub loc: Loc,
}

impl<'a> Expr<'a> {
    pub fn with_sugar(mut self, sugar: Sugar) -> Self {
        self.sugar = sugar;
        self
    }
}

#[derive(Clone, Debug)]
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
    Unary(UnOp, Box<Expr<'a>>),
    /// Operands of one type (the result's, or for comparisons the type they
    /// are compared as); shifts convert the amount to the left's type.
    Binary(BinOp, Box<Expr<'a>>, Box<Expr<'a>>),
    /// `ptr + index * size`, `index` a `long`.
    PtrAdd(Box<Expr<'a>>, Box<Expr<'a>>, u64),
    /// `(a - b) / size`, a `long`.
    PtrDiff(Box<Expr<'a>>, Box<Expr<'a>>, u64),
    LogAnd(Box<Expr<'a>>, Box<Expr<'a>>),
    LogOr(Box<Expr<'a>>, Box<Expr<'a>>),
    /// The right side is already converted to the left's type.
    Assign(Box<Expr<'a>>, Box<Expr<'a>>),
    /// `lhs op= rhs`: the left is converted to `ty`, combined with the right
    /// (already of `ty`), and converted back.
    OpAssign {
        op: BinOp,
        ty: Ty,
        lhs: Box<Expr<'a>>,
        rhs: Box<Expr<'a>>,
    },
    /// `p += n` / `p -= n` on a pointer: `n` is a `long`, already negated
    /// for `-=`; `size` is the element size.
    PtrOpAssign {
        lhs: Box<Expr<'a>>,
        rhs: Box<Expr<'a>>,
        size: u64,
    },
    /// `++` and `--`, prefix or postfix.
    IncDec {
        lhs: Box<Expr<'a>>,
        inc: bool,
        prefix: bool,
    },
    Cond(Box<Expr<'a>>, Box<Expr<'a>>, Box<Expr<'a>>),
    /// GNU's statement expression `({ ...; e; })`: the statements, in
    /// a block scope, then the value of the last one if it is an
    /// expression.
    StmtExpr {
        scope: usize,
        stmts: Vec<Stmt<'a>>,
        value: Option<Box<Expr<'a>>>,
    },
    Comma(Box<Expr<'a>>, Box<Expr<'a>>),
    /// Calls a function pointer. Arguments are converted to the parameter
    /// types, and extra ones promoted.
    Call(Box<Expr<'a>>, Vec<Expr<'a>>),
    /// Converts the operand to this expression's type.
    Cast(Box<Expr<'a>>),
    Deref(Box<Expr<'a>>),
    Addr(Box<Expr<'a>>),
    /// A member of a struct-typed operand, `offset` bytes in.
    Member {
        base: Box<Expr<'a>>,
        offset: u64,
        bits: Option<BitField>,
        align: u64,
    },
    /// `va_start(ap)` and `va_arg(ap)` (of this expression's type), given
    /// the address of the `va_list` object. `va_copy` is an assignment, of
    /// the pointer on Apple arm64 and of the struct on SysV.
    VaStart(Box<Expr<'a>>),
    VaArg(Box<Expr<'a>>),
    Builtin(Builtin, Vec<Expr<'a>>),
    /// A compound literal in a function: a local initialized here.
    CompoundLit(LocalId, Box<Init<'a>>),
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
#[derive(Clone, Debug, Default)]
pub struct Init<'a> {
    pub items: Vec<InitItem<'a>>,
}

#[derive(Clone, Debug)]
pub enum InitItem<'a> {
    /// A scalar (converted to the member's type) or a struct-typed
    /// expression to copy, at a byte offset.
    Expr {
        offset: u64,
        expr: Expr<'a>,
        bits: Option<BitField>,
    },
    /// The bytes of a string literal initializing a character array.
    Bytes { offset: u64, bytes: &'a [u8] },
}

#[derive(Clone, Debug)]
pub enum Stmt<'a> {
    Expr(Expr<'a>),
    /// A local's initialization, at its declaration.
    Init(LocalId, Init<'a>, Loc),
    /// A local declared without an initializer. Each time its declaration
    /// is reached it is a new object with an indeterminate value, so its
    /// old value (from the last trip round a loop) is not needed.
    Declare(LocalId),
    Block(Vec<Stmt<'a>>),
    /// Statements in a block scope (a compound statement or a `for`), by
    /// its index in the function's scopes.
    Scoped(usize, Vec<Stmt<'a>>),
    If(Expr<'a>, Box<Stmt<'a>>, Option<Box<Stmt<'a>>>),
    While(Expr<'a>, Box<Stmt<'a>>),
    DoWhile(Box<Stmt<'a>>, Expr<'a>),
    For {
        init: Option<Box<Stmt<'a>>>,
        cond: Option<Expr<'a>>,
        step: Option<Expr<'a>>,
        body: Box<Stmt<'a>>,
    },
    /// The cases are the values of the `Case` labels in the body, by their
    /// index, and which one is `default`.
    Switch {
        cond: Expr<'a>,
        body: Box<Stmt<'a>>,
        cases: Vec<(i64, usize)>,
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
    pub ty: Ty,
    pub sugar: Sugar,
    pub align: u64,
    /// The block scope it is declared in, `None` for the function's
    /// outermost one.
    pub scope: Option<usize>,
    /// Its address is taken (or it is an aggregate, or `volatile`), so it
    /// lives in memory rather than in an SSA variable.
    pub in_memory: bool,
}

#[derive(Clone, Debug)]
pub struct FuncDef<'a> {
    /// Index in the global table.
    pub global: usize,
    pub ty: Rc<FuncTy>,
    pub params: Vec<LocalId>,
    pub locals: Vec<Local<'a>>,
    /// The parent of each block scope; `None` for the outermost.
    pub scopes: Vec<Option<usize>>,
    /// The labels whose address is taken (`&&label`), numbered from 1.
    pub label_addrs: Vec<&'a str>,
    pub body: Vec<Stmt<'a>>,
    pub loc: Loc,
}

/// A file-scope name: a function or an object with static storage
/// (including `static` locals and file-scope compound literals).
#[derive(Clone, Debug)]
pub struct Global<'a> {
    pub name: &'a str,
    /// The linker name (`__asm("_" "realpath" "$DARWIN_EXTSN")` renames).
    pub symbol: &'a str,
    pub ty: Ty,
    /// From the last declaration.
    pub sugar: Sugar,
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
    pub noreturn: bool,
    pub function_attrs: keel_ir::FunctionAttrs,
    /// Index in the function list, for a defined function.
    pub func: Option<usize>,
}

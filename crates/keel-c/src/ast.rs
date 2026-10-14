//! The typed syntax tree the parser builds. Every expression has its C
//! type, and the parser has made every implicit conversion an explicit
//! `Cast`: the integer promotions, the usual arithmetic conversions,
//! conversion on assignment, array and function decay (`Addr`). Code
//! generation only has to follow it.

use crate::Loc;
use crate::types::{BitField, FuncTy, Ty};
use std::rc::Rc;

pub type LocalId = usize;

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    /// An integer constant, as the bits of its type (sign-extended).
    Int(i64),
    Float(f64),
    /// A string literal (an array lvalue), by index in the string table.
    Str(usize),
    Local(LocalId),
    /// A function or an object with static storage, by index in the
    /// global table.
    Global(usize),
    Unary(UnOp, Box<Expr>),
    /// Operands of one type (the result's, or for comparisons the type they
    /// are compared as); shifts convert the amount to the left's type.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `ptr + index * size`, `index` a `long`.
    PtrAdd(Box<Expr>, Box<Expr>, u64),
    /// `(a - b) / size`, a `long`.
    PtrDiff(Box<Expr>, Box<Expr>, u64),
    LogAnd(Box<Expr>, Box<Expr>),
    LogOr(Box<Expr>, Box<Expr>),
    /// The right side is already converted to the left's type.
    Assign(Box<Expr>, Box<Expr>),
    /// `lhs op= rhs`: the left is converted to `ty`, combined with the right
    /// (already of `ty`), and converted back.
    OpAssign {
        op: BinOp,
        ty: Ty,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `p += n` / `p -= n` on a pointer: `n` is a `long`, already negated
    /// for `-=`; `size` is the element size.
    PtrOpAssign {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        size: u64,
    },
    /// `++` and `--`, prefix or postfix.
    IncDec {
        lhs: Box<Expr>,
        inc: bool,
        prefix: bool,
    },
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Comma(Box<Expr>, Box<Expr>),
    /// Calls a function pointer. Arguments are converted to the parameter
    /// types, and extra ones promoted.
    Call(Box<Expr>, Vec<Expr>),
    /// Converts the operand to this expression's type.
    Cast(Box<Expr>),
    Deref(Box<Expr>),
    Addr(Box<Expr>),
    /// A member of a struct-typed operand, `offset` bytes in.
    Member {
        base: Box<Expr>,
        offset: u64,
        bits: Option<BitField>,
    },
    /// `va_start(ap)` and `va_arg(ap)` (of this expression's type); `ap`
    /// is the `va_list` lvalue. `va_copy` is an assignment, as `va_list`
    /// is a pointer.
    VaStart(Box<Expr>),
    VaArg(Box<Expr>),
    Builtin(Builtin, Vec<Expr>),
    /// A compound literal in a function: a local initialized here.
    CompoundLit(LocalId, Box<Init>),
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
    Clz,
    Ctz,
    Popcount,
    Alloca,
    Trap,
    /// `__atomic_load_n(ptr, order)` and `__atomic_store_n(ptr, value,
    /// order)`; the order is a constant.
    AtomicLoad,
    AtomicStore,
}

/// An initializer, flattened: the object is zero except for these items.
#[derive(Clone, Debug, Default)]
pub struct Init {
    pub items: Vec<InitItem>,
}

#[derive(Clone, Debug)]
pub enum InitItem {
    /// A scalar (converted to the member's type) or a struct-typed
    /// expression to copy, at a byte offset.
    Expr {
        offset: u64,
        expr: Expr,
        bits: Option<BitField>,
    },
    /// The bytes of a string literal initializing a character array.
    Bytes { offset: u64, bytes: Vec<u8> },
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr),
    /// A local's initialization, at its declaration.
    Init(LocalId, Init, Loc),
    /// A local declared without an initializer. Each time its declaration
    /// is reached it is a new object with an indeterminate value, so its
    /// old value (from the last trip round a loop) is not needed.
    Declare(LocalId),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    /// The cases are the values of the `Case` labels in the body, by their
    /// index, and which one is `default`.
    Switch {
        cond: Expr,
        body: Box<Stmt>,
        cases: Vec<(i64, usize)>,
        default: Option<usize>,
        labels: usize,
    },
    /// The label of a switch case, by index in its switch.
    Case(usize),
    Break,
    Continue,
    Return(Option<Expr>, Loc),
    Goto(String),
    Label(String),
}

#[derive(Clone, Debug)]
pub struct Local {
    pub ty: Ty,
    /// Its address is taken (or it is an aggregate, or `volatile`), so it
    /// lives in memory rather than in an SSA variable.
    pub in_memory: bool,
}

#[derive(Clone, Debug)]
pub struct FuncDef {
    /// Index in the global table.
    pub global: usize,
    pub ty: Rc<FuncTy>,
    pub params: Vec<LocalId>,
    pub locals: Vec<Local>,
    pub body: Vec<Stmt>,
    pub loc: Loc,
}

/// A file-scope name: a function or an object with static storage
/// (including `static` locals and file-scope compound literals).
#[derive(Clone, Debug)]
pub struct Global {
    pub name: String,
    /// The linker name (`__asm("_" "realpath" "$DARWIN_EXTSN")` renames).
    pub symbol: String,
    pub ty: Ty,
    /// `static`, or a C99 `inline` definition without `extern`.
    pub internal: bool,
    /// Defined in this file: a function body, an initializer, or a
    /// tentative definition (`int x;`).
    pub defined: bool,
    pub init: Option<Init>,
    pub is_const: bool,
    pub thread_local: bool,
    pub returns_twice: bool,
    /// Index in the function list, for a defined function.
    pub func: Option<usize>,
}

//! Keel IR: a CFG of blocks with block arguments, pinned side-effecting
//! instructions and floating pure ones. See `docs/design.md`.

pub mod dominators;
pub mod entity;
pub mod function;
pub mod inst;
pub mod module;
pub mod parser;
pub mod types;
pub mod verifier;

pub use entity::{Arena, EntityRef};
pub use function::{CallConv, FuncDecl, Function, Signature, StackSlotData, ValueData, ValueDef};
pub use inst::{BinaryOp, BlockCall, ConvertOp, InstData, IntCC, UnaryOp};
pub use module::Module;
pub use parser::{ParseError, parse};
pub use types::Type;
pub use verifier::{VerifierError, verify, verify_module};

entity!(
    /// An SSA value: an instruction result or a block parameter.
    Value,
    "v"
);
entity!(
    /// An instruction.
    Inst,
    "inst"
);
entity!(
    /// A basic block.
    Block,
    "block"
);
entity!(
    /// A function this function calls, declared in its `FuncDecl` table.
    FuncRef,
    "fn"
);
entity!(
    /// A fixed-size region of the function's stack frame.
    StackSlot,
    "ss"
);

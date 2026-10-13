//! Keel IR: a CFG of blocks with block arguments, pinned side-effecting
//! instructions and floating pure ones. See `docs/design.md`.

pub mod entity;
pub mod function;
pub mod inst;
pub mod types;

pub use entity::{Arena, EntityRef};
pub use function::{CallConv, Function, Signature, ValueData, ValueDef};
pub use inst::{BinaryOp, BlockCall, ConvertOp, InstData, IntCC};
pub use types::Type;

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

//! Keel IR: a CFG of blocks with block arguments, pinned side-effecting
//! instructions and floating pure ones. See `docs/design.md`.

pub mod dominators;
pub mod entity;
pub mod function;
pub mod inst;
pub mod parser;
pub mod types;
pub mod verifier;

pub use entity::{Arena, EntityRef};
pub use function::{CallConv, Function, Signature, ValueData, ValueDef};
pub use inst::{BinaryOp, BlockCall, ConvertOp, InstData, IntCC};
pub use parser::{ParseError, parse};
pub use types::Type;
pub use verifier::{VerifierError, verify};

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

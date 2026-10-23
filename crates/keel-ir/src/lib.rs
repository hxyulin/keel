//! Keel IR: a CFG of blocks with block arguments, pinned side-effecting
//! instructions and floating pure ones. See `docs/design.md`.

pub mod asm;
pub mod builder;
pub mod data;
pub mod debug;
pub mod dominators;
pub mod entity;
pub mod function;
pub mod fxhash;
pub mod inst;
pub mod intrinsic;
pub mod layout;
pub mod loops;
pub mod memory;
mod neon;
pub mod module;
pub mod parser;
pub mod section;
pub use section::SectionName;
pub mod target;
pub mod types;
pub mod vector;
pub mod verifier;
pub mod x87_owned_stack;

pub use asm::{AsmClobber, AsmOperand, InlineAsm, PhysReg};
pub use builder::InstBuilder;
pub use data::{DataInit, DataItem, DataObject};
pub use debug::{
    DebugGlobal, DebugMember, DebugScope, DebugType, DebugTypeData, DebugVar, Encoding, Language,
    MemberLocation,
};
pub use entity::{Arena, EntityRef};
pub use function::{
    AbiParam, CallArgs, ArgExt, CallConv, FuncDecl, Function, FunctionAttrs, InsertPoint, Purpose, Signature,
    SourceLoc, StackSlotData, ValueData, ValueDef, X86VectorAbi,
};
pub use inst::{
    BinaryOp, BlockCall, Clause, ConvertOp, FloatCC, InstData, IntCC, Ordering, OverflowOp, RmwOp,
    Symbol, UnaryOp,
};
pub use intrinsic::{Intrinsic, IntrinsicInfo};
pub use layout::Layout;
pub use memory::{MemoryEffects, MemoryFlags};
pub use module::Module;
pub use parser::{ParseError, parse, parse_with_ir_lines};
pub use target::Target;
pub use types::Type;
pub use neon::NeonIntrinsic;
pub use vector::{VectorBinaryOp, VectorReduceOp, VectorType};
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
entity!(
    /// A data object this function takes the address of, by name.
    DataRef,
    "data"
);
entity!(
    /// A signature used by an indirect call.
    SigRef,
    "sig"
);
entity!(
    /// One of the module's source files, which instruction locations refer to.
    SourceFile,
    "f"
);

/// Closed CFG verification for the explicit terminal-monitor FLAGS wrapper.
pub mod rflags_region;
/// Non-emitting physical stack/frame descriptors; structural acceptance only.
pub mod stack_region;
mod x87_region;
pub use x87_region::{X87RegionOp, X87RegionOperand, X87_REGION_OPS, x87_region_control};

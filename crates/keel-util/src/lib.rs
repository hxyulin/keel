//! Small helpers that the assembler, linker and code generator share. The
//! crate has no dependencies, so the linker can use it without depending on
//! the assembler.
pub mod leb128;

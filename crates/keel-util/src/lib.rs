//! Small helpers that the assembler, linker, code generator and command-line
//! tools all need. The crate has no dependencies, so the linker can use it
//! without depending on the assembler.
pub mod fs;
pub mod leb128;

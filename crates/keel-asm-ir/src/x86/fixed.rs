//! Fixed assembler identities. Execution semantics remain separate.
include!("isa/fixed_opcodes.rs");
impl FixedOpcode {
    /// Catalogue mnemonic, operand-size selection and optional leading FWAIT.
    pub const fn source(self) -> (&'static str, Option<u16>, bool) {
        super::isa::fixed::source(self)
    }
    pub const fn bytes(self) -> &'static [u8] {
        super::isa::fixed::bytes(self)
    }
}

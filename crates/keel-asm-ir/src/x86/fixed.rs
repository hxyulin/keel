//! Fixed assembler identities. Execution semantics remain separate.
include!("isa/fixed_opcodes.rs");
impl FixedOpcode {
    pub const fn bytes(self) -> &'static [u8] {
        super::isa::fixed::bytes(self)
    }
}

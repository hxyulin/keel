//! Optional assembler identities. Compiler semantics remain separate.
include!("isa/extension_opcodes.rs");
impl ExtensionOpcode {
    pub fn required_features(self) -> super::Features {
        super::isa::opcode_features(super::Opcode::Extension(self))
    }
}

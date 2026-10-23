//! Ordinary owned 14/28-byte environments and 94/108-byte register images.
//! No hidden save/restore: caller owns nonempty physical state until explicitly
//! returning it empty/TOP0, before F80 computation or any ABI boundary.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_legacy_x87_image(&mut self, id: Intrinsic, args: &[Value]) {
        let (op, bits, _) = id.x86_legacy_x87_image().unwrap();
        // GPR-only address preparation cannot touch x87, SIMD, CW or SW. Live
        // F80 values already have memory homes. Unlike FXRSTOR, FRSTOR/FLDENV
        // have no GPR/SIMD clobbers; allocator call-clobber spilling is unnecessary.
        let address = self.use_reg(args[0], S0);
        // Keep WAIT explicit in both typed and text output. LLVM's assembler
        // misplaces 66 before WAIT for `data16 fstenv/fsave`; the FN spelling
        // ensures every consumer applies the override to the memory instruction.
        let op = match op {
            "fstenv" => {
                self.emit(Opcode::parse("fwait").unwrap(), []);
                "fnstenv"
            }
            "fsave" => {
                self.emit(Opcode::parse("fwait").unwrap(), []);
                "fnsave"
            }
            other => other,
        };
        self.emit(
            Opcode::parse(op).unwrap(),
            [asm_arg(sized(memory(r64(address), 0), bits))],
        );
    }
}

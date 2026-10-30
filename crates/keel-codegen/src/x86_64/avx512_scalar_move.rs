//! Scalar bit transfers distinguish register passthrough from load zeroing.
use super::*;
use keel_asm_ir::x86::evex::EvexOpcode;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("scalar EVEX move requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("scalar EVEX move signature mismatch");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    fn scalar_move_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                Opcode::Movups,
                [asm_arg(xmm(scratch)), asm_arg(xmm(source))],
            );
        }
    }

    pub(super) fn native_avx512_scalar_move(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let info = id.info();
        let memory_form = info.params[0] == Type::Ptr;
        let store = info.returns.is_empty();
        let container = if memory_form {
            info.params[1]
        } else {
            info.params[0]
        };
        let double = container == Type::Vector(keel_ir::VectorType::F64x2);
        let mut opcode = EvexOpcode::parse(if double { "vmovsd" } else { "vmovss" })
            .expect("scalar EVEX move opcode");
        opcode.options.mask = Some(1);
        if memory_form {
            self.scalar_move_stage(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let address = asm_arg(sized(memory(r64(base), 0), if double { 64 } else { 32 }));
            if store {
                self.emit(Opcode::Evex(opcode), [address, asm_arg(xmm(FS0))]);
                return;
            }
            // The memory form clears upper XMM bits even when mask bit zero is clear.
            self.emit(Opcode::Evex(opcode), [asm_arg(xmm(FS0)), address]);
        } else {
            for (&value, scratch) in [args[0], args[2], args[3]].iter().zip([FS0, FS1, FS2]) {
                self.scalar_move_stage(value, scratch);
            }
            self.avx512_stage_mask(args[1]);
            self.emit(
                Opcode::Evex(opcode),
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
            );
        }
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                Opcode::Movups,
                [asm_arg(xmm(destination)), asm_arg(xmm(FS0))],
            );
        }
        self.def_done(result, destination);
    }
}

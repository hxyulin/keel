//! AVX512F packed lane transfers. Memory forms access only the packed prefix.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f || (id.x86_avx512_vl_pack().is_some() && !features.avx512vl) {
        return Err(
            "AVX512 compression/expansion requires AVX512F and narrow forms require AVX512VL",
        );
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 compression/expansion signature mismatch");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_pack(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        let info = id.info();
        let ty = *info
            .params
            .iter()
            .find(|ty| ty.is_vector())
            .expect("pack vector");
        let width = (ty.bytes().expect("pack width") * 8) as u16;
        let name = if let Some(descriptor) = id.x86_avx512_vl_pack() {
            descriptor.opcode
        } else {
            match id {
                X86Avx512Compressd | X86Avx512CompressStored => "vpcompressd",
                X86Avx512Compressq | X86Avx512CompressStoreq => "vpcompressq",
                X86Avx512Compressps | X86Avx512CompressStoreps => "vcompressps",
                X86Avx512Compresspd | X86Avx512CompressStorepd => "vcompresspd",
                X86Avx512Expandd | X86Avx512ExpandLoadd => "vpexpandd",
                X86Avx512Expandq | X86Avx512ExpandLoadq => "vpexpandq",
                X86Avx512Expandps | X86Avx512ExpandLoadps => "vexpandps",
                X86Avx512Expandpd | X86Avx512ExpandLoadpd => "vexpandpd",
                _ => unreachable!("AVX512 packed lane intrinsic"),
            }
        };
        let opcode = avx512_mask::opcode(name, true);
        if id.is_x86_avx512_compress_store() || id.is_x86_avx512_expand_load() {
            self.avx512_stage_vector(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            // The encoded capacity matches the vector width. Hardware accesses exactly
            // popcount(mask) contiguous elements, including none for mask zero.
            let memory = sized(memory(r64(base), 0), width);
            if id.is_x86_avx512_compress_store() {
                self.emit(opcode, [asm_arg(memory), asm_arg(simd_width(FS0, width))]);
                return;
            }
            self.emit(opcode, [asm_arg(simd_width(FS0, width)), asm_arg(memory)]);
        } else {
            // Compression keeps the merge tail above popcount(mask); expansion
            // keeps merge lanes at inactive mask positions. Capture both inputs
            // before modifying either, even when allocation aliases locations.
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            self.avx512_stage_mask(args[1]);
            self.emit(
                opcode,
                [
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS1, width)),
                ],
            );
        }
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                Opcode::Vmovups,
                [
                    asm_arg(simd_width(destination, width)),
                    asm_arg(simd_width(FS0, width)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}

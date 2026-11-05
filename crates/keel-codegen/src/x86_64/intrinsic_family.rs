//! One exhaustive dispatch per x86 intrinsic family. Every [`X86Family`]
//! variant needs a validation arm and a lowering arm, so a new family
//! does not compile until it has both. The target-independent intrinsics
//! (libc memory calls, stack and frame access, `va_start`) are lowered
//! here too.
use super::*;
use keel_ir::X86Family;
use keel_ir::X86Family as F;

/// Target f, operand types and immediate ranges, checked before
/// frame planning so an unsupported intrinsic fails with a message.
pub(super) fn validate(
    family: X86Family,
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    f: keel_asm_ir::x86::Features,
) -> Result<(), String> {
    match family {
        F::Aes => {
            if !f.aes {
                return Err(format!("{id} requires AES"));
            }
            let params = id.info().params;
            if args.len() != params.len()
                || args
                    .iter()
                    .zip(params)
                    .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
            {
                return Err(format!("{id} operand types or arity mismatch"));
            }
            if id == Intrinsic::X86Aeskeygenassist128
                && !args
                    .get(1)
                    .and_then(|&v| iconst(func, v))
                    .is_some_and(|n| (0..256).contains(&n))
            {
                return Err("AESKEYGENASSIST requires iconst.i32 in0..255".into());
            }
        }
        F::Avx512BwConvert => avx512_bw_convert::validate(func, id, args, f)?,
        F::Avx512BwCore => avx512_bw::validate(func, id, args, f)?,
        F::Avx512BwMask => avx512_bw_mask::validate(func, id, args, f)?,
        F::Avx512BwMaskAlu => avx512_bw_mask_alu::validate(func, id, args, f)?,
        F::Avx512BwMisc => avx512_bw_misc::validate(func, id, args, f)?,
        F::Avx512BwMul => avx512_bw_mul::validate(func, id, args, f)?,
        F::Avx512BwPack => avx512_bw_pack::validate(func, id, args, f)?,
        F::Avx512BwPermute => avx512_bw_permute::validate(func, id, args, f)?,
        F::Avx512BwShift => avx512_bw_shift::validate(func, id, args, f)?,
        F::Avx512Convert => avx512_convert::validate(func, id, args, f)?,
        F::Avx512DqBridge => avx512_dq_bridge::validate(func, id, args, f)?,
        F::Avx512DqConvert => avx512_dq_convert::validate(func, id, args, f)?,
        F::Avx512DqLane => avx512_dq_lane::validate(func, id, args, f)?,
        F::Avx512DqMaskAlu => avx512_dq_mask_alu::validate(func, id, args, f)?,
        F::Avx512DqSpecial => avx512_dq_special::validate(func, id, args, f)?,
        F::Avx512Fma => avx512_fma::validate(func, id, args, f)?,
        F::Avx512Indexed => avx512_indexed::validate(func, id, args, f)?,
        F::Avx512Integer => avx512_integer_intrinsics::validate(func, id, args, f)?,
        F::Avx512IntegerConvert => avx512_integer_convert::validate(func, id, args, f)?,
        F::Avx512Lane => avx512_lane::validate(func, id, args, f)?,
        F::Avx512Mask => avx512_mask::validate(func, id, args, f)?,
        F::Avx512MaskAlu => avx512_mask_alu::validate(func, id, args, f)?,
        F::Avx512Pack => avx512_pack::validate(func, id, args, f)?,
        F::Avx512Packed => avx512_packed::validate(func, id, args, f)?,
        F::Avx512Permute => avx512_permute::validate(func, id, args, f)?,
        F::Avx512Scalar => avx512_scalar::validate(func, id, args, f)?,
        F::Avx512ScalarConvert => avx512_scalar_convert::validate(func, id, args, f)?,
        F::Avx512ScalarFma => avx512_scalar_fma::validate(func, id, args, f)?,
        F::Avx512ScalarMove => avx512_scalar_move::validate(func, id, args, f)?,
        F::Avx512Special => avx512_special::validate(func, id, args, f)?,
        F::Avx512VlConvert => avx512_vl_convert::validate(func, id, args, f)?,
        F::Avx512VlInteger => avx512_vl_integer::validate(func, id, args, f)?,
        F::Avx512VlLane => avx512_vl_lane::validate(func, id, args, f)?,
        F::Avx512VlNarrow => avx512_vl_narrow::validate(func, id, args, f)?,
        F::Avx512VlShift => avx512_vl_shift::validate(func, id, args, f)?,
        F::Avx512VlWiden => avx512_vl_widen::validate(func, id, args, f)?,
        F::Bmi => {
            let bmi1 = matches!(id, Intrinsic::X86Bmi1Bextr32 | Intrinsic::X86Bmi1Bextr64);
            if !(if bmi1 { f.bmi1 } else { f.bmi2 }) {
                return Err(format!("{id} requires BMI{}", if bmi1 { 1 } else { 2 }));
            }
        }
        F::ByteMaskStore => {
            if id == Intrinsic::X86AvxVmaskmovdqu && !f.avx {
                return Err(format!("{id} requires AVX"));
            }
        }
        F::ByteShuffle => {
            let enabled = match id {
                Intrinsic::X86Ssse3Pshufb128 => f.ssse3,
                Intrinsic::X86AvxVpshufb128 => f.avx,
                Intrinsic::X86Avx2Vpshufb256 => f.avx && f.avx2,
                _ => unreachable!(),
            };
            if !enabled {
                return Err(format!("{id} requires its SSSE3/AVX/AVX2 feature"));
            }
        }
        F::CacheStream => {
            let name = id.info().name;
            if id == Intrinsic::X86Clflush && !f.clflush {
                return Err(format!("{id} requires CLFSH (+clflush)"));
            }
            if name.starts_with("x86_avx2_") && !(f.avx && f.avx2) {
                return Err(format!("{id} requires AVX2"));
            }
            if name.starts_with("x86_avx_") && !f.avx {
                return Err(format!("{id} requires AVX"));
            }
            if id == Intrinsic::X86Sse41Movntdqa128 && !f.sse41 {
                return Err(format!("{id} requires SSE4.1"));
            }
        }
        F::CpuControl => {
            if id == Intrinsic::X86Xgetbv0 && !(f.xsave && f.osxsave) {
                return Err(format!("{id} requires XSAVE and OSXSAVE"));
            }
        }
        F::Crc32 => {
            if !f.sse42 {
                return Err(format!("{id} requires SSE4.2"));
            }
        }
        F::Cx16 => {
            if !f.cx16 {
                return Err(format!("{id} requires CX16"));
            }
        }
        F::F16c => {
            if !(f.avx && f.f16c || f.avx512f && f.avx512vl) {
                return Err(
                    "half conversion requires AVX and F16C, or AVX512F and AVX512VL".into(),
                );
            }
            if id.has_f16c_rounding()
                && !args
                    .get(1)
                    .and_then(|&v| iconst(func, v))
                    .is_some_and(|n| (0..=7).contains(&n))
            {
                return Err("F16C rounding control must be iconst.i32 in0..=7".into());
            }
        }
        F::FlagsState => {
            if !f.lahf_sahf {
                return Err(format!("{id} requires LAHF/SAHF"));
            }
        }
        F::FpCompare => {
            if !f.avx {
                return Err(format!("{id} requires AVX"));
            }
            if !args
                .get(2)
                .and_then(|&v| iconst(func, v))
                .is_some_and(|n| (0..32).contains(&n))
            {
                return Err("AVX compare predicate must be iconst.i32 in0..=31".into());
            }
        }
        F::FpMisc => {
            if id.is_x86_fp_misc() {
                if !f.avx {
                    return Err(format!("{id} requires AVX"));
                }
                if id.has_x86_round_control()
                    && !args
                        .get(1)
                        .and_then(|&v| iconst(func, v))
                        .is_some_and(|n| (0..16).contains(&n))
                {
                    return Err("AVX round control must be iconst.i32 in0..=15".into());
                }
            }
            if id.is_x86_fp_horizontal_dot() {
                if !f.avx {
                    return Err(format!("{id} requires AVX"));
                }
                if id.has_x86_dot_control()
                    && !args
                        .get(2)
                        .and_then(|&v| iconst(func, v))
                        .is_some_and(|n| (0..256).contains(&n))
                {
                    return Err("AVX dot control must be iconst.i32 in0..=255".into());
                }
            }
        }
        F::Gather => {
            if !(f.avx && f.avx2) {
                return Err("AVX2 gather requires AVX and AVX2 f".into());
            }
            if !args
                .get(4)
                .and_then(|&v| iconst(func, v))
                .is_some_and(|n| matches!(n, 1 | 2 | 4 | 8))
            {
                return Err("AVX2 gather scale must be iconst.i32 in1,2,4,8".into());
            }
        }
        F::LegacyFpMisc => {
            if id.is_x86_legacy_fp_compare()
                && !args
                    .get(2)
                    .and_then(|&v| iconst(func, v))
                    .is_some_and(|n| (0..8).contains(&n))
            {
                return Err("legacy SSE compare predicate must be iconst.i32 in0..=7".into());
            }
            if id.is_x86_legacy_fp_v2() {
                let sse3 = id.info().name.starts_with("x86_sse3_");
                if !(if sse3 { f.sse3 } else { f.sse41 }) {
                    return Err(format!(
                        "{id} requires {}",
                        if sse3 { "SSE3" } else { "SSE4.1" }
                    ));
                }
                if let Some((index, limit)) = id.x86_legacy_fp_v2_control()
                    && !args
                        .get(index)
                        .and_then(|&v| iconst(func, v))
                        .is_some_and(|n| (0..limit).contains(&n))
                {
                    return Err(format!(
                        "legacy FP control must be iconst.i32 in0..{}",
                        limit - 1
                    ));
                }
            }
        }
        F::MaskedMem => {
            let avx2 = id.info().name.starts_with("x86_avx2_");
            if !f.avx || avx2 && !f.avx2 {
                return Err(format!("{id} requires AVX{}", if avx2 { "2" } else { "" }));
            }
        }
        F::PackedPair => {
            if let Some((_, tier)) = id.x86_packed_pair() {
                let supported = match tier {
                    0 => true,
                    1 => f.ssse3,
                    2 => f.sse41,
                    3 => f.avx,
                    4 => f.avx && f.avx2,
                    _ => unreachable!(),
                };
                if !supported {
                    return Err(format!("{id} requires its SSSE3/SSE4.1/AVX/AVX2 feature"));
                }
            }
        }
        F::PackedSearch => {
            if let Some((_, tier, sad)) = id.x86_packed_search() {
                if !match tier {
                    2 => f.sse41,
                    3 => f.avx,
                    4 => f.avx && f.avx2,
                    _ => false,
                } {
                    return Err(format!("{id} requires its SSE4.1/AVX/AVX2 feature"));
                }
                if sad
                    && !args
                        .get(2)
                        .and_then(|&v| iconst(func, v))
                        .is_some_and(|n| (0..256).contains(&n))
                {
                    return Err("MPSADBW requires iconst.i32 in0..255".into());
                }
            }
        }
        F::PackedShift => {
            if id.x86_byte_shift().is_some()
                && !args
                    .get(1)
                    .and_then(|&v| iconst(func, v))
                    .is_some_and(|n| (0..256).contains(&n))
            {
                return Err("packed byte shift requires iconst.i32 in0..255".into());
            }
            let tier = id
                .x86_packed_shift()
                .map(|(_, _, tier)| tier)
                .or_else(|| id.x86_byte_shift().map(|(_, tier)| tier))
                .expect("packed shift descriptor");
            if (tier == 1 && !f.avx) || (tier == 2 && !(f.avx && f.avx2)) {
                return Err(format!("{id} requires its AVX/AVX2 feature"));
            }
        }
        F::Pclmul => pclmul::validate(func, id, args, f)?,
        F::PortIo => {
            if let Some((_, _, immediate)) = id.x86_port_io() {
                if args.len() != id.info().params.len()
                    || args
                        .iter()
                        .zip(id.info().params)
                        .any(|(&v, &ty)| func.value_type(v) != ty)
                {
                    return Err("port I/O operand types or arity mismatch".into());
                }
                if immediate
                    && !args
                        .first()
                        .and_then(|&v| iconst(func, v))
                        .is_some_and(|n| (0..=255).contains(&n))
                {
                    return Err("port I/O immediate requires iconst.i16 in0..=255".into());
                }
            }
        }
        F::PureBits => {
            if id.is_x86_pure_bits() {
                let avx2 = id.info().name.starts_with("x86_avx2_");
                if !f.avx || avx2 && !f.avx2 {
                    return Err(format!("{id} requires AVX{}", if avx2 { "2" } else { "" }));
                }
            }
            if id.is_x86_legacy_ptest() && !f.sse41 {
                return Err(format!("{id} requires SSE4.1"));
            }
        }
        F::RawFma => {
            if !f.avx || !f.fma {
                return Err(format!("{id} requires AVX+FMA"));
            }
        }
        F::RawFpConvert => {
            if !f.avx {
                return Err(format!("{id} requires AVX"));
            }
        }
        F::RflagsSwap => {
            return Err(raw_rflags::SWAP_BLOCKER.into());
        }
        F::Sha256 => {
            if !f.sha {
                return Err(format!("{id} requires SHA"));
            }
            if args.len() != id.info().params.len()
                || args
                    .iter()
                    .zip(id.info().params)
                    .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
            {
                return Err(format!("{id} operand types or arity mismatch"));
            }
        }
        F::SignBlend => {
            let (_, tier) = id.x86_sign_blend().expect("sign blend descriptor");
            if !match tier {
                0 => f.sse41,
                1 => f.avx,
                2 => f.avx && f.avx2,
                _ => false,
            } {
                return Err(format!("{id} requires its SSE4.1/AVX/AVX2 feature"));
            }
        }
        F::String => {
            if let Some((opcode, index, _)) = id.x86_string_info() {
                // The VEX forms; the SSE4.2 forms are V2 opcodes.
                let avx = matches!(opcode, Opcode::V3(_));
                if !(if avx { f.avx } else { f.sse42 }) {
                    return Err(format!(
                        "{id} requires {}",
                        if avx { "AVX" } else { "SSE4.2" }
                    ));
                }
                if !args
                    .get(index)
                    .and_then(|&v| iconst(func, v))
                    .is_some_and(|n| (0..256).contains(&n))
                {
                    return Err("packed string control must be iconst.i32 in0..=255".into());
                }
            }
        }
        F::Vzeroall => {
            if !f.avx {
                return Err("VZEROALL requires AVX".into());
            }
        }
        F::X87Numerical => {
            if id.is_x86_x87_fisttp() && !f.sse3 {
                return Err(format!("{id} requires SSE3"));
            }
        }
        F::X87Region => {
            let control = iconst(func, args[1])
                .and_then(keel_ir::x87_region_control)
                .ok_or("x87 region control must be a valid iconst.i32")?;
            if control.0.opcode == op::FISTTP && !f.sse3 {
                return Err("x87 region FISTTP requires SSE3".into());
            }
        }
        F::Xstate => {
            if !(f.xsave && f.osxsave) {
                return Err(format!("{id} requires XSAVE and OSXSAVE"));
            }
        }
        F::DescriptorQuery
        | F::Fxstate
        | F::LegacyRawFpConvert
        | F::LegacyX87Image
        | F::MmxValueConvert
        | F::MxcsrAccess
        | F::OwnedX87Stack
        | F::RawDiv
        | F::RflagsRead
        | F::SelectorQuery
        | F::StringPortIo
        | F::System
        | F::X87State => {}
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn x86_intrinsic(
        &mut self,
        inst: Inst,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let family = id.x86_family().expect("an x86 intrinsic");
        match family {
            F::Aes => self.native_aes(id, args, result),
            F::Avx512BwConvert => self.native_avx512_bw_convert(id, args, result),
            F::Avx512BwCore => self.native_avx512_bw(id, args, result),
            F::Avx512BwMask => self.native_avx512_bw_mask(id, args, result),
            F::Avx512BwMaskAlu => self.native_avx512_bw_mask_alu(id, args, result),
            F::Avx512BwMisc => self.native_avx512_bw_misc(id, args, result),
            F::Avx512BwMul => self.native_avx512_bw_mul(id, args, result),
            F::Avx512BwPack => self.native_avx512_bw_pack(id, args, result),
            F::Avx512BwPermute => self.native_avx512_bw_permute(id, args, result),
            F::Avx512BwShift => self.native_avx512_bw_shift(id, args, result),
            F::Avx512Convert => self.native_avx512_convert(id, args, result),
            F::Avx512DqBridge => self.native_avx512_dq_bridge(id, args, result),
            F::Avx512DqConvert => self.native_avx512_dq_convert(id, args, result),
            F::Avx512DqLane => self.native_avx512_dq_lane(id, args, result),
            F::Avx512DqMaskAlu => self.native_avx512_dq_mask_alu(id, args, result),
            F::Avx512DqSpecial => self.native_avx512_dq_special(id, args, result),
            F::Avx512Fma => self.native_avx512_fma(id, args, result),
            F::Avx512Indexed => self.native_avx512_indexed(id, args, result),
            F::Avx512Integer => self.native_avx512_integer_intrinsic(id, args, result),
            F::Avx512IntegerConvert => self.native_avx512_integer_convert(id, args, result),
            F::Avx512Lane => self.native_avx512_lane(id, args, result),
            F::Avx512Mask => self.native_avx512_mask(id, args, result),
            F::Avx512MaskAlu => self.native_avx512_mask_alu(id, args, result),
            F::Avx512Pack => self.native_avx512_pack(id, args, result),
            F::Avx512Packed => self.native_avx512_packed(id, args, result),
            F::Avx512Permute => self.native_avx512_permute(id, args, result),
            F::Avx512Scalar => self.native_avx512_scalar(id, args, result),
            F::Avx512ScalarConvert => self.native_avx512_scalar_convert(id, args, result),
            F::Avx512ScalarFma => self.native_avx512_scalar_fma(id, args, result),
            F::Avx512ScalarMove => self.native_avx512_scalar_move(id, args, result),
            F::Avx512Special => self.native_avx512_special(id, args, result),
            F::Avx512VlConvert => self.native_avx512_vl_convert(id, args, result),
            F::Avx512VlInteger => self.native_avx512_vl_integer(id, args, result),
            F::Avx512VlLane => self.native_avx512_vl_lane(id, args, result),
            F::Avx512VlNarrow => self.native_avx512_vl_narrow(id, args, result),
            F::Avx512VlShift => self.native_avx512_vl_shift(id, args, result),
            F::Avx512VlWiden => self.native_avx512_vl_widen(id, args, result),
            F::Bmi => self.native_bmi(id, args, result),
            F::ByteMaskStore => self.native_byte_mask_store(id, args),
            F::ByteShuffle => self.native_byte_shuffle(id, args, result),
            F::CacheStream => self.native_cache_stream(id, args, result),
            F::CpuControl => self.native_cpu_control(id, args, result),
            F::Crc32 => self.native_crc32(args, result),
            F::Cx16 => self.native_cx16(args, result),
            F::DescriptorQuery => self.native_descriptor_query(id, args, result),
            F::F16c => self.native_f16c(id, args, result),
            F::FlagsState => self.native_flags_state(id, args, result),
            F::FpCompare => self.native_fp_compare(id, args, result),
            F::FpMisc => self.native_fp_misc(id, args, result),
            F::Fxstate => self.native_fxstate(id, args),
            F::Gather => self.native_gather(id, args, result),
            F::LegacyFpMisc => self.native_legacy_fp_misc(id, args, result),
            F::LegacyRawFpConvert => self.native_legacy_raw_fp_convert(id, args, result),
            F::LegacyX87Image => self.native_legacy_x87_image(id, args),
            F::MaskedMem => self.native_masked_mem(id, args, result),
            F::MmxValueConvert => self.native_mmx_value_convert(id, args, result),
            F::MxcsrAccess => self.native_mxcsr_access(id, args, result),
            F::OwnedX87Stack => self.native_owned_x87_stack(id),
            F::PackedPair => self.native_packed_pair(id, args, result),
            F::PackedSearch => self.native_packed_search(id, args, result),
            F::PackedShift => self.native_packed_shift(id, args, result),
            F::Pclmul => self.native_pclmul(id, args, result),
            F::PortIo => self.native_port_io(id, args, result),
            F::PureBits => self.native_pure_bits(id, args, result),
            F::RawDiv => self.native_raw_div(inst, id, args),
            F::RawFma => self.native_raw_fma(id, args, result),
            F::RawFpConvert => self.native_raw_fp_convert(id, args, result),
            F::RflagsRead => self.native_rflags_read(result),
            F::RflagsSwap => unreachable!("validation rejects {id}"),
            F::SelectorQuery => self.native_selector_query(inst, id, args),
            F::Sha256 => self.native_sha256(id, args, result),
            F::SignBlend => self.native_sign_blend(id, args, result),
            F::String => self.native_string(id, args, result),
            F::StringPortIo => self.native_string_port_io(inst, id, args),
            F::System => self.native_generated_system(id, args, result),
            F::Vzeroall => {
                self.emit(op::VZEROALL, []);
            }
            F::X87Numerical => self.native_x87_numerical(inst, id, args),
            F::X87Region => self.native_x87_region(inst, args),
            F::X87State => self.native_x87_state(id, args, result),
            F::Xstate => self.native_xstate(id, args),
        }
    }

    /// Lowers an intrinsic: an x86 one by its family, the others here.
    pub(super) fn intrinsic(
        &mut self,
        inst: Inst,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        if id.is_x86_target() {
            return self.x86_intrinsic(inst, id, args, result);
        }
        match id {
            keel_ir::x86_intrinsics!() => unreachable!("dispatched x86 intrinsic"),
            Intrinsic::Aarch64Sha256h
            | Intrinsic::Aarch64Sha256h2
            | Intrinsic::Aarch64Sha256su0
            | Intrinsic::Aarch64Sha256su1 => unreachable!("rejected AArch64 intrinsic"),
            // The libc functions take the same arguments in the same
            // order. memset's `int c` only uses its low byte, so the
            // unspecified upper bits of the i8 are harmless.
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp => {
                let sig = keel_ir::Signature {
                    params: id.info().params.iter().map(|&t| AbiParam::new(t)).collect(),
                    returns: id
                        .info()
                        .returns
                        .iter()
                        .map(|&t| AbiParam::new(t))
                        .collect(),
                    cc: CallConv::C,
                    x86_vector_abi: None,
                    variadic: false,
                    returns_twice: false,
                };
                self.move_args(&args.to_vec().into(), &sig, None, false);
                self.emit(
                    Opcode::Call,
                    [asm_arg(symbol_address(&id.to_string(), Modifier::Plt))],
                );
                if id == Intrinsic::Memcmp {
                    self.call_result(result, None);
                }
            }
            // What clang emits for __builtin_debugtrap; a debugger can
            // continue past it.
            Intrinsic::Debugtrap => self.emit(Opcode::Int3, []),
            // Round up to keep rsp 16-byte aligned; the memory starts
            // above the outgoing arguments, which stay at rsp.
            Intrinsic::StackAlloc => {
                let size = self.use_reg(args[0], S0);
                self.emit(
                    Opcode::Lea,
                    [
                        asm_arg(r64(0)),
                        asm_arg(memory(r64(size), (self.frame_align - 1) as i128)),
                    ],
                );
                self.emit(
                    Opcode::And,
                    [asm_arg(r64(0)), asm_arg(-(self.frame_align as i128))],
                );
                self.emit(Opcode::Sub, [asm_arg(r64(4)), asm_arg(r64(0))]);
                let d = self.def_reg(result);
                self.emit(
                    Opcode::Lea,
                    [
                        asm_arg(r64(d)),
                        asm_arg(memory(r64(4), (self.outgoing) as i128)),
                    ],
                );
                self.def_done(result, d);
            }
            Intrinsic::StackSave => {
                let d = self.def_reg(result);
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(4))]);
                self.def_done(result, d);
            }
            // Lifetime markers constrain IR transformations but change no runtime state.
            Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd => {}
            Intrinsic::StackRestore => {
                let mark = self.use_reg(args[0], S0);
                self.emit(Opcode::Mov, [asm_arg(r64(4)), asm_arg(r64(mark))]);
            }
            // Where the prologue points rbp: the saved rbp, with the
            // return address above it.
            Intrinsic::FrameAddress => {
                let d = self.def_reg(result);
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(5))]);
                self.def_done(result, d);
            }
            // SysV's va_list: the offsets of the next general and vector
            // register in the save area (vector registers are 16 bytes,
            // after the 48 of the general ones), the next stack argument
            // and the save area.
            Intrinsic::VaStart => {
                let p = self.use_reg(args[0], S1);
                let p = r64(p);
                let (ints, floats) = (self.named.ints, self.named.floats);
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(sized(memory(p, 0), 32)),
                        asm_arg(8 * u32::from(ints)),
                    ],
                );
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(sized(memory(p, 4i128), 32)),
                        asm_arg(48 + 16 * u32::from(floats)),
                    ],
                );
                self.emit(
                    Opcode::Lea,
                    [
                        asm_arg(r64(0)),
                        asm_arg(memory(r64(5), (16 + self.named.stack) as i128)),
                    ],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(memory(p, 8i128), 64)), asm_arg(r64(0))],
                );
                let save = self.frame(self.reg_save.expect("a variadic function"));
                self.emit(Opcode::Lea, [asm_arg(r64(0)), asm_arg(save)]);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(memory(p, 16i128), 64)), asm_arg(r64(0))],
                );
            }
        }
    }
}

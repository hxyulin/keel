//! Application cache maintenance, hints and non-temporal memory operations.
//! CLFLUSH is a faulting cache-line writeback/invalidate at the exact address;
//! unlike prefetch it follows byte-read protection checks. Requires independent
//! CPUID.CLFSH. Hardware ordering differs on Intel/AMD; see
//! docs/targets/x86_64/system/README.md. No implicit fence is inserted.
//!
//! All are pinned IR barriers (including unused hints/loads), ordering compiler
//! memory accesses without adding a hardware fence. Callers use x86_sfence for
//! streaming-store publication as required by their synchronization protocol.
//! Stores/loads access exactly their typed width; vector addresses must be
//! 16/32-byte aligned. MOVNTI allows unaligned addresses (ordinary architectural
//! alignment checking still applies). Hardware protection/alignment faults are
//! preserved, with no masking, widening, read-modify-write or fallback.
//!
//! Prefetch requires a valid pointer into a live readable allocation. It returns
//! no data, performs no normal observable user load, and promises neither cache
//! placement nor fault suppression. The m8 encoding identifies an address, not a
//! one-byte load or a specified cache-line footprint. Prefetch/stream policy is advisory,
//! including MOVNTDQA's memory-type-dependent behavior; these are not MMIO APIs.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_cache_stream(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let name = id.info().name;
        let address_only = name.starts_with("x86_prefetch") || id == Intrinsic::X86Clflush;
        let load = name.contains("movntdqa");
        let opcode = if address_only {
            &name[4..]
        } else if name.starts_with("x86_movnti") {
            "movnti"
        } else {
            name.rsplit('_')
                .next()
                .unwrap()
                .trim_end_matches(|c: char| c.is_ascii_digit())
        };
        let opcode = Opcode::parse(opcode).expect("audited cache/stream opcode");
        // Reserved SIMD/GPR fallbacks are independent of the address scratch.
        // Capture the stored value before obtaining the address, then publish a
        // load result only after its address has been consumed.
        let ty = if load {
            id.info().returns[0]
        } else if address_only {
            Type::I8
        } else {
            self.ty(args[1])
        };
        let source = if !address_only && !load {
            Some(if ty.is_vector() {
                let r = self.use_freg(args[1], FS0);
                simd(r, ty)
            } else {
                let r = self.use_reg(args[1], S0);
                gpr(r, ptr(ty))
            })
        } else {
            None
        };
        let base = self.use_reg(args[0], S1);
        let at = sized(memory(r64(base), 0), ptr(ty));
        if address_only {
            self.emit(opcode, [asm_arg(at)]);
        } else if load {
            let d = self.def_freg(result);
            self.emit(opcode, [asm_arg(simd(d, ty)), asm_arg(at)]);
            self.def_done(result, d);
        } else {
            self.emit(opcode, [asm_arg(at), asm_arg(source.unwrap())]);
        }
    }
}

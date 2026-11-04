//! Legal packed VL permutation forms retain independent merge and index types.
use super::{Intrinsic, Type, VectorType, X86Avx512PermuteKind};
use keel_asm_ir::x86::EvexMnemonic;
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlPermute {
    pub name: &'static str,
    pub opcode: EvexMnemonic,
    pub kind: X86Avx512PermuteKind,
    pub vector: VectorType,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// Legal AVX512F plus AVX512VL register permutations at their exact widths.
    pub fn x86_avx512_vl_permute(self) -> Option<X86Avx512VlPermute> {
        macro_rules! descriptor {
            ($kind:ident, $name:literal, $vector:ident, $index:ident, $opcode:ident) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const I: Type = Type::Vector(VectorType::$index);
                use X86Avx512PermuteKind::*;
                X86Avx512VlPermute {
                    name: $name,
                    opcode: EvexMnemonic::$opcode,
                    kind: $kind,
                    vector: VectorType::$vector,
                    params: match $kind {
                        Full | Local => &[V, Type::I8, V, I],
                        TwoIndex | TwoTable => &[V, Type::I8, V, I, V],
                        Immediate => &[V, Type::I8, V, Type::I32],
                        PairImmediate => &[V, Type::I8, V, V, Type::I32],
                        Pair => &[V, Type::I8, V, V],
                        Unary => &[V, Type::I8, V],
                    },
                    returns: &[V],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlVpermi2d128 => {
                descriptor!(TwoIndex, "x86_avx512vl_vpermi2d128", I32x4, I32x4, Vpermi2d)
            }
            Self::X86Avx512VlVpermi2q128 => {
                descriptor!(TwoIndex, "x86_avx512vl_vpermi2q128", I64x2, I64x2, Vpermi2q)
            }
            Self::X86Avx512VlVpermi2ps128 => descriptor!(
                TwoIndex,
                "x86_avx512vl_vpermi2ps128",
                F32x4,
                I32x4,
                Vpermi2ps
            ),
            Self::X86Avx512VlVpermi2pd128 => descriptor!(
                TwoIndex,
                "x86_avx512vl_vpermi2pd128",
                F64x2,
                I64x2,
                Vpermi2pd
            ),
            Self::X86Avx512VlVpermt2d128 => {
                descriptor!(TwoTable, "x86_avx512vl_vpermt2d128", I32x4, I32x4, Vpermt2d)
            }
            Self::X86Avx512VlVpermt2q128 => {
                descriptor!(TwoTable, "x86_avx512vl_vpermt2q128", I64x2, I64x2, Vpermt2q)
            }
            Self::X86Avx512VlVpermt2ps128 => descriptor!(
                TwoTable,
                "x86_avx512vl_vpermt2ps128",
                F32x4,
                I32x4,
                Vpermt2ps
            ),
            Self::X86Avx512VlVpermt2pd128 => descriptor!(
                TwoTable,
                "x86_avx512vl_vpermt2pd128",
                F64x2,
                I64x2,
                Vpermt2pd
            ),
            Self::X86Avx512VlVpermilps128 => {
                descriptor!(Local, "x86_avx512vl_vpermilps128", F32x4, I32x4, Vpermilps)
            }
            Self::X86Avx512VlVpermilpd128 => {
                descriptor!(Local, "x86_avx512vl_vpermilpd128", F64x2, I64x2, Vpermilpd)
            }
            Self::X86Avx512VlVpermilpsImm128 => descriptor!(
                Immediate,
                "x86_avx512vl_vpermilpsimm128",
                F32x4,
                I32x4,
                Vpermilps
            ),
            Self::X86Avx512VlVpermilpdImm128 => descriptor!(
                Immediate,
                "x86_avx512vl_vpermilpdimm128",
                F64x2,
                I64x2,
                Vpermilpd
            ),
            Self::X86Avx512VlVshufps128 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufps128",
                F32x4,
                I32x4,
                Vshufps
            ),
            Self::X86Avx512VlVshufpd128 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufpd128",
                F64x2,
                I64x2,
                Vshufpd
            ),
            Self::X86Avx512VlVpshufd128 => {
                descriptor!(Immediate, "x86_avx512vl_vpshufd128", I32x4, I32x4, Vpshufd)
            }
            Self::X86Avx512VlValignd128 => descriptor!(
                PairImmediate,
                "x86_avx512vl_valignd128",
                I32x4,
                I32x4,
                Valignd
            ),
            Self::X86Avx512VlValignq128 => descriptor!(
                PairImmediate,
                "x86_avx512vl_valignq128",
                I64x2,
                I64x2,
                Valignq
            ),
            Self::X86Avx512VlVpunpckldq128 => {
                descriptor!(Pair, "x86_avx512vl_vpunpckldq128", I32x4, I32x4, Vpunpckldq)
            }
            Self::X86Avx512VlVpunpcklqdq128 => descriptor!(
                Pair,
                "x86_avx512vl_vpunpcklqdq128",
                I64x2,
                I64x2,
                Vpunpcklqdq
            ),
            Self::X86Avx512VlVunpcklps128 => {
                descriptor!(Pair, "x86_avx512vl_vunpcklps128", F32x4, I32x4, Vunpcklps)
            }
            Self::X86Avx512VlVunpcklpd128 => {
                descriptor!(Pair, "x86_avx512vl_vunpcklpd128", F64x2, I64x2, Vunpcklpd)
            }
            Self::X86Avx512VlVpunpckhdq128 => {
                descriptor!(Pair, "x86_avx512vl_vpunpckhdq128", I32x4, I32x4, Vpunpckhdq)
            }
            Self::X86Avx512VlVpunpckhqdq128 => descriptor!(
                Pair,
                "x86_avx512vl_vpunpckhqdq128",
                I64x2,
                I64x2,
                Vpunpckhqdq
            ),
            Self::X86Avx512VlVunpckhps128 => {
                descriptor!(Pair, "x86_avx512vl_vunpckhps128", F32x4, I32x4, Vunpckhps)
            }
            Self::X86Avx512VlVunpckhpd128 => {
                descriptor!(Pair, "x86_avx512vl_vunpckhpd128", F64x2, I64x2, Vunpckhpd)
            }
            Self::X86Avx512VlVmovshdup128 => {
                descriptor!(Unary, "x86_avx512vl_vmovshdup128", F32x4, I32x4, Vmovshdup)
            }
            Self::X86Avx512VlVmovsldup128 => {
                descriptor!(Unary, "x86_avx512vl_vmovsldup128", F32x4, I32x4, Vmovsldup)
            }
            Self::X86Avx512VlVmovddup128 => {
                descriptor!(Unary, "x86_avx512vl_vmovddup128", F64x2, I64x2, Vmovddup)
            }
            Self::X86Avx512VlVpermd256 => {
                descriptor!(Full, "x86_avx512vl_vpermd256", I32x8, I32x8, Vpermd)
            }
            Self::X86Avx512VlVpermq256 => {
                descriptor!(Full, "x86_avx512vl_vpermq256", I64x4, I64x4, Vpermq)
            }
            Self::X86Avx512VlVpermps256 => {
                descriptor!(Full, "x86_avx512vl_vpermps256", F32x8, I32x8, Vpermps)
            }
            Self::X86Avx512VlVpermpd256 => {
                descriptor!(Full, "x86_avx512vl_vpermpd256", F64x4, I64x4, Vpermpd)
            }
            Self::X86Avx512VlVpermi2d256 => {
                descriptor!(TwoIndex, "x86_avx512vl_vpermi2d256", I32x8, I32x8, Vpermi2d)
            }
            Self::X86Avx512VlVpermi2q256 => {
                descriptor!(TwoIndex, "x86_avx512vl_vpermi2q256", I64x4, I64x4, Vpermi2q)
            }
            Self::X86Avx512VlVpermi2ps256 => descriptor!(
                TwoIndex,
                "x86_avx512vl_vpermi2ps256",
                F32x8,
                I32x8,
                Vpermi2ps
            ),
            Self::X86Avx512VlVpermi2pd256 => descriptor!(
                TwoIndex,
                "x86_avx512vl_vpermi2pd256",
                F64x4,
                I64x4,
                Vpermi2pd
            ),
            Self::X86Avx512VlVpermt2d256 => {
                descriptor!(TwoTable, "x86_avx512vl_vpermt2d256", I32x8, I32x8, Vpermt2d)
            }
            Self::X86Avx512VlVpermt2q256 => {
                descriptor!(TwoTable, "x86_avx512vl_vpermt2q256", I64x4, I64x4, Vpermt2q)
            }
            Self::X86Avx512VlVpermt2ps256 => descriptor!(
                TwoTable,
                "x86_avx512vl_vpermt2ps256",
                F32x8,
                I32x8,
                Vpermt2ps
            ),
            Self::X86Avx512VlVpermt2pd256 => descriptor!(
                TwoTable,
                "x86_avx512vl_vpermt2pd256",
                F64x4,
                I64x4,
                Vpermt2pd
            ),
            Self::X86Avx512VlVpermilps256 => {
                descriptor!(Local, "x86_avx512vl_vpermilps256", F32x8, I32x8, Vpermilps)
            }
            Self::X86Avx512VlVpermilpd256 => {
                descriptor!(Local, "x86_avx512vl_vpermilpd256", F64x4, I64x4, Vpermilpd)
            }
            Self::X86Avx512VlVpermilpsImm256 => descriptor!(
                Immediate,
                "x86_avx512vl_vpermilpsimm256",
                F32x8,
                I32x8,
                Vpermilps
            ),
            Self::X86Avx512VlVpermilpdImm256 => descriptor!(
                Immediate,
                "x86_avx512vl_vpermilpdimm256",
                F64x4,
                I64x4,
                Vpermilpd
            ),
            Self::X86Avx512VlVpermqImm256 => {
                descriptor!(Immediate, "x86_avx512vl_vpermqimm256", I64x4, I64x4, Vpermq)
            }
            Self::X86Avx512VlVpermpdImm256 => descriptor!(
                Immediate,
                "x86_avx512vl_vpermpdimm256",
                F64x4,
                I64x4,
                Vpermpd
            ),
            Self::X86Avx512VlVshufps256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufps256",
                F32x8,
                I32x8,
                Vshufps
            ),
            Self::X86Avx512VlVshufpd256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufpd256",
                F64x4,
                I64x4,
                Vshufpd
            ),
            Self::X86Avx512VlVpshufd256 => {
                descriptor!(Immediate, "x86_avx512vl_vpshufd256", I32x8, I32x8, Vpshufd)
            }
            Self::X86Avx512VlVshufi32x4256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufi32x4256",
                I32x8,
                I32x8,
                Vshufi32x4
            ),
            Self::X86Avx512VlVshufi64x2256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshufi64x2256",
                I64x4,
                I64x4,
                Vshufi64x2
            ),
            Self::X86Avx512VlVshuff32x4256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshuff32x4256",
                F32x8,
                I32x8,
                Vshuff32x4
            ),
            Self::X86Avx512VlVshuff64x2256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_vshuff64x2256",
                F64x4,
                I64x4,
                Vshuff64x2
            ),
            Self::X86Avx512VlValignd256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_valignd256",
                I32x8,
                I32x8,
                Valignd
            ),
            Self::X86Avx512VlValignq256 => descriptor!(
                PairImmediate,
                "x86_avx512vl_valignq256",
                I64x4,
                I64x4,
                Valignq
            ),
            Self::X86Avx512VlVpunpckldq256 => {
                descriptor!(Pair, "x86_avx512vl_vpunpckldq256", I32x8, I32x8, Vpunpckldq)
            }
            Self::X86Avx512VlVpunpcklqdq256 => descriptor!(
                Pair,
                "x86_avx512vl_vpunpcklqdq256",
                I64x4,
                I64x4,
                Vpunpcklqdq
            ),
            Self::X86Avx512VlVunpcklps256 => {
                descriptor!(Pair, "x86_avx512vl_vunpcklps256", F32x8, I32x8, Vunpcklps)
            }
            Self::X86Avx512VlVunpcklpd256 => {
                descriptor!(Pair, "x86_avx512vl_vunpcklpd256", F64x4, I64x4, Vunpcklpd)
            }
            Self::X86Avx512VlVpunpckhdq256 => {
                descriptor!(Pair, "x86_avx512vl_vpunpckhdq256", I32x8, I32x8, Vpunpckhdq)
            }
            Self::X86Avx512VlVpunpckhqdq256 => descriptor!(
                Pair,
                "x86_avx512vl_vpunpckhqdq256",
                I64x4,
                I64x4,
                Vpunpckhqdq
            ),
            Self::X86Avx512VlVunpckhps256 => {
                descriptor!(Pair, "x86_avx512vl_vunpckhps256", F32x8, I32x8, Vunpckhps)
            }
            Self::X86Avx512VlVunpckhpd256 => {
                descriptor!(Pair, "x86_avx512vl_vunpckhpd256", F64x4, I64x4, Vunpckhpd)
            }
            Self::X86Avx512VlVmovshdup256 => {
                descriptor!(Unary, "x86_avx512vl_vmovshdup256", F32x8, I32x8, Vmovshdup)
            }
            Self::X86Avx512VlVmovsldup256 => {
                descriptor!(Unary, "x86_avx512vl_vmovsldup256", F32x8, I32x8, Vmovsldup)
            }
            Self::X86Avx512VlVmovddup256 => {
                descriptor!(Unary, "x86_avx512vl_vmovddup256", F64x4, I64x4, Vmovddup)
            }
            _ => return None,
        })
    }
}

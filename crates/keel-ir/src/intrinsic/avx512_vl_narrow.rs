//! Narrowing conversions retain encoded source width and zero unused result bytes.
use super::{Intrinsic, Type, VectorType};
use keel_asm_ir::x86::EvexMnemonic;
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlNarrow {
    pub name: &'static str,
    pub encoded_bits: u16,
    pub source: VectorType,
    pub result: VectorType,
    pub store: bool,
    pub memory_bytes: u16,
    pub opcode: EvexMnemonic,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_vl_narrow(self) -> Option<X86Avx512VlNarrow> {
        macro_rules! row {
            ($name:literal,$width:literal,$source:ident,$result:ident,$store:literal,$bytes:literal,$opcode:ident) => {{
                const S: Type = Type::Vector(VectorType::$source);
                const R: Type = Type::Vector(VectorType::$result);
                X86Avx512VlNarrow {
                    name: $name,
                    encoded_bits: $width,
                    source: VectorType::$source,
                    result: VectorType::$result,
                    store: $store,
                    memory_bytes: $bytes,
                    opcode: EvexMnemonic::$opcode,
                    params: if $store {
                        &[Type::Ptr, S, Type::I8]
                    } else {
                        &[R, Type::I8, S]
                    },
                    returns: if $store { &[] } else { &[R] },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlPmovdb128 => row!(
                "x86_avx512vl_pmovdb128",
                128,
                I32x4,
                I8x16,
                false,
                4,
                Vpmovdb
            ),
            Self::X86Avx512VlPmovdbStore128 => row!(
                "x86_avx512vl_pmovdbstore128",
                128,
                I32x4,
                I8x16,
                true,
                4,
                Vpmovdb
            ),
            Self::X86Avx512VlPmovdw128 => row!(
                "x86_avx512vl_pmovdw128",
                128,
                I32x4,
                I16x8,
                false,
                8,
                Vpmovdw
            ),
            Self::X86Avx512VlPmovdwStore128 => row!(
                "x86_avx512vl_pmovdwstore128",
                128,
                I32x4,
                I16x8,
                true,
                8,
                Vpmovdw
            ),
            Self::X86Avx512VlPmovqb128 => row!(
                "x86_avx512vl_pmovqb128",
                128,
                I64x2,
                I8x16,
                false,
                2,
                Vpmovqb
            ),
            Self::X86Avx512VlPmovqbStore128 => row!(
                "x86_avx512vl_pmovqbstore128",
                128,
                I64x2,
                I8x16,
                true,
                2,
                Vpmovqb
            ),
            Self::X86Avx512VlPmovqw128 => row!(
                "x86_avx512vl_pmovqw128",
                128,
                I64x2,
                I16x8,
                false,
                4,
                Vpmovqw
            ),
            Self::X86Avx512VlPmovqwStore128 => row!(
                "x86_avx512vl_pmovqwstore128",
                128,
                I64x2,
                I16x8,
                true,
                4,
                Vpmovqw
            ),
            Self::X86Avx512VlPmovqd128 => row!(
                "x86_avx512vl_pmovqd128",
                128,
                I64x2,
                I32x4,
                false,
                8,
                Vpmovqd
            ),
            Self::X86Avx512VlPmovqdStore128 => row!(
                "x86_avx512vl_pmovqdstore128",
                128,
                I64x2,
                I32x4,
                true,
                8,
                Vpmovqd
            ),
            Self::X86Avx512VlPmovsdb128 => row!(
                "x86_avx512vl_pmovsdb128",
                128,
                I32x4,
                I8x16,
                false,
                4,
                Vpmovsdb
            ),
            Self::X86Avx512VlPmovsdbStore128 => row!(
                "x86_avx512vl_pmovsdbstore128",
                128,
                I32x4,
                I8x16,
                true,
                4,
                Vpmovsdb
            ),
            Self::X86Avx512VlPmovsdw128 => row!(
                "x86_avx512vl_pmovsdw128",
                128,
                I32x4,
                I16x8,
                false,
                8,
                Vpmovsdw
            ),
            Self::X86Avx512VlPmovsdwStore128 => row!(
                "x86_avx512vl_pmovsdwstore128",
                128,
                I32x4,
                I16x8,
                true,
                8,
                Vpmovsdw
            ),
            Self::X86Avx512VlPmovsqb128 => row!(
                "x86_avx512vl_pmovsqb128",
                128,
                I64x2,
                I8x16,
                false,
                2,
                Vpmovsqb
            ),
            Self::X86Avx512VlPmovsqbStore128 => row!(
                "x86_avx512vl_pmovsqbstore128",
                128,
                I64x2,
                I8x16,
                true,
                2,
                Vpmovsqb
            ),
            Self::X86Avx512VlPmovsqw128 => row!(
                "x86_avx512vl_pmovsqw128",
                128,
                I64x2,
                I16x8,
                false,
                4,
                Vpmovsqw
            ),
            Self::X86Avx512VlPmovsqwStore128 => row!(
                "x86_avx512vl_pmovsqwstore128",
                128,
                I64x2,
                I16x8,
                true,
                4,
                Vpmovsqw
            ),
            Self::X86Avx512VlPmovsqd128 => row!(
                "x86_avx512vl_pmovsqd128",
                128,
                I64x2,
                I32x4,
                false,
                8,
                Vpmovsqd
            ),
            Self::X86Avx512VlPmovsqdStore128 => row!(
                "x86_avx512vl_pmovsqdstore128",
                128,
                I64x2,
                I32x4,
                true,
                8,
                Vpmovsqd
            ),
            Self::X86Avx512VlPmovusdb128 => row!(
                "x86_avx512vl_pmovusdb128",
                128,
                I32x4,
                I8x16,
                false,
                4,
                Vpmovusdb
            ),
            Self::X86Avx512VlPmovusdbStore128 => row!(
                "x86_avx512vl_pmovusdbstore128",
                128,
                I32x4,
                I8x16,
                true,
                4,
                Vpmovusdb
            ),
            Self::X86Avx512VlPmovusdw128 => row!(
                "x86_avx512vl_pmovusdw128",
                128,
                I32x4,
                I16x8,
                false,
                8,
                Vpmovusdw
            ),
            Self::X86Avx512VlPmovusdwStore128 => row!(
                "x86_avx512vl_pmovusdwstore128",
                128,
                I32x4,
                I16x8,
                true,
                8,
                Vpmovusdw
            ),
            Self::X86Avx512VlPmovusqb128 => row!(
                "x86_avx512vl_pmovusqb128",
                128,
                I64x2,
                I8x16,
                false,
                2,
                Vpmovusqb
            ),
            Self::X86Avx512VlPmovusqbStore128 => row!(
                "x86_avx512vl_pmovusqbstore128",
                128,
                I64x2,
                I8x16,
                true,
                2,
                Vpmovusqb
            ),
            Self::X86Avx512VlPmovusqw128 => row!(
                "x86_avx512vl_pmovusqw128",
                128,
                I64x2,
                I16x8,
                false,
                4,
                Vpmovusqw
            ),
            Self::X86Avx512VlPmovusqwStore128 => row!(
                "x86_avx512vl_pmovusqwstore128",
                128,
                I64x2,
                I16x8,
                true,
                4,
                Vpmovusqw
            ),
            Self::X86Avx512VlPmovusqd128 => row!(
                "x86_avx512vl_pmovusqd128",
                128,
                I64x2,
                I32x4,
                false,
                8,
                Vpmovusqd
            ),
            Self::X86Avx512VlPmovusqdStore128 => row!(
                "x86_avx512vl_pmovusqdstore128",
                128,
                I64x2,
                I32x4,
                true,
                8,
                Vpmovusqd
            ),
            Self::X86Avx512VlPmovdb256 => row!(
                "x86_avx512vl_pmovdb256",
                256,
                I32x8,
                I8x16,
                false,
                8,
                Vpmovdb
            ),
            Self::X86Avx512VlPmovdbStore256 => row!(
                "x86_avx512vl_pmovdbstore256",
                256,
                I32x8,
                I8x16,
                true,
                8,
                Vpmovdb
            ),
            Self::X86Avx512VlPmovdw256 => row!(
                "x86_avx512vl_pmovdw256",
                256,
                I32x8,
                I16x8,
                false,
                16,
                Vpmovdw
            ),
            Self::X86Avx512VlPmovdwStore256 => row!(
                "x86_avx512vl_pmovdwstore256",
                256,
                I32x8,
                I16x8,
                true,
                16,
                Vpmovdw
            ),
            Self::X86Avx512VlPmovqb256 => row!(
                "x86_avx512vl_pmovqb256",
                256,
                I64x4,
                I8x16,
                false,
                4,
                Vpmovqb
            ),
            Self::X86Avx512VlPmovqbStore256 => row!(
                "x86_avx512vl_pmovqbstore256",
                256,
                I64x4,
                I8x16,
                true,
                4,
                Vpmovqb
            ),
            Self::X86Avx512VlPmovqw256 => row!(
                "x86_avx512vl_pmovqw256",
                256,
                I64x4,
                I16x8,
                false,
                8,
                Vpmovqw
            ),
            Self::X86Avx512VlPmovqwStore256 => row!(
                "x86_avx512vl_pmovqwstore256",
                256,
                I64x4,
                I16x8,
                true,
                8,
                Vpmovqw
            ),
            Self::X86Avx512VlPmovqd256 => row!(
                "x86_avx512vl_pmovqd256",
                256,
                I64x4,
                I32x4,
                false,
                16,
                Vpmovqd
            ),
            Self::X86Avx512VlPmovqdStore256 => row!(
                "x86_avx512vl_pmovqdstore256",
                256,
                I64x4,
                I32x4,
                true,
                16,
                Vpmovqd
            ),
            Self::X86Avx512VlPmovsdb256 => row!(
                "x86_avx512vl_pmovsdb256",
                256,
                I32x8,
                I8x16,
                false,
                8,
                Vpmovsdb
            ),
            Self::X86Avx512VlPmovsdbStore256 => row!(
                "x86_avx512vl_pmovsdbstore256",
                256,
                I32x8,
                I8x16,
                true,
                8,
                Vpmovsdb
            ),
            Self::X86Avx512VlPmovsdw256 => row!(
                "x86_avx512vl_pmovsdw256",
                256,
                I32x8,
                I16x8,
                false,
                16,
                Vpmovsdw
            ),
            Self::X86Avx512VlPmovsdwStore256 => row!(
                "x86_avx512vl_pmovsdwstore256",
                256,
                I32x8,
                I16x8,
                true,
                16,
                Vpmovsdw
            ),
            Self::X86Avx512VlPmovsqb256 => row!(
                "x86_avx512vl_pmovsqb256",
                256,
                I64x4,
                I8x16,
                false,
                4,
                Vpmovsqb
            ),
            Self::X86Avx512VlPmovsqbStore256 => row!(
                "x86_avx512vl_pmovsqbstore256",
                256,
                I64x4,
                I8x16,
                true,
                4,
                Vpmovsqb
            ),
            Self::X86Avx512VlPmovsqw256 => row!(
                "x86_avx512vl_pmovsqw256",
                256,
                I64x4,
                I16x8,
                false,
                8,
                Vpmovsqw
            ),
            Self::X86Avx512VlPmovsqwStore256 => row!(
                "x86_avx512vl_pmovsqwstore256",
                256,
                I64x4,
                I16x8,
                true,
                8,
                Vpmovsqw
            ),
            Self::X86Avx512VlPmovsqd256 => row!(
                "x86_avx512vl_pmovsqd256",
                256,
                I64x4,
                I32x4,
                false,
                16,
                Vpmovsqd
            ),
            Self::X86Avx512VlPmovsqdStore256 => row!(
                "x86_avx512vl_pmovsqdstore256",
                256,
                I64x4,
                I32x4,
                true,
                16,
                Vpmovsqd
            ),
            Self::X86Avx512VlPmovusdb256 => row!(
                "x86_avx512vl_pmovusdb256",
                256,
                I32x8,
                I8x16,
                false,
                8,
                Vpmovusdb
            ),
            Self::X86Avx512VlPmovusdbStore256 => row!(
                "x86_avx512vl_pmovusdbstore256",
                256,
                I32x8,
                I8x16,
                true,
                8,
                Vpmovusdb
            ),
            Self::X86Avx512VlPmovusdw256 => row!(
                "x86_avx512vl_pmovusdw256",
                256,
                I32x8,
                I16x8,
                false,
                16,
                Vpmovusdw
            ),
            Self::X86Avx512VlPmovusdwStore256 => row!(
                "x86_avx512vl_pmovusdwstore256",
                256,
                I32x8,
                I16x8,
                true,
                16,
                Vpmovusdw
            ),
            Self::X86Avx512VlPmovusqb256 => row!(
                "x86_avx512vl_pmovusqb256",
                256,
                I64x4,
                I8x16,
                false,
                4,
                Vpmovusqb
            ),
            Self::X86Avx512VlPmovusqbStore256 => row!(
                "x86_avx512vl_pmovusqbstore256",
                256,
                I64x4,
                I8x16,
                true,
                4,
                Vpmovusqb
            ),
            Self::X86Avx512VlPmovusqw256 => row!(
                "x86_avx512vl_pmovusqw256",
                256,
                I64x4,
                I16x8,
                false,
                8,
                Vpmovusqw
            ),
            Self::X86Avx512VlPmovusqwStore256 => row!(
                "x86_avx512vl_pmovusqwstore256",
                256,
                I64x4,
                I16x8,
                true,
                8,
                Vpmovusqw
            ),
            Self::X86Avx512VlPmovusqd256 => row!(
                "x86_avx512vl_pmovusqd256",
                256,
                I64x4,
                I32x4,
                false,
                16,
                Vpmovusqd
            ),
            Self::X86Avx512VlPmovusqdStore256 => row!(
                "x86_avx512vl_pmovusqdstore256",
                256,
                I64x4,
                I32x4,
                true,
                16,
                Vpmovusqd
            ),
            _ => return None,
        })
    }
}

//! Pure masked VL integer operations with independent merge values.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512VlIntegerKind {
    Abs,
    Multiply,
    Ternary,
    Test,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlInteger {
    pub name: &'static str,
    pub opcode: &'static str,
    pub vector: VectorType,
    pub kind: X86Avx512VlIntegerKind,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_vl_integer(self) -> Option<X86Avx512VlInteger> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$opcode:literal,$v:ident,$d:ident,$q:ident,$params:expr) => {{
                const V: Type = Type::Vector(VectorType::$v);
                const D: Type = Type::Vector(VectorType::$d);
                const Q: Type = Type::Vector(VectorType::$q);
                // The unused shape constants keep the signature table explicit.
                let _ = (D, Q);
                X86Avx512VlInteger {
                    name: $name,
                    opcode: $opcode,
                    vector: VectorType::$v,
                    kind: X86Avx512VlIntegerKind::$kind,
                    params: $params,
                    returns: if matches!(
                        X86Avx512VlIntegerKind::$kind,
                        X86Avx512VlIntegerKind::Test
                    ) {
                        &[Type::I8]
                    } else {
                        &[V]
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlPabsd128 => descriptor!(
                Abs,
                "x86_avx512vl_pabsd128",
                "vpabsd",
                I32x4,
                I32x4,
                I64x2,
                &[V, Type::I8, V]
            ),
            Self::X86Avx512VlPabsq128 => descriptor!(
                Abs,
                "x86_avx512vl_pabsq128",
                "vpabsq",
                I64x2,
                I32x4,
                I64x2,
                &[V, Type::I8, V]
            ),
            Self::X86Avx512VlPmuldq128 => descriptor!(
                Multiply,
                "x86_avx512vl_pmuldq128",
                "vpmuldq",
                I64x2,
                I32x4,
                I64x2,
                &[Q, Type::I8, D, D]
            ),
            Self::X86Avx512VlPmuludq128 => descriptor!(
                Multiply,
                "x86_avx512vl_pmuludq128",
                "vpmuludq",
                I64x2,
                I32x4,
                I64x2,
                &[Q, Type::I8, D, D]
            ),
            Self::X86Avx512VlPternlogd128 => descriptor!(
                Ternary,
                "x86_avx512vl_pternlogd128",
                "vpternlogd",
                I32x4,
                I32x4,
                I64x2,
                &[V, Type::I8, V, V, V, Type::I32]
            ),
            Self::X86Avx512VlPternlogq128 => descriptor!(
                Ternary,
                "x86_avx512vl_pternlogq128",
                "vpternlogq",
                I64x2,
                I32x4,
                I64x2,
                &[V, Type::I8, V, V, V, Type::I32]
            ),
            Self::X86Avx512VlPtestmd128 => descriptor!(
                Test,
                "x86_avx512vl_ptestmd128",
                "vptestmd",
                I32x4,
                I32x4,
                I64x2,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestmq128 => descriptor!(
                Test,
                "x86_avx512vl_ptestmq128",
                "vptestmq",
                I64x2,
                I32x4,
                I64x2,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestnmd128 => descriptor!(
                Test,
                "x86_avx512vl_ptestnmd128",
                "vptestnmd",
                I32x4,
                I32x4,
                I64x2,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestnmq128 => descriptor!(
                Test,
                "x86_avx512vl_ptestnmq128",
                "vptestnmq",
                I64x2,
                I32x4,
                I64x2,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPabsd256 => descriptor!(
                Abs,
                "x86_avx512vl_pabsd256",
                "vpabsd",
                I32x8,
                I32x8,
                I64x4,
                &[V, Type::I8, V]
            ),
            Self::X86Avx512VlPabsq256 => descriptor!(
                Abs,
                "x86_avx512vl_pabsq256",
                "vpabsq",
                I64x4,
                I32x8,
                I64x4,
                &[V, Type::I8, V]
            ),
            Self::X86Avx512VlPmuldq256 => descriptor!(
                Multiply,
                "x86_avx512vl_pmuldq256",
                "vpmuldq",
                I64x4,
                I32x8,
                I64x4,
                &[Q, Type::I8, D, D]
            ),
            Self::X86Avx512VlPmuludq256 => descriptor!(
                Multiply,
                "x86_avx512vl_pmuludq256",
                "vpmuludq",
                I64x4,
                I32x8,
                I64x4,
                &[Q, Type::I8, D, D]
            ),
            Self::X86Avx512VlPternlogd256 => descriptor!(
                Ternary,
                "x86_avx512vl_pternlogd256",
                "vpternlogd",
                I32x8,
                I32x8,
                I64x4,
                &[V, Type::I8, V, V, V, Type::I32]
            ),
            Self::X86Avx512VlPternlogq256 => descriptor!(
                Ternary,
                "x86_avx512vl_pternlogq256",
                "vpternlogq",
                I64x4,
                I32x8,
                I64x4,
                &[V, Type::I8, V, V, V, Type::I32]
            ),
            Self::X86Avx512VlPtestmd256 => descriptor!(
                Test,
                "x86_avx512vl_ptestmd256",
                "vptestmd",
                I32x8,
                I32x8,
                I64x4,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestmq256 => descriptor!(
                Test,
                "x86_avx512vl_ptestmq256",
                "vptestmq",
                I64x4,
                I32x8,
                I64x4,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestnmd256 => descriptor!(
                Test,
                "x86_avx512vl_ptestnmd256",
                "vptestnmd",
                I32x8,
                I32x8,
                I64x4,
                &[V, V, Type::I8]
            ),
            Self::X86Avx512VlPtestnmq256 => descriptor!(
                Test,
                "x86_avx512vl_ptestnmq256",
                "vptestnmq",
                I64x4,
                I32x8,
                I64x4,
                &[V, V, Type::I8]
            ),
            _ => return None,
        })
    }
}

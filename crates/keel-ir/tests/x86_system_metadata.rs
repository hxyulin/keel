//! Frozen pre-generation IDs and signatures; no privileged instructions execute.
#[path = "x86_system_metadata/x86_query_metadata.rs"]
mod x86_query_metadata;

use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use std::collections::HashMap;

fn fixture(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter(|line| !line.starts_with('#'))
}

#[test]
fn all_numeric_ids_and_inventory_order_stay_stable() {
    let actual: HashMap<_, _> = Intrinsic::ALL
        .iter()
        .map(|id| (format!("{id:?}"), *id as usize))
        .collect();
    let expected: Vec<_> = fixture(include_str!("fixtures/intrinsic-inventory.txt")).collect();
    let extensions = &[
        Intrinsic::LifetimeStart,
        Intrinsic::LifetimeEnd,
        Intrinsic::X86CpuXcr0,
        Intrinsic::X86Sha256msg1,
        Intrinsic::X86Sha256msg2,
        Intrinsic::X86Sha256rnds2,
        Intrinsic::X86Vpclmulqdq256,
        Intrinsic::X86Vpclmulqdq512,
        Intrinsic::X86Avx512Cmpd,
        Intrinsic::X86Avx512Cmpq,
        Intrinsic::X86Avx512Cmpud,
        Intrinsic::X86Avx512Cmpuq,
        Intrinsic::X86Avx512MaskAddd,
        Intrinsic::X86Avx512MaskAddq,
        Intrinsic::X86Avx512MaskSubd,
        Intrinsic::X86Avx512MaskSubq,
        Intrinsic::X86Avx512MaskLoadd,
        Intrinsic::X86Avx512MaskLoadq,
        Intrinsic::X86Avx512MaskStored,
        Intrinsic::X86Avx512MaskStoreq,
        Intrinsic::X86Avx512Cmpps,
        Intrinsic::X86Avx512Cmppd,
        Intrinsic::X86Avx512MaskAddps,
        Intrinsic::X86Avx512MaskAddpd,
        Intrinsic::X86Avx512MaskSubps,
        Intrinsic::X86Avx512MaskSubpd,
        Intrinsic::X86Avx512MaskMulps,
        Intrinsic::X86Avx512MaskMulpd,
        Intrinsic::X86Avx512MaskDivps,
        Intrinsic::X86Avx512MaskDivpd,
        Intrinsic::X86Avx512MaskSqrtps,
        Intrinsic::X86Avx512MaskSqrtpd,
        Intrinsic::X86Avx512MaskLoadps,
        Intrinsic::X86Avx512MaskLoadpd,
        Intrinsic::X86Avx512MaskStoreps,
        Intrinsic::X86Avx512MaskStorepd,
        Intrinsic::X86Avx512Vpgatherdd,
        Intrinsic::X86Avx512Vpgatherdq,
        Intrinsic::X86Avx512Vpgatherqd,
        Intrinsic::X86Avx512Vpgatherqq,
        Intrinsic::X86Avx512Vgatherdps,
        Intrinsic::X86Avx512Vgatherdpd,
        Intrinsic::X86Avx512Vgatherqps,
        Intrinsic::X86Avx512Vgatherqpd,
        Intrinsic::X86Avx512Vpscatterdd,
        Intrinsic::X86Avx512Vpscatterdq,
        Intrinsic::X86Avx512Vpscatterqd,
        Intrinsic::X86Avx512Vpscatterqq,
        Intrinsic::X86Avx512Vscatterdps,
        Intrinsic::X86Avx512Vscatterdpd,
        Intrinsic::X86Avx512Vscatterqps,
        Intrinsic::X86Avx512Vscatterqpd,
        Intrinsic::X86Avx512Compressd,
        Intrinsic::X86Avx512Compressq,
        Intrinsic::X86Avx512Compressps,
        Intrinsic::X86Avx512Compresspd,
        Intrinsic::X86Avx512Expandd,
        Intrinsic::X86Avx512Expandq,
        Intrinsic::X86Avx512Expandps,
        Intrinsic::X86Avx512Expandpd,
        Intrinsic::X86Avx512CompressStored,
        Intrinsic::X86Avx512CompressStoreq,
        Intrinsic::X86Avx512CompressStoreps,
        Intrinsic::X86Avx512CompressStorepd,
        Intrinsic::X86Avx512ExpandLoadd,
        Intrinsic::X86Avx512ExpandLoadq,
        Intrinsic::X86Avx512ExpandLoadps,
        Intrinsic::X86Avx512ExpandLoadpd,
        Intrinsic::X86Avx512Vcvtdq2ps,
        Intrinsic::X86Avx512Vcvtudq2ps,
        Intrinsic::X86Avx512Vcvtdq2pd,
        Intrinsic::X86Avx512Vcvtudq2pd,
        Intrinsic::X86Avx512Vcvtps2dq,
        Intrinsic::X86Avx512Vcvtps2udq,
        Intrinsic::X86Avx512Vcvttps2dq,
        Intrinsic::X86Avx512Vcvttps2udq,
        Intrinsic::X86Avx512Vcvtpd2dq,
        Intrinsic::X86Avx512Vcvtpd2udq,
        Intrinsic::X86Avx512Vcvttpd2dq,
        Intrinsic::X86Avx512Vcvttpd2udq,
        Intrinsic::X86Avx512Vcvtps2pd,
        Intrinsic::X86Avx512Vcvtpd2ps,
        Intrinsic::X86Avx512Vcvtph2ps,
        Intrinsic::X86Avx512Vcvtps2ph,
        Intrinsic::X86Avx512Getexpps,
        Intrinsic::X86Avx512Getexppd,
        Intrinsic::X86Avx512Getmantps,
        Intrinsic::X86Avx512Getmantpd,
        Intrinsic::X86Avx512Scalefps,
        Intrinsic::X86Avx512Scalefpd,
        Intrinsic::X86Avx512Rcp14ps,
        Intrinsic::X86Avx512Rcp14pd,
        Intrinsic::X86Avx512Rsqrt14ps,
        Intrinsic::X86Avx512Rsqrt14pd,
        Intrinsic::X86Avx512Roundscaleps,
        Intrinsic::X86Avx512Roundscalepd,
        Intrinsic::X86Avx512Fixupimmps,
        Intrinsic::X86Avx512Fixupimmpd,
        Intrinsic::X86Avx512MaskTernlogd,
        Intrinsic::X86Avx512MaskTernlogq,
        Intrinsic::X86Avx512MaskRold,
        Intrinsic::X86Avx512MaskRolq,
        Intrinsic::X86Avx512MaskRord,
        Intrinsic::X86Avx512MaskRorq,
        Intrinsic::X86Avx512MaskRolvd,
        Intrinsic::X86Avx512MaskRolvq,
        Intrinsic::X86Avx512MaskRorvd,
        Intrinsic::X86Avx512MaskRorvq,
        Intrinsic::X86Avx512MaskSllvd,
        Intrinsic::X86Avx512MaskSllvq,
        Intrinsic::X86Avx512MaskSrlvd,
        Intrinsic::X86Avx512MaskSrlvq,
        Intrinsic::X86Avx512MaskSravd,
        Intrinsic::X86Avx512MaskSravq,
        Intrinsic::X86Avx512Testd,
        Intrinsic::X86Avx512Testq,
        Intrinsic::X86Avx512Testnd,
        Intrinsic::X86Avx512Testnq,
        Intrinsic::X86Avx512MaskAbsd,
        Intrinsic::X86Avx512MaskAbsq,
        Intrinsic::X86Avx512MaskMuldq,
        Intrinsic::X86Avx512MaskMuludq,
        Intrinsic::X86Avx512Vfmadd132ps,
        Intrinsic::X86Avx512Vfmadd132pd,
        Intrinsic::X86Avx512Vfmadd213ps,
        Intrinsic::X86Avx512Vfmadd213pd,
        Intrinsic::X86Avx512Vfmadd231ps,
        Intrinsic::X86Avx512Vfmadd231pd,
        Intrinsic::X86Avx512Vfmsub132ps,
        Intrinsic::X86Avx512Vfmsub132pd,
        Intrinsic::X86Avx512Vfmsub213ps,
        Intrinsic::X86Avx512Vfmsub213pd,
        Intrinsic::X86Avx512Vfmsub231ps,
        Intrinsic::X86Avx512Vfmsub231pd,
        Intrinsic::X86Avx512Vfnmadd132ps,
        Intrinsic::X86Avx512Vfnmadd132pd,
        Intrinsic::X86Avx512Vfnmadd213ps,
        Intrinsic::X86Avx512Vfnmadd213pd,
        Intrinsic::X86Avx512Vfnmadd231ps,
        Intrinsic::X86Avx512Vfnmadd231pd,
        Intrinsic::X86Avx512Vfnmsub132ps,
        Intrinsic::X86Avx512Vfnmsub132pd,
        Intrinsic::X86Avx512Vfnmsub213ps,
        Intrinsic::X86Avx512Vfnmsub213pd,
        Intrinsic::X86Avx512Vfnmsub231ps,
        Intrinsic::X86Avx512Vfnmsub231pd,
        Intrinsic::X86Avx512Vfmaddsub132ps,
        Intrinsic::X86Avx512Vfmaddsub132pd,
        Intrinsic::X86Avx512Vfmaddsub213ps,
        Intrinsic::X86Avx512Vfmaddsub213pd,
        Intrinsic::X86Avx512Vfmaddsub231ps,
        Intrinsic::X86Avx512Vfmaddsub231pd,
        Intrinsic::X86Avx512Vfmsubadd132ps,
        Intrinsic::X86Avx512Vfmsubadd132pd,
        Intrinsic::X86Avx512Vfmsubadd213ps,
        Intrinsic::X86Avx512Vfmsubadd213pd,
        Intrinsic::X86Avx512Vfmsubadd231ps,
        Intrinsic::X86Avx512Vfmsubadd231pd,
        Intrinsic::X86Avx512Addss,
        Intrinsic::X86Avx512Addsd,
        Intrinsic::X86Avx512Subss,
        Intrinsic::X86Avx512Subsd,
        Intrinsic::X86Avx512Mulss,
        Intrinsic::X86Avx512Mulsd,
        Intrinsic::X86Avx512Divss,
        Intrinsic::X86Avx512Divsd,
        Intrinsic::X86Avx512Sqrtss,
        Intrinsic::X86Avx512Sqrtsd,
        Intrinsic::X86Avx512Minss,
        Intrinsic::X86Avx512Minsd,
        Intrinsic::X86Avx512Maxss,
        Intrinsic::X86Avx512Maxsd,
        Intrinsic::X86Avx512Cmpss,
        Intrinsic::X86Avx512Cmpsd,
        Intrinsic::X86Avx512Getexpss,
        Intrinsic::X86Avx512Getexpsd,
        Intrinsic::X86Avx512Getmantss,
        Intrinsic::X86Avx512Getmantsd,
        Intrinsic::X86Avx512Scalefss,
        Intrinsic::X86Avx512Scalefsd,
        Intrinsic::X86Avx512Rcp14ss,
        Intrinsic::X86Avx512Rcp14sd,
        Intrinsic::X86Avx512Rsqrt14ss,
        Intrinsic::X86Avx512Rsqrt14sd,
        Intrinsic::X86Avx512Roundscaless,
        Intrinsic::X86Avx512Roundscalesd,
        Intrinsic::X86Avx512Fixupimmss,
        Intrinsic::X86Avx512Fixupimmsd,
        Intrinsic::X86Avx512Kandw,
        Intrinsic::X86Avx512Kandnw,
        Intrinsic::X86Avx512Korw,
        Intrinsic::X86Avx512Kxorw,
        Intrinsic::X86Avx512Kxnorw,
        Intrinsic::X86Avx512Knotw,
        Intrinsic::X86Avx512Kshiftlw,
        Intrinsic::X86Avx512Kshiftrw,
        Intrinsic::X86Avx512Kunpckbw,
        Intrinsic::X86Avx512Ktestw,
        Intrinsic::X86Avx512Kortestw,
        Intrinsic::X86Avx512Vfmadd132ss,
        Intrinsic::X86Avx512Vfmadd132sd,
        Intrinsic::X86Avx512Vfmadd213ss,
        Intrinsic::X86Avx512Vfmadd213sd,
        Intrinsic::X86Avx512Vfmadd231ss,
        Intrinsic::X86Avx512Vfmadd231sd,
        Intrinsic::X86Avx512Vfmsub132ss,
        Intrinsic::X86Avx512Vfmsub132sd,
        Intrinsic::X86Avx512Vfmsub213ss,
        Intrinsic::X86Avx512Vfmsub213sd,
        Intrinsic::X86Avx512Vfmsub231ss,
        Intrinsic::X86Avx512Vfmsub231sd,
        Intrinsic::X86Avx512Vfnmadd132ss,
        Intrinsic::X86Avx512Vfnmadd132sd,
        Intrinsic::X86Avx512Vfnmadd213ss,
        Intrinsic::X86Avx512Vfnmadd213sd,
        Intrinsic::X86Avx512Vfnmadd231ss,
        Intrinsic::X86Avx512Vfnmadd231sd,
        Intrinsic::X86Avx512Vfnmsub132ss,
        Intrinsic::X86Avx512Vfnmsub132sd,
        Intrinsic::X86Avx512Vfnmsub213ss,
        Intrinsic::X86Avx512Vfnmsub213sd,
        Intrinsic::X86Avx512Vfnmsub231ss,
        Intrinsic::X86Avx512Vfnmsub231sd,
        Intrinsic::X86Avx512Vcvtsi2ss32,
        Intrinsic::X86Avx512Vcvtsi2sd32,
        Intrinsic::X86Avx512Vcvtsi2ss64,
        Intrinsic::X86Avx512Vcvtsi2sd64,
        Intrinsic::X86Avx512Vcvtusi2ss32,
        Intrinsic::X86Avx512Vcvtusi2sd32,
        Intrinsic::X86Avx512Vcvtusi2ss64,
        Intrinsic::X86Avx512Vcvtusi2sd64,
        Intrinsic::X86Avx512Vcvtss2si32,
        Intrinsic::X86Avx512Vcvtss2si64,
        Intrinsic::X86Avx512Vcvttss2si32,
        Intrinsic::X86Avx512Vcvttss2si64,
        Intrinsic::X86Avx512Vcvtss2usi32,
        Intrinsic::X86Avx512Vcvtss2usi64,
        Intrinsic::X86Avx512Vcvttss2usi32,
        Intrinsic::X86Avx512Vcvttss2usi64,
        Intrinsic::X86Avx512Vcvtsd2si32,
        Intrinsic::X86Avx512Vcvtsd2si64,
        Intrinsic::X86Avx512Vcvttsd2si32,
        Intrinsic::X86Avx512Vcvttsd2si64,
        Intrinsic::X86Avx512Vcvtsd2usi32,
        Intrinsic::X86Avx512Vcvtsd2usi64,
        Intrinsic::X86Avx512Vcvttsd2usi32,
        Intrinsic::X86Avx512Vcvttsd2usi64,
        Intrinsic::X86Avx512Vcvtss2sd,
        Intrinsic::X86Avx512Vcvtsd2ss,
        Intrinsic::X86Avx512Vcomiss,
        Intrinsic::X86Avx512Vcomisd,
        Intrinsic::X86Avx512Vucomiss,
        Intrinsic::X86Avx512Vucomisd,
        Intrinsic::X86Avx512MaskPmovsxbd,
        Intrinsic::X86Avx512MaskPmovsxbq,
        Intrinsic::X86Avx512MaskPmovsxwd,
        Intrinsic::X86Avx512MaskPmovsxwq,
        Intrinsic::X86Avx512MaskPmovsxdq,
        Intrinsic::X86Avx512MaskPmovzxbd,
        Intrinsic::X86Avx512MaskPmovzxbq,
        Intrinsic::X86Avx512MaskPmovzxwd,
        Intrinsic::X86Avx512MaskPmovzxwq,
        Intrinsic::X86Avx512MaskPmovzxdq,
        Intrinsic::X86Avx512MaskLoadPmovsxbd,
        Intrinsic::X86Avx512MaskLoadPmovsxbq,
        Intrinsic::X86Avx512MaskLoadPmovsxwd,
        Intrinsic::X86Avx512MaskLoadPmovsxwq,
        Intrinsic::X86Avx512MaskLoadPmovsxdq,
        Intrinsic::X86Avx512MaskLoadPmovzxbd,
        Intrinsic::X86Avx512MaskLoadPmovzxbq,
        Intrinsic::X86Avx512MaskLoadPmovzxwd,
        Intrinsic::X86Avx512MaskLoadPmovzxwq,
        Intrinsic::X86Avx512MaskLoadPmovzxdq,
        Intrinsic::X86Avx512MaskPmovdb,
        Intrinsic::X86Avx512MaskPmovdw,
        Intrinsic::X86Avx512MaskPmovqb,
        Intrinsic::X86Avx512MaskPmovqd,
        Intrinsic::X86Avx512MaskPmovqw,
        Intrinsic::X86Avx512MaskPmovsdb,
        Intrinsic::X86Avx512MaskPmovsdw,
        Intrinsic::X86Avx512MaskPmovsqb,
        Intrinsic::X86Avx512MaskPmovsqd,
        Intrinsic::X86Avx512MaskPmovsqw,
        Intrinsic::X86Avx512MaskPmovusdb,
        Intrinsic::X86Avx512MaskPmovusdw,
        Intrinsic::X86Avx512MaskPmovusqb,
        Intrinsic::X86Avx512MaskPmovusqd,
        Intrinsic::X86Avx512MaskPmovusqw,
        Intrinsic::X86Avx512MaskStorePmovdb,
        Intrinsic::X86Avx512MaskStorePmovdw,
        Intrinsic::X86Avx512MaskStorePmovqb,
        Intrinsic::X86Avx512MaskStorePmovqd,
        Intrinsic::X86Avx512MaskStorePmovqw,
        Intrinsic::X86Avx512MaskStorePmovsdb,
        Intrinsic::X86Avx512MaskStorePmovsdw,
        Intrinsic::X86Avx512MaskStorePmovsqb,
        Intrinsic::X86Avx512MaskStorePmovsqd,
        Intrinsic::X86Avx512MaskStorePmovsqw,
        Intrinsic::X86Avx512MaskStorePmovusdb,
        Intrinsic::X86Avx512MaskStorePmovusdw,
        Intrinsic::X86Avx512MaskStorePmovusqb,
        Intrinsic::X86Avx512MaskStorePmovusqd,
        Intrinsic::X86Avx512MaskStorePmovusqw,
        Intrinsic::X86Avx512Vpermd,
        Intrinsic::X86Avx512Vpermq,
        Intrinsic::X86Avx512Vpermps,
        Intrinsic::X86Avx512Vpermpd,
        Intrinsic::X86Avx512Vpermi2d,
        Intrinsic::X86Avx512Vpermi2q,
        Intrinsic::X86Avx512Vpermi2ps,
        Intrinsic::X86Avx512Vpermi2pd,
        Intrinsic::X86Avx512Vpermt2d,
        Intrinsic::X86Avx512Vpermt2q,
        Intrinsic::X86Avx512Vpermt2ps,
        Intrinsic::X86Avx512Vpermt2pd,
        Intrinsic::X86Avx512Vpermilps,
        Intrinsic::X86Avx512Vpermilpd,
        Intrinsic::X86Avx512VpermilpsImm,
        Intrinsic::X86Avx512VpermilpdImm,
        Intrinsic::X86Avx512VpermqImm,
        Intrinsic::X86Avx512VpermpdImm,
        Intrinsic::X86Avx512Vshufps,
        Intrinsic::X86Avx512Vshufpd,
        Intrinsic::X86Avx512Vpshufd,
        Intrinsic::X86Avx512Vshufi32x4,
        Intrinsic::X86Avx512Vshufi64x2,
        Intrinsic::X86Avx512Vshuff32x4,
        Intrinsic::X86Avx512Vshuff64x2,
        Intrinsic::X86Avx512Valignd,
        Intrinsic::X86Avx512Valignq,
        Intrinsic::X86Avx512Vpunpckldq,
        Intrinsic::X86Avx512Vpunpcklqdq,
        Intrinsic::X86Avx512Vunpcklps,
        Intrinsic::X86Avx512Vunpcklpd,
        Intrinsic::X86Avx512Vpunpckhdq,
        Intrinsic::X86Avx512Vpunpckhqdq,
        Intrinsic::X86Avx512Vunpckhps,
        Intrinsic::X86Avx512Vunpckhpd,
        Intrinsic::X86Avx512Vmovshdup,
        Intrinsic::X86Avx512Vmovsldup,
        Intrinsic::X86Avx512Vmovddup,
        Intrinsic::X86Avx512MaskMulld,
        Intrinsic::X86Avx512MaskMinsd,
        Intrinsic::X86Avx512MaskMinud,
        Intrinsic::X86Avx512MaskMaxsd,
        Intrinsic::X86Avx512MaskMaxud,
        Intrinsic::X86Avx512MaskAndd,
        Intrinsic::X86Avx512MaskAndnd,
        Intrinsic::X86Avx512MaskOrd,
        Intrinsic::X86Avx512MaskXord,
        Intrinsic::X86Avx512MaskMinsq,
        Intrinsic::X86Avx512MaskMinuq,
        Intrinsic::X86Avx512MaskMaxsq,
        Intrinsic::X86Avx512MaskMaxuq,
        Intrinsic::X86Avx512MaskAndq,
        Intrinsic::X86Avx512MaskAndnq,
        Intrinsic::X86Avx512MaskOrq,
        Intrinsic::X86Avx512MaskXorq,
        Intrinsic::X86Avx512MaskAddRoundps,
        Intrinsic::X86Avx512MaskAddRoundpd,
        Intrinsic::X86Avx512MaskSubRoundps,
        Intrinsic::X86Avx512MaskSubRoundpd,
        Intrinsic::X86Avx512MaskMulRoundps,
        Intrinsic::X86Avx512MaskMulRoundpd,
        Intrinsic::X86Avx512MaskDivRoundps,
        Intrinsic::X86Avx512MaskDivRoundpd,
        Intrinsic::X86Avx512MaskSqrtRoundps,
        Intrinsic::X86Avx512MaskSqrtRoundpd,
        Intrinsic::X86Avx512MaskMinps,
        Intrinsic::X86Avx512MaskMinpd,
        Intrinsic::X86Avx512MaskMaxps,
        Intrinsic::X86Avx512MaskMaxpd,
        Intrinsic::X86Avx512MaskMoved,
        Intrinsic::X86Avx512Blendd,
        Intrinsic::X86Avx512MaskMoveq,
        Intrinsic::X86Avx512Blendq,
        Intrinsic::X86Avx512MaskMoveps,
        Intrinsic::X86Avx512Blendps,
        Intrinsic::X86Avx512MaskMovepd,
        Intrinsic::X86Avx512Blendpd,
        Intrinsic::X86Avx512MaskAlignedLoadd,
        Intrinsic::X86Avx512MaskAlignedStored,
        Intrinsic::X86Avx512MaskAlignedLoadq,
        Intrinsic::X86Avx512MaskAlignedStoreq,
        Intrinsic::X86Avx512MaskAlignedLoadps,
        Intrinsic::X86Avx512MaskAlignedStoreps,
        Intrinsic::X86Avx512MaskAlignedLoadpd,
        Intrinsic::X86Avx512MaskAlignedStorepd,
        Intrinsic::X86Avx512StreamLoaddq,
        Intrinsic::X86Avx512StreamStoredq,
        Intrinsic::X86Avx512StreamStoreps,
        Intrinsic::X86Avx512StreamStorepd,
        Intrinsic::X86Avx512MaskSllid,
        Intrinsic::X86Avx512MaskSlliq,
        Intrinsic::X86Avx512MaskSrlid,
        Intrinsic::X86Avx512MaskSrliq,
        Intrinsic::X86Avx512MaskSraid,
        Intrinsic::X86Avx512MaskSraiq,
        Intrinsic::X86Avx512MaskSlld,
        Intrinsic::X86Avx512MaskSllq,
        Intrinsic::X86Avx512MaskSrld,
        Intrinsic::X86Avx512MaskSrlq,
        Intrinsic::X86Avx512MaskSrad,
        Intrinsic::X86Avx512MaskSraq,
        Intrinsic::X86Avx512MaskPbroadcastd,
        Intrinsic::X86Avx512MaskPbroadcastq,
        Intrinsic::X86Avx512MaskBroadcastss,
        Intrinsic::X86Avx512MaskBroadcastsd,
        Intrinsic::X86Avx512MaskLoadPbroadcastd,
        Intrinsic::X86Avx512MaskLoadPbroadcastq,
        Intrinsic::X86Avx512MaskLoadBroadcastss,
        Intrinsic::X86Avx512MaskLoadBroadcastsd,
        Intrinsic::X86Avx512MaskBroadcasti32x4,
        Intrinsic::X86Avx512MaskBroadcasti64x4,
        Intrinsic::X86Avx512MaskBroadcastf32x4,
        Intrinsic::X86Avx512MaskBroadcastf64x4,
        Intrinsic::X86Avx512MaskLoadBroadcasti32x4,
        Intrinsic::X86Avx512MaskLoadBroadcasti64x4,
        Intrinsic::X86Avx512MaskLoadBroadcastf32x4,
        Intrinsic::X86Avx512MaskLoadBroadcastf64x4,
        Intrinsic::X86Avx512MaskExtracti32x4,
        Intrinsic::X86Avx512MaskExtracti64x4,
        Intrinsic::X86Avx512MaskExtractf32x4,
        Intrinsic::X86Avx512MaskExtractf64x4,
        Intrinsic::X86Avx512MaskStoreExtracti32x4,
        Intrinsic::X86Avx512MaskStoreExtracti64x4,
        Intrinsic::X86Avx512MaskStoreExtractf32x4,
        Intrinsic::X86Avx512MaskStoreExtractf64x4,
        Intrinsic::X86Avx512MaskInserti32x4,
        Intrinsic::X86Avx512MaskInserti64x4,
        Intrinsic::X86Avx512MaskInsertf32x4,
        Intrinsic::X86Avx512MaskInsertf64x4,
        Intrinsic::X86Avx512MaskLoadInserti32x4,
        Intrinsic::X86Avx512MaskLoadInserti64x4,
        Intrinsic::X86Avx512MaskLoadInsertf32x4,
        Intrinsic::X86Avx512MaskLoadInsertf64x4,
        Intrinsic::X86Avx512MaskMovess,
        Intrinsic::X86Avx512MaskMovesd,
        Intrinsic::X86Avx512MaskLoadss,
        Intrinsic::X86Avx512MaskLoadsd,
        Intrinsic::X86Avx512MaskStoress,
        Intrinsic::X86Avx512MaskStoresd,
        Intrinsic::X86Avx512CmpRoundps,
        Intrinsic::X86Avx512CmpRoundpd,
        Intrinsic::X86Avx512VlMaskAddd128,
        Intrinsic::X86Avx512VlMaskAddq128,
        Intrinsic::X86Avx512VlMaskSubd128,
        Intrinsic::X86Avx512VlMaskSubq128,
        Intrinsic::X86Avx512VlMaskLoadd128,
        Intrinsic::X86Avx512VlMaskLoadq128,
        Intrinsic::X86Avx512VlMaskStored128,
        Intrinsic::X86Avx512VlMaskStoreq128,
        Intrinsic::X86Avx512VlCmpd128,
        Intrinsic::X86Avx512VlCmpq128,
        Intrinsic::X86Avx512VlCmpud128,
        Intrinsic::X86Avx512VlCmpuq128,
        Intrinsic::X86Avx512VlMaskAddps128,
        Intrinsic::X86Avx512VlMaskAddpd128,
        Intrinsic::X86Avx512VlMaskSubps128,
        Intrinsic::X86Avx512VlMaskSubpd128,
        Intrinsic::X86Avx512VlMaskLoadps128,
        Intrinsic::X86Avx512VlMaskLoadpd128,
        Intrinsic::X86Avx512VlMaskStoreps128,
        Intrinsic::X86Avx512VlMaskStorepd128,
        Intrinsic::X86Avx512VlCmpps128,
        Intrinsic::X86Avx512VlCmppd128,
        Intrinsic::X86Avx512VlMaskAddd256,
        Intrinsic::X86Avx512VlMaskAddq256,
        Intrinsic::X86Avx512VlMaskSubd256,
        Intrinsic::X86Avx512VlMaskSubq256,
        Intrinsic::X86Avx512VlMaskLoadd256,
        Intrinsic::X86Avx512VlMaskLoadq256,
        Intrinsic::X86Avx512VlMaskStored256,
        Intrinsic::X86Avx512VlMaskStoreq256,
        Intrinsic::X86Avx512VlCmpd256,
        Intrinsic::X86Avx512VlCmpq256,
        Intrinsic::X86Avx512VlCmpud256,
        Intrinsic::X86Avx512VlCmpuq256,
        Intrinsic::X86Avx512VlMaskAddps256,
        Intrinsic::X86Avx512VlMaskAddpd256,
        Intrinsic::X86Avx512VlMaskSubps256,
        Intrinsic::X86Avx512VlMaskSubpd256,
        Intrinsic::X86Avx512VlMaskLoadps256,
        Intrinsic::X86Avx512VlMaskLoadpd256,
        Intrinsic::X86Avx512VlMaskStoreps256,
        Intrinsic::X86Avx512VlMaskStorepd256,
        Intrinsic::X86Avx512VlCmpps256,
        Intrinsic::X86Avx512VlCmppd256,
    ];
    assert_eq!(Intrinsic::ALL.len(), expected.len() + extensions.len());
    assert_eq!(&Intrinsic::ALL[expected.len()..], extensions);
    assert_eq!(
        actual.len(),
        Intrinsic::ALL.len(),
        "duplicate inventory entry"
    );
    let mut names = std::collections::HashSet::new();
    for intrinsic in Intrinsic::ALL {
        assert!(
            names.insert(intrinsic.info().name),
            "duplicate intrinsic name"
        );
        assert_eq!(Intrinsic::from_name(intrinsic.info().name), Some(intrinsic));
    }
    for (index, row) in expected.iter().enumerate() {
        let (numeric, inventory) = row.split_once('|').unwrap();
        assert_eq!(actual[numeric], index, "numeric ID for {numeric}");
        assert_eq!(format!("{:?}", Intrinsic::ALL[index]), inventory);
    }
}

fn types(source: &str) -> Vec<Type> {
    if source.is_empty() {
        return vec![];
    }
    source
        .split(',')
        .map(|name| match name {
            "I16" => Type::I16,
            "I32" => Type::I32,
            "I64" => Type::I64,
            "Ptr" => Type::Ptr,
            _ => panic!("unexpected fixture type {name}"),
        })
        .collect()
}

#[test]
fn system_signatures_effects_names_and_target_classification_stay_stable() {
    let rows: Vec<_> = fixture(include_str!("fixtures/x86-system-intrinsics.txt")).collect();
    assert_eq!(rows.len(), 45);
    for row in rows {
        let fields: Vec<_> = row.split('|').collect();
        let id = Intrinsic::from_name(fields[2]).unwrap();
        assert_eq!(id as usize, fields[0].parse::<usize>().unwrap());
        assert_eq!(format!("{id:?}"), fields[1]);
        let info = id.info();
        assert_eq!(info.name, fields[2]);
        assert_eq!(id.to_string(), fields[2]);
        assert_eq!(info.params, types(fields[3]));
        assert_eq!(info.returns, types(fields[4]));
        assert_eq!(info.pure, fields[5].parse::<bool>().unwrap());
        assert_eq!(fields[6], "Barrier");
        assert_eq!(fields[7], "x86_64");
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
        assert!(inst.is_pinned());
        assert!(id.is_x86_target());
        // Exercise exported patterns from outside the defining crate.
        assert!(matches!(id, keel_ir::x86_system_intrinsics!(x86_64)));
        assert!(matches!(id, keel_ir::x86_system_intrinsics!(Barrier)));
    }
    for id in Intrinsic::ALL {
        assert_eq!(
            matches!(id, keel_ir::x86_system_intrinsics!(all)),
            (774..=818).contains(&(id as usize)),
            "{id:?}"
        );
    }
}

#[test]
fn generated_helper_mappings_match_the_frozen_public_api() {
    let expected: HashMap<_, _> = fixture(include_str!("fixtures/x86-system-helper-mappings.txt"))
        .map(|row| {
            let fields: Vec<_> = row.split('|').collect();
            (fields[0], fields)
        })
        .collect();
    assert_eq!(expected.len(), 35);
    for id in Intrinsic::ALL {
        let name = format!("{id:?}");
        match expected.get(name.as_str()) {
            Some(fields) if fields[1] == "register" => {
                assert_eq!(
                    id.x86_system_register(),
                    Some((fields[2], fields[3].parse().unwrap()))
                );
                assert_eq!(id.x86_system_write(), None);
            }
            Some(fields) => {
                assert_eq!(fields[1], "unary");
                assert_eq!(
                    id.x86_system_write(),
                    Some((
                        fields[2],
                        fields[3].parse().unwrap(),
                        fields[4].parse().unwrap()
                    ))
                );
                assert_eq!(id.x86_system_register(), None);
            }
            None => {
                assert_eq!(id.x86_system_register(), None, "{name}");
                assert_eq!(id.x86_system_write(), None, "{name}");
            }
        }
    }
}

#[test]
fn generated_x86_pattern_matches_independent_target_classification() {
    for intrinsic in Intrinsic::ALL {
        assert_eq!(
            matches!(intrinsic, keel_ir::x86_intrinsics!()),
            intrinsic.is_x86_target(),
            "{intrinsic:?}"
        );
    }
}

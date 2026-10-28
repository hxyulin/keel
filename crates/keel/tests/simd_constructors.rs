//! Ordinary C vector literals must reach packed IR before target legalization.
use keel_ir::{InstData, Type, VectorType};

#[test]
fn full_recovers_vector_literals_as_splats_and_shuffles() {
    for target in [
        keel_codegen::Target::X86_64LinuxGnu,
        keel_codegen::Target::Aarch64AppleDarwin,
    ] {
        for (case, expected) in [
            (include_str!("fixtures/simd-reduce.c"), "vsplat"),
            (include_str!("fixtures/simd-shuffle.c"), "vshuffle"),
        ] {
            let mut m = keel_c::compile(
                case,
                "constructors.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            keel_opt::optimize(&mut m).unwrap();
            keel_ir::verify_module(&m).unwrap();
            let f = &m.functions[0];
            let mut loads = 0;
            for (_, i) in f.layout_insts() {
                match f.inst_data(i) {
                    InstData::Load {
                        ty: Type::Vector(VectorType::I32x4),
                        ..
                    } => loads += 1,
                    InstData::Load { .. } => panic!("residual scalar load: {f}"),
                    InstData::Store { addr, .. } => assert_eq!(
                        *addr,
                        f.block_params(f.entry_block().unwrap())[0],
                        "private construction: {f}"
                    ),
                    _ => {}
                }
            }
            assert_eq!(loads, if expected == "vsplat" { 1 } else { 2 }, "{f}");
            assert!(f.to_string().contains(expected), "{f}");
        }
    }
}

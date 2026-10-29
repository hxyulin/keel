use keel_ir::{ConvertOp, InstData as D, Type, VectorType};
use keel_opt::VectorTarget;

const SOURCE: &str = include_str!("../packed_clamp/loops.keel");

fn optimize(target: VectorTarget, pass: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &[pass, "dce"], target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

#[test]
fn packed_clamps_select_in_the_source_ring_and_extend_only_the_result() {
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        let m = optimize(target, "loop-reduce");
        for f in &m.functions {
            let narrow = matches!(
                f.name.as_str(),
                "clamp_bias"
                    | "clamp_signed"
                    | "clamp_unsigned"
                    | "wrap_select"
                    | "shared_wide_arm"
            );
            let mut selects = 0;
            for (_, i) in f.layout_insts() {
                if let D::VectorSelect { args } = *f.inst_data(i) {
                    selects += 1;
                    let shape = if narrow {
                        VectorType::I32x4
                    } else {
                        VectorType::I64x2
                    };
                    assert_eq!(f.value_type(args[0]), Type::Vector(shape), "{}", f.name);
                    assert_eq!(f.value_type(args[1]), Type::Vector(shape), "{}", f.name);
                }
                if let D::Convert {
                    op: ConvertOp::SwidenLow,
                    arg,
                    ..
                } = *f.inst_data(i)
                    && narrow
                {
                    assert!(
                        !matches!(f.value_def(f.resolve(arg)), keel_ir::ValueDef::Result(d, _)
                            if matches!(f.inst_data(d), D::VectorIcmp { .. })),
                        "{}",
                        f.name
                    );
                }
                if let D::Load { ty, .. } = *f.inst_data(i) {
                    assert!(matches!(ty, Type::I32 | Type::I64), "read footprint: {f}");
                }
            }
            assert!(selects > 0, "{}", f.name);
            // Original scalar selects and source-width wrapping stay in the tail.
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), D::Select { .. }))
            );
            if f.name == "wrap_select" || f.name == "clamp_bias" {
                assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i),
                    D::VectorBinary { op: keel_ir::VectorBinaryOp::Iadd, args }
                    if f.value_type(args[0]) == Type::Vector(VectorType::I32x4))));
            }
        }
        let mut again = m.clone();
        keel_opt::run_passes_for_target(&mut again, &["loop-reduce", "dce"], target).unwrap();
        assert_eq!(m.to_string(), again.to_string());
        assert_eq!(m.to_string(), optimize(target, "loop-reduce").to_string());
    }
}

#[test]
fn packed_clamps_leave_fast_and_portable_eligibility_unchanged() {
    for (target, pass) in [
        (VectorTarget::Portable, "loop-reduce"),
        (VectorTarget::Aarch64Neon, "fast-loop-reduce"),
        (VectorTarget::X86Sse2, "fast-loop-reduce"),
    ] {
        let m = optimize(target, pass);
        assert!(m.functions.iter().all(|f| {
            !f.layout_insts()
                .any(|(_, i)| f.inst_data(i).is_vector_instruction())
        }));
    }
}

#[test]
fn packed_clamp_generated_selects_and_extensions_preserve_debug_locations() {
    let mut m = keel_ir::parse(SOURCE).unwrap();
    let file = m.add_file("clamp.c");
    let loc = Some(keel_ir::SourceLoc {
        file,
        line: 17,
        col: 9,
        scope: None,
    });
    for f in &mut m.functions {
        for i in f.layout_insts().map(|(_, i)| i).collect::<Vec<_>>() {
            if matches!(f.inst_data(i), D::Select { .. }) {
                f.set_inst_loc(i, loc);
            }
        }
    }
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce", "dce"], VectorTarget::Aarch64Neon)
        .unwrap();
    let f = &m.functions[0];
    for (_, i) in f.layout_insts() {
        if matches!(
            f.inst_data(i),
            D::VectorSelect { .. }
                | D::Convert {
                    op: ConvertOp::SwidenLow,
                    ..
                }
        ) {
            assert_eq!(f.inst_loc(i), loc);
        }
    }
}

use keel_ir::{InstData, Type, VectorBinaryOp, VectorType};

#[test]
fn vector_values_survive_every_pass_and_the_default_pipeline() {
    let source = "func @vector(ptr, i32) -> i32 {
block0(v0: ptr, v1: i32):
 v2 = load.i32x4 v0
 v3 = vsplat.i32x4 v1
 v4 = viadd v2, v3
 v5 = viadd v2, v3
 v6 = vxor v4, v5
 v7 = vicmp.slt v4, v2
 v8 = vselect v7, v6, v2
 v9 = vshuffle v8, v4, [0, 5, 2, 7]
 store v9, v0
 v10 = vextract v9, 3
 return v10
}";
    for pass in keel_opt::PASSES {
        let mut m = keel_ir::parse(source).unwrap();
        keel_opt::run_passes(&mut m, &[pass.name()]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let text = m.to_string();
        keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    }
    let mut m = keel_ir::parse(source).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(
                f.inst_data(*i),
                InstData::VectorBinary {
                    op: VectorBinaryOp::Iadd,
                    ..
                }
            ))
            .count(),
        1,
        "GVN should share identical vector computations"
    );
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Store { .. })),
        "vector stores are side effects"
    );
}

#[test]
fn vector_memory_forwarding_is_shape_sensitive_and_dce_removes_dead_vectors() {
    let mut m=keel_ir::parse("func @forward(ptr, i32x4) -> f32x4 {\nblock0(v0: ptr, v1: i32x4):\n store v1, v0\n v2 = load.i32x4 v0\n v3 = load.f32x4 v0\n v4 = viadd v2, v2\n return v3\n}").unwrap();
    keel_opt::run_passes(&mut m, &["memory-forward", "dce"]).unwrap();
    let f = &m.functions[0];
    let loads: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| match f.inst_data(i) {
            InstData::Load { ty, .. } => Some(*ty),
            _ => None,
        })
        .collect();
    assert_eq!(loads, vec![Type::Vector(VectorType::F32x4)]);
    assert!(
        !f.layout_insts()
            .any(|(_, i)| f.inst_data(i).is_vector_instruction())
    );
}

#[test]
fn vector_block_arguments_calls_and_bitcasts_remain_valid_after_optimization() {
    let mut m = keel_ir::parse(
        "decl @callee(f32x4) -> f32x4
func @control(i8, ptr, i128) -> f32x4 {
block0(v0: i8, v1: ptr, v2: i128):
 v3 = bitcast.f32x4 v2
 brif v0, block1(v3), block2(v3)
block1(v4: f32x4):
 v5 = load.f32x4 v1
 jump block3(v5)
block2(v6: f32x4):
 v7 = vfadd v6, v6
 jump block3(v7)
block3(v8: f32x4):
 v9 = call @callee(v8)
 return v9
}",
    )
    .unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
}

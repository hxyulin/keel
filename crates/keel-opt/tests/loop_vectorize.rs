use keel_ir::{InstData, Type, VectorType};
use keel_opt::VectorTarget;

fn kernel(ty: Type, index: Type, op: &str, signed: bool) -> String {
    let bytes = ty.bytes().unwrap();
    let extend = if index == Type::I32 {
        " v9 = uext.i64 v5\n"
    } else {
        ""
    };
    let idx = if index == Type::I32 { "v9" } else { "v5" };
    let scale = if bytes == 1 {
        String::new()
    } else {
        format!(" v10 = iconst.i64 {bytes}\n v11 = imul {idx},v10\n")
    };
    let offset = if bytes == 1 { idx } else { "v11" };
    format!(
        "func @map(ptr,ptr,ptr,{index}) {{
block0(v0: ptr,v1: ptr,v2: ptr,v3: {index}):
 v4 = iconst.{index} 0
 jump block1(v4)
block1(v5: {index}):
 v6 = icmp.{} v5,v3
 brif v6,block2,block3
block2:
{extend}{scale} v12 = ptradd v0,{offset}
 v13 = ptradd v1,{offset}
 v14 = ptradd v2,{offset}
 v15 = load.{ty} v13
 v16 = load.{ty} v14
 v17 = {op} v15,v16
 store v17,v12
 v18 = iconst.{index} 1
 v19 = iadd v5,v18
 jump block1(v19)
block3:
 return
}}",
        if signed { "slt" } else { "ult" }
    )
}
fn optimize(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-vectorize"], target).unwrap();
    let text = m.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    m
}
fn packed(m: &keel_ir::Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| m.functions[0].inst_data(*i).is_vector_instruction())
        .count()
}
#[test]
fn vectorizes_counted_maps_preserves_scalar_tail_and_is_deterministic_and_idempotent() {
    // Byte-index addresses are deliberately expressed directly for i8.
    for shape in VectorType::ALL {
        let ty = shape.lane_type();
        let op = if ty.is_float() { "fadd" } else { "iadd" };
        for index in [Type::I32, Type::I64] {
            for signed in [false, true] {
                let src = kernel(ty, index, op, signed);
                let m = optimize(&src, VectorTarget::Portable);
                assert_eq!(packed(&m), 1, "{shape}, {index}, {signed}");
                let text = m.to_string();
                assert!(text.contains(&format!("load.{shape}")));
                assert!(text.contains("load."));
                assert!(m.functions[0].layout_insts().any(
                    |(_, i)| matches!(m.functions[0].inst_data(i),InstData::Load{ty:t,..}if *t==ty)
                ));
                assert_eq!(text, optimize(&src, VectorTarget::Portable).to_string());
                let mut twice = m.clone();
                keel_opt::run_passes(&mut twice, &["loop-vectorize"]).unwrap();
                assert_eq!(text, twice.to_string());
            }
        }
    }
}
#[test]
fn target_cost_gate_avoids_scalarized_integer_multiply() {
    let source = kernel(Type::I32, Type::I64, "imul", false);
    assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 1);
    for target in [VectorTarget::Portable, VectorTarget::X86Sse2] {
        assert_eq!(packed(&optimize(&source, target)), 0);
    }
    let source = kernel(Type::I64, Type::I64, "imul", false);
    assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 0);
}
#[test]
fn rejects_volatile_calls_nonunit_strides_offsets_reductions_and_escaping_values() {
    let source = kernel(Type::I32, Type::I64, "iadd", false);
    let cases = [
        source
            .replace(
                " jump block1(v4)",
                " v50 = iconst.i32 0\n jump block1(v4,v50)",
            )
            .replace("block1(v5: i64)", "block1(v5: i64,v51: i32)")
            .replace("v17 = iadd v15,v16", "v17 = iadd v15,v51")
            .replace("jump block1(v19)", "jump block1(v19,v17)"),
        source.replace("load.i32", "load.i32 volatile"),
        source.replace("store v17", "store volatile v17"),
        format!(
            "decl @effect()\n{}",
            source.replace(" store v17", " call @effect()\n store v17")
        ),
        source.replace("iconst.i64 1\n", "iconst.i64 2\n"),
        source.replace("iconst.i64 4\n", "iconst.i64 8\n"),
        source.replace("icmp.ult", "icmp.ule"),
        source.replace("block3:\n return", "block3:\n store v5,v0\n return"),
        source.replace("v17 = iadd v15,v16", "v17 = udiv v15,v16"),
    ];
    for src in cases {
        let m = optimize(&src, VectorTarget::Aarch64Neon);
        assert_eq!(packed(&m), 0, "{src}");
    }
}

#[test]
fn leaves_known_short_and_negative_loops_unchanged() {
    for n in [0, 1, 15, -1] {
        let source = kernel(Type::I32, Type::I64, "iadd", true)
            .replace(
                " v4 = iconst.i64 0",
                &format!(" v60 = iconst.i64 {n}\n v4 = iconst.i64 0"),
            )
            .replace("icmp.slt v5,v3", "icmp.slt v5,v60");
        assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 0);
    }
}

#[test]
fn ignores_two_instruction_blocks_starting_with_a_store_or_void_call() {
    for first in ["store v2,v0", "call @effect()"] {
        let source = format!(
            "decl @effect()\nfunc @not_a_counted_loop(ptr,i64) {{\nblock0(v0: ptr,v1: i64):\n jump block1(v1)\nblock1(v2: i64):\n {first}\n brif v2,block1(v2),block2\nblock2:\n return\n}}"
        );
        let before = keel_ir::parse(&source).unwrap().to_string();
        let after = optimize(&source, VectorTarget::Portable);
        assert_eq!(before, after.to_string());
    }
}

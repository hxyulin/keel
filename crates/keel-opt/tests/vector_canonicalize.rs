use keel_ir::{EntityRef, InstData as D, Module, Value, VectorType};

fn clean(source: &str) -> Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["instcombine"]).unwrap();
    let combined = m.to_string();
    keel_opt::run_passes(&mut m, &["instcombine"]).unwrap();
    assert_eq!(
        combined,
        m.to_string(),
        "instcombine must reach its fixed point"
    );
    keel_opt::run_passes(&mut m, &["gvn", "instcombine", "dce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let once = m.to_string();
    keel_opt::run_passes(&mut m, &["instcombine", "gvn", "instcombine", "dce"]).unwrap();
    assert_eq!(once, m.to_string(), "canonical cleanup must be idempotent");
    m
}

fn body(args: &str, result: &str, params: &str, body: &str) -> String {
    format!("func @f({args}) -> {result} {{\nblock0({params}):\n{body}\n}}")
}

#[test]
fn lane_transport_all_shapes() {
    for ty in VectorType::ALL {
        let scalar = ty.lane_type();
        let last = ty.lanes() - 1;
        for ops in [
            format!("v2 = vsplat.{ty} v1\nv3 = vextract v2, {last}\nreturn v3"),
            format!("v2 = vinsert v0, v1, {last}\nv3 = vextract v2, {last}\nreturn v3"),
        ] {
            let m = clean(&body(
                &format!("{ty},{scalar}"),
                &scalar.to_string(),
                &format!("v0: {ty},v1: {scalar}"),
                &ops,
            ));
            assert_eq!(m.functions[0].layout_insts().count(), 1);
            assert!(m.to_string().contains("return v1"));
        }
        for ops in [
            format!("v2 = vextract v0, {last}\nv3 = vinsert v0, v2, {last}\nreturn v3"),
            format!(
                "v2 = bitcast.{} v0\nv3 = bitcast.{ty} v2\nreturn v3",
                ty.mask()
            ),
        ] {
            let m = clean(&body(
                &ty.to_string(),
                &ty.to_string(),
                &format!("v0: {ty}"),
                &ops,
            ));
            assert_eq!(m.functions[0].layout_insts().count(), 1);
        }
        let lanes = (0..ty.lanes())
            .map(|i| (i + ty.lanes()).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let m = clean(&body(
            &format!("{ty},{ty}"),
            &ty.to_string(),
            &format!("v0: {ty},v1: {ty}"),
            &format!("v2 = vshuffle v0,v1,[{lanes}]\nreturn v2"),
        ));
        assert_eq!(m.functions[0].layout_insts().count(), 1);
        assert!(m.to_string().contains("return v1"));
    }
}

#[test]
fn inserts_splats_and_vector_copies() {
    for (ops, expected) in [
        (
            "v3 = vinsert v0,v1,2\nv4 = vinsert v3,v2,2\nreturn v4",
            "vinsert v0, v2, 2",
        ),
        (
            "v3 = vinsert v0,v1,1\nv4 = vextract v3,2\nreturn v4",
            "vextract v0, 2",
        ),
        (
            "v3 = vsplat.i32x4 v1\nv4 = vinsert v3,v1,3\nreturn v4",
            "vsplat.i32x4 v1",
        ),
        (
            "v3 = vsplat.i32x4 v1\nv4 = vsplat.i32x4 v1\nv5 = vshuffle v3,v4,[1,4,3,6]\nreturn v5",
            "vsplat.i32x4 v1",
        ),
        (
            "v3 = bitcast.f32x4 v0\nv4 = bitcast.i8x16 v3\nreturn v4",
            "bitcast.i8x16 v0",
        ),
    ] {
        let result = if ops.contains("vextract") {
            "i32"
        } else if ops.contains("i8x16") {
            "i8x16"
        } else {
            "i32x4"
        };
        let m = clean(&body(
            "i32x4,i32,i32",
            result,
            "v0: i32x4,v1: i32,v2: i32",
            ops,
        ));
        assert!(m.to_string().contains(expected), "{m}");
        assert_eq!(m.functions[0].layout_insts().count(), 2, "{m}");
    }
}

#[test]
fn masks_use_nonzero_lanes_and_integer_width() {
    for ty in VectorType::ALL
        .into_iter()
        .filter(|ty| ty.lane_type().is_int())
    {
        for (literal, op, expected) in [
            (0, "vselect v3,v0,v1", "return v1"),
            (1, "vselect v3,v0,v1", "return v0"),
            (-128, "vselect v3,v0,v1", "return v0"),
            (-1, "vand v0,v3", "return v0"),
            (0, "vor v3,v0", "return v0"),
            (0, "vxor v0,v3", "return v0"),
        ] {
            let m = clean(&body(
                &format!("{ty},{ty}"),
                &ty.to_string(),
                &format!("v0: {ty},v1: {ty}"),
                &format!(
                    "v2 = iconst.{} {literal}\nv3 = vsplat.{ty} v2\nv4 = {op}\nreturn v4",
                    ty.lane_type()
                ),
            ));
            assert_eq!(m.functions[0].layout_insts().count(), 1, "{m}");
            assert!(m.to_string().contains(expected), "{m}");
        }
    }
    // 256 truncates to zero in an i8 mask, but remains nonzero in i16.
    for (ty, expected) in [("i8x16", "v1"), ("i16x8", "v0")] {
        let scalar = if ty == "i8x16" { "i8" } else { "i16" };
        let m = clean(&body(
            &format!("{ty},{ty}"),
            ty,
            &format!("v0: {ty},v1: {ty}"),
            &format!(
                "v2 = iconst.{scalar} 256\nv3 = vsplat.{ty} v2\nv4 = vselect v3,v0,v1\nreturn v4"
            ),
        ));
        assert!(m.to_string().contains(&format!("return {expected}")), "{m}");
    }
}

#[test]
fn conversions_compose_only_lossless_low_lane_extensions() {
    for (inner, outer, expected) in [
        ("uwiden_low", "uwiden_low", "uwiden_low"),
        ("uwiden_low", "swiden_low", "uwiden_low"),
        ("swiden_low", "swiden_low", "swiden_low"),
    ] {
        let m = clean(&body(
            "i8x16",
            "i64x2",
            "v0: i8x16",
            &format!("v1 = {inner}.i16x8 v0\nv2 = {outer}.i64x2 v1\nreturn v2"),
        ));
        assert!(
            m.to_string().contains(&format!("{expected}.i64x2 v0")),
            "{m}"
        );
        assert_eq!(m.functions[0].layout_insts().count(), 2);
    }
    for ops in [
        "v1 = swiden_low.i16x8 v0\nv2 = uwiden_low.i64x2 v1\nreturn v2",
        "v1 = uwiden_high.i16x8 v0\nv2 = uwiden_low.i64x2 v1\nreturn v2",
        "v1 = uwiden_low.i64x2 v0\nv2 = narrow_low.i8x16 v1\nreturn v2",
        "v1 = snarrow_sat_low.i8x16 v0\nv2 = swiden_low.i64x2 v1\nreturn v2",
    ] {
        let (input, output) = if ops.contains("snarrow") {
            ("i16x8", "i64x2")
        } else if ops.contains("narrow_low") {
            ("i8x16", "i8x16")
        } else {
            ("i8x16", "i64x2")
        };
        let m = clean(&body(input, output, &format!("v0: {input}"), ops));
        assert_eq!(m.functions[0].layout_insts().count(), 3, "{m}");
    }
}

#[test]
fn bitcast_chains_preserve_legal_scalar_endpoints() {
    for input in ["i64", "f64"] {
        for output in ["i64", "f64"] {
            for vector in ["i64x1", "f64x1", "i8x8"] {
                let m = clean(&body(
                    input,
                    output,
                    &format!("v0: {input}"),
                    &format!("v1 = bitcast.{vector} v0\nv2 = bitcast.{output} v1\nreturn v2"),
                ));
                if input == output {
                    assert_eq!(m.functions[0].layout_insts().count(), 1, "{m}");
                } else {
                    assert_eq!(m.functions[0].layout_insts().count(), 2, "{m}");
                }
            }
        }
    }
}

// A lane oracle independent of the optimizer: track distinct input bit patterns
// through transport operations. No FP calculation or native selector is involved.
fn transport(m: &Module) -> Vec<u64> {
    let f = &m.functions[0];
    let mut values = vec![Vec::<u64>::new(); f.num_values()];
    for (i, &param) in f.block_params(f.entry_block().unwrap()).iter().enumerate() {
        values[param.index()] = (0..4).map(|lane| (i * 100 + lane) as u64).collect();
    }
    for (_, inst) in f.layout_insts() {
        let get = |v: Value| values[f.resolve(v).index()].clone();
        let value = match f.inst_data(inst) {
            D::VectorShuffle { args, lanes } => {
                let all = [get(args[0]), get(args[1])].concat();
                lanes.iter().map(|&i| all[i as usize]).collect()
            }
            D::Return { args } => return get(args[0]),
            _ => panic!("unexpected oracle operation"),
        };
        values[f.first_result(inst).index()] = value;
    }
    panic!("no return")
}

#[test]
fn shuffle_composition_matches_lane_oracle() {
    // All 4096 outer maps, using two different inner maps. Includes repeats,
    // permutations, identity, and selections from both halves of each input.
    for encoding in 0..4096 {
        let lanes = (0..4)
            .map(|i| (encoding >> (i * 3)) & 7)
            .collect::<Vec<_>>();
        let source = body(
            "i32x4,i32x4",
            "i32x4",
            "v0: i32x4,v1: i32x4",
            &format!(
                "v2 = vshuffle v0,v1,[3,4,1,6]\nv3 = vshuffle v1,v0,[5,2,7,0]\nv4 = vshuffle v2,v3,{lanes:?}\nreturn v4"
            ),
        );
        let original = keel_ir::parse(&source).unwrap();
        let optimized = clean(&source);
        assert_eq!(
            transport(&original),
            transport(&optimized),
            "{source}\n{optimized}"
        );
        assert!(
            optimized.functions[0].layout_insts().count() <= 4,
            "{optimized}"
        );
    }
}

#[test]
fn nested_shuffles_only_compose_to_existing_sources() {
    for ty in VectorType::ALL {
        let n = ty.lanes();
        let reverse = (0..n).rev().collect::<Vec<_>>();
        let m = clean(&body(
            &ty.to_string(),
            &ty.to_string(),
            &format!("v0: {ty}"),
            &format!("v1 = vshuffle v0,v0,{reverse:?}\nv2 = vshuffle v1,v0,{reverse:?}\nreturn v2"),
        ));
        assert!(m.to_string().contains("return v0"), "{m}");
        assert_eq!(m.functions[0].layout_insts().count(), 1);
    }
    // Preserve both one-source and two-source nontrivial compositions.
    for ops in [
        "v2 = vshuffle v0,v0,[3,2,1,0]\nv3 = vshuffle v2,v0,[0,0,1,1]\nreturn v3",
        "v2 = vshuffle v0,v1,[0,1,4,5]\nv3 = vshuffle v0,v1,[2,3,6,7]\nv4 = vshuffle v2,v3,[0,2,4,6]\nreturn v4",
    ] {
        let source = body("i32x4,i32x4", "i32x4", "v0: i32x4,v1: i32x4", ops);
        assert_eq!(
            keel_ir::parse(&source).unwrap().to_string(),
            clean(&source).to_string()
        );
    }
}

#[test]
fn shared_shuffles_and_three_sources_remain_bounded() {
    for source in [
        body(
            "i32x4,i32x4,i32x4",
            "i32x4",
            "v0: i32x4,v1: i32x4,v2: i32x4",
            "v3 = vshuffle v0,v1,[0,5,2,7]\nv4 = vshuffle v3,v2,[0,1,6,7]\nreturn v4",
        ),
        body(
            "i32x4,i32x4,ptr",
            "i32x4",
            "v0: i32x4,v1: i32x4,v2: ptr",
            "v3 = vshuffle v0,v1,[0,5,2,7]\nstore v3,v2\nv4 = vshuffle v3,v0,[0,1,6,7]\nreturn v4",
        ),
    ] {
        let m = clean(&source);
        assert_eq!(
            m.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), D::VectorShuffle { .. }))
                .count(),
            2,
            "{m}"
        );
    }
}

#[test]
fn fp_arithmetic_arbitrary_masks_and_effect_order_are_preserved() {
    let source = "func @effects(ptr,ptr,f32x4,i32x4) -> f32x4 {
block0(v0: ptr,v1: ptr,v2: f32x4,v3: i32x4):
 v4 = load.f32x4 align 1 v0
 v5 = load.f32x4 volatile v1
 v6 = atomic_load.i32 acquire v0
 v7 = vfadd v2,v4
 v8 = vfmul v7,v5
 v9 = vselect v3,v8,v2
 store volatile v9,v1
 return v9
}";
    let original = keel_ir::parse(source).unwrap();
    let m = clean(source);
    assert_eq!(original.to_string(), m.to_string());
    // A bit mask of 1 is not an identity AND. Different insertion lanes matter.
    for (ops, remaining) in [
        (
            "v2 = iconst.i32 1\nv3 = vsplat.i32x4 v2\nv4 = vand v0,v3\nreturn v4",
            "vand",
        ),
        (
            "v2 = vextract v0,0\nv3 = vinsert v0,v2,1\nreturn v3",
            "vinsert",
        ),
    ] {
        let m = clean(&body("i32x4", "i32x4", "v0: i32x4", ops));
        assert!(m.to_string().contains(remaining), "{m}");
    }
}

#[test]
fn fast_local_reuses_bounded_rules_without_composing_shared_shuffles() {
    let mut m = keel_ir::parse(&body("f32x4", "f32x4", "v0: f32x4",
        "v1 = vextract v0,2\nv2 = vinsert v0,v1,2\nv3 = bitcast.i32x4 v2\nv4 = bitcast.f32x4 v3\nreturn v4")).unwrap();
    keel_opt::run_passes(&mut m, &["fast-local", "dce"]).unwrap();
    assert_eq!(m.functions[0].layout_insts().count(), 1);
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn identity_of_shuffle_keeps_written_source_order() {
    let m = clean(&body(
        "i32x4,i32x4",
        "i32x4",
        "v0: i32x4,v1: i32x4",
        "v2 = vshuffle v0,v1,[4,1,6,3]\nv3 = vshuffle v2,v0,[0,1,2,3]\nreturn v3",
    ));
    assert!(
        m.to_string().contains("vshuffle v0, v1, [4, 1, 6, 3]"),
        "{m}"
    );
}

#[test]
fn extracts_follow_shuffles_and_revisit_out_of_layout_consumers() {
    for ty in VectorType::ALL {
        let n = ty.lanes();
        let map = (0..n)
            .map(|i| (2 * n - 1 - i).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let m = clean(&body(
            &format!("{ty},{ty}"),
            &ty.lane_type().to_string(),
            &format!("v0: {ty},v1: {ty}"),
            &format!("v2 = vshuffle v0,v1,[{map}]\nv3 = vextract v2,0\nreturn v3"),
        ));
        assert!(
            m.to_string().contains(&format!("vextract v1, {}", n - 1)),
            "{m}"
        );
        assert_eq!(m.functions[0].layout_insts().count(), 2);
    }
    let m = clean(
        "func @f(i32) -> i32 {
block0(v0: i32):
 jump block2
block1:
 v3 = vextract v2,3
 return v3
block2:
 v1 = vsplat.i32x4 v0
 v2 = vinsert v1,v0,1
 jump block1
}",
    );
    assert!(m.to_string().contains("return v0"), "{m}");
    assert!(!m.to_string().contains("vsplat"), "{m}");
}

// Evaluate scalar sums independently of recognition, including overflow bits.
fn sum_result(m: &Module, input: [u32; 4]) -> u32 {
    let f = &m.functions[0];
    let mut values = vec![0u32; f.num_values()];
    for (_, inst) in f.layout_insts() {
        let get = |v| values[f.resolve(v).index()];
        let result = match *f.inst_data(inst) {
            D::VectorExtract { lane, .. } => input[lane as usize],
            D::Binary {
                op: keel_ir::BinaryOp::Iadd,
                args,
            } => get(args[0]).wrapping_add(get(args[1])),
            D::VectorReduce {
                op: keel_ir::VectorReduceOp::Add,
                ..
            } => input.into_iter().fold(0u32, u32::wrapping_add),
            D::Return { ref args } => return get(args[0]),
            ref other => panic!("unexpected sum operation: {other:?}"),
        };
        values[f.first_result(inst).index()] = result;
    }
    panic!("no return")
}

#[test]
fn complete_i32_lane_sums_match_wrapping_oracle() {
    // All permutations, all five binary tree shapes. Input bits
    // include signed extremes and overflow; signedness is absent from IR iadd.
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    if [a, b, c, d]
                        .iter()
                        .copied()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        != 4
                    {
                        continue;
                    }
                    for tree in [
                        "v5 = iadd v1,v2\nv6 = iadd v5,v3\nv7 = iadd v6,v4",
                        "v5 = iadd v3,v4\nv6 = iadd v2,v5\nv7 = iadd v1,v6",
                        "v5 = iadd v1,v2\nv6 = iadd v3,v4\nv7 = iadd v5,v6",
                        "v5 = iadd v2,v3\nv6 = iadd v1,v5\nv7 = iadd v6,v4",
                        "v5 = iadd v2,v3\nv6 = iadd v5,v4\nv7 = iadd v1,v6",
                    ] {
                        let source = body(
                            "i32x4",
                            "i32",
                            "v0: i32x4",
                            &format!(
                                "v1 = vextract v0,{a}\nv2 = vextract v0,{b}\nv3 = vextract v0,{c}\nv4 = vextract v0,{d}\n{tree}\nreturn v7"
                            ),
                        );
                        let original = keel_ir::parse(&source).unwrap();
                        let optimized = clean(&source);
                        let mut fast = original.clone();
                        keel_opt::run_passes(&mut fast, &["fast-local", "dce"]).unwrap();
                        assert!(!fast.to_string().contains("vreduce_add"));
                        assert!(
                            optimized.to_string().contains("vreduce_add v0"),
                            "{optimized}"
                        );
                        assert_eq!(optimized.functions[0].layout_insts().count(), 2);
                        let mut seed = 0x12345678u32;
                        for _ in 0..256 {
                            let input = std::array::from_fn(|_| {
                                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                                seed
                            });
                            assert_eq!(sum_result(&original, input), sum_result(&optimized, input));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn incomplete_mixed_promoted_shared_and_fp_sums_stay_scalar() {
    let extracts = "v2 = vextract v0,0\nv3 = vextract v0,1\nv4 = vextract v0,2\nv5 = vextract v0,3";
    for tail in [
        "v6 = iadd v2,v3\nv7 = iadd v6,v4\nreturn v7", // missing lane
        "v6 = iadd v2,v3\nv7 = iadd v6,v4\nv8 = iadd v7,v4\nreturn v8", // repeated lane
        "v6 = iadd v2,v3\nv7 = iadd v6,v4\nstore v6,v1\nv8 = iadd v7,v5\nreturn v8", // shared sum
        "store v2,v1\nv6 = iadd v2,v3\nv7 = iadd v6,v4\nv8 = iadd v7,v5\nreturn v8", // shared extract
        "v6 = vextract v1,3\nv7 = iadd v2,v3\nv8 = iadd v7,v4\nv9 = iadd v8,v6\nreturn v9", // mixed vectors
    ] {
        let arg = if tail.contains("vextract v1") {
            "i32x4"
        } else {
            "ptr"
        };
        let source = body(
            &format!("i32x4,{arg}"),
            "i32",
            &format!("v0: i32x4,v1: {arg}"),
            &format!("{extracts}\n{tail}"),
        );
        assert!(!clean(&source).to_string().contains("vreduce_add"));
    }
    for (vector, scalar, cast, op) in [
        ("i16x8", "i32", "sext.i32", "iadd"),
        ("i32x4", "i64", "uext.i64", "iadd"),
        ("f32x4", "f32", "", "fadd"),
    ] {
        let mut ops = String::new();
        for lane in 0..4 {
            ops += &format!("v{} = vextract v0,{lane}\n", lane + 1);
            if !cast.is_empty() {
                ops += &format!("v{} = {cast} v{}\n", lane + 5, lane + 1);
            }
        }
        let start = if cast.is_empty() { 1 } else { 5 };
        ops += &format!(
            "v9 = {op} v{start},v{}\nv10 = {op} v9,v{}\nv11 = {op} v10,v{}\nreturn v11",
            start + 1,
            start + 2,
            start + 3
        );
        let source = body(vector, scalar, &format!("v0: {vector}"), &ops);
        assert!(!clean(&source).to_string().contains("vreduce_add"));
    }
}

#[test]
fn lane_sum_resolved_sources_effects_and_result_extensions() {
    let source = "decl @observe(ptr)
func @f(ptr,ptr) -> i64 {
block0(v0: ptr,v1: ptr):
 v2 = load.i32x4 volatile align 1 v0
 v3 = bitcast.i32x4 v2
 v4 = vextract v2,3
 store volatile v4,v1
 v5 = vextract v3,0
 v6 = vextract v2,1
 v7 = iadd v5,v6
 call @observe(v1)
 v8 = load.i32 volatile v1
 v9 = vextract v3,2
 v10 = vextract v2,3
 v11 = iadd v9,v10
 v12 = iadd v7,v11
 v13 = sext.i64 v12
 v14 = uext.i64 v12
 v15 = iadd v13,v14
 store volatile v8,v1
 return v15
}";
    let mut m = keel_ir::parse(source).unwrap();
    m.files.push("effects.c".into());
    // Test alias identity before canonicalization has rewritten operand IDs.
    m.functions[0].replace_all_uses(Value::new(3), Value::new(2));
    let f = &mut m.functions[0];
    let insts = f.layout_insts().map(|(_, i)| i).collect::<Vec<_>>();
    for (line, i) in insts.into_iter().enumerate() {
        f.set_inst_loc(
            i,
            Some(keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(0),
                line: line as u32 + 1,
                col: 1,
                scope: None,
            }),
        );
    }
    // Instcombine replaces the root in place and retains every effect, its
    // operands/flags, layout order and debug location. Root users stay i64
    // sign/zero extensions of the same i32 result, even with two consumers.
    let effects = |m: &Module| {
        m.functions[0]
            .layout_insts()
            .filter_map(|(_, i)| {
                let f = &m.functions[0];
                matches!(
                    f.inst_data(i),
                    D::Load { .. } | D::Store { .. } | D::Call { .. }
                )
                .then(|| (i, f.inst_data(i).clone(), f.inst_loc(i)))
            })
            .collect::<Vec<_>>()
    };
    let before = effects(&m);
    let root = m.functions[0]
        .layout_insts()
        .find(|(_, i)| m.functions[0].inst_results(*i).contains(&Value::new(12)))
        .unwrap()
        .1;
    let loc = m.functions[0].inst_loc(root);
    keel_opt::run_passes(&mut m, &["instcombine"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(before, effects(&m));
    assert_eq!(loc, m.functions[0].inst_loc(root));
    assert!(m.to_string().contains("v12 = vreduce_add v2"), "{m}");
    assert!(m.to_string().contains("sext.i64 v12"), "{m}");
    assert!(m.to_string().contains("uext.i64 v12"), "{m}");
}

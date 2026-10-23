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
            optimized.functions[0].layout_insts().count() <= 2,
            "{optimized}"
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
fn composed_shuffle_keeps_written_source_order() {
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

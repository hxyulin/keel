//! Recover complete private vector constructors without changing their memory effects.
use keel_ir::{EntityRef, InstData, Module, Type, Value, VectorType};

const STORES: &str = "store v10, v3\n store v11, v5\n store v12, v7\n store v13, v9";
fn fixture() -> String {
    format!(
        "decl @effect(ptr)
func @f(ptr, ptr, ptr) {{
 ss0 = stack_slot 16, align 16
block0(v0: ptr, v1: ptr, v2: ptr):
 v3 = stack_addr ss0
 v4 = iconst.i64 4
 v5 = ptradd v3, v4
 v6 = iconst.i64 8
 v7 = ptradd v3, v6
 v8 = iconst.i64 12
 v9 = ptradd v3, v8
 v14 = load.i32x4 align 1 v0
 v15 = load.i32x4 align 1 v1
 v10 = vextract v14, 3
 v11 = vextract v15, 0
 v12 = vextract v14, 1
 v13 = vextract v15, 2
 {STORES}
 v16 = load.i32x4 align 16 v3
 store align 1 v16, v2
 return
}}"
    )
}
fn parse(s: &str) -> Module {
    let m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn pass(m: &mut Module) -> bool {
    let changed = keel_opt::find_pass("stack-forward")
        .unwrap()
        .run(&mut m.functions[0]);
    keel_ir::verify_module(m).unwrap();
    changed
}
fn recovered(m: &Module) -> Option<&InstData> {
    let f = &m.functions[0];
    f.layout_insts().find_map(|(_, i)| {
        matches!(
            f.inst_data(i),
            InstData::VectorSplat { .. } | InstData::VectorShuffle { .. }
        )
        .then(|| f.inst_data(i))
    })
}
#[test]
fn final_byte_images_and_barriers() {
    for (name, before, after, expected) in [
        ("ordinary", "", "", true),
        (
            "zeroed first",
            "v20 = iconst.i64 0\n store v20, v3\n store v20, v7",
            "",
            true,
        ),
        ("repair torn bytes", "store v6, v5", "", true),
        ("external effect", "", "call @effect(v0)", true),
        ("shared snapshot", "", "store v14, v2", true),
        ("torn lane", "", "store v6, v5", false),
        (
            "byte overwrite",
            "",
            "v20 = iconst.i8 0\n store v20, v3",
            false,
        ),
        ("volatile store", "", "store volatile v10, v3", false),
        (
            "atomic access",
            "",
            "v20 = atomic_load.i32 relaxed v3",
            false,
        ),
        ("escape", "", "call @effect(v3)", false),
        ("address conversion", "", "v20 = ptrtoint.i64 v3", false),
        (
            "frame observer",
            "",
            "v20 = intrinsic frame_address()",
            false,
        ),
        (
            "lifetime",
            "",
            "v20 = intrinsic stack_save()\n intrinsic stack_restore(v20)",
            false,
        ),
    ] {
        let s = fixture().replace(STORES, &format!("{before}\n {STORES}\n {after}"));
        let mut m = parse(&s);
        pass(&mut m);
        assert_eq!(recovered(&m).is_some(), expected, "{name}\n{m}");
        if expected {
            let InstData::VectorShuffle { lanes, .. } = recovered(&m).unwrap() else {
                panic!("{m}")
            };
            assert_eq!(lanes, &[3, 4, 1, 6]);
        }
        let once = m.to_string();
        assert!(!pass(&mut m), "second pass: {name}");
        assert_eq!(m.to_string(), once);
    }
}
#[test]
fn incomplete_or_unsupported_constructors_keep_memory() {
    for (from, to) in [
        ("store v13, v9", ""),
        ("v16 = load.i32x4", "v16 = load.i32x4 volatile"),
        ("stack_slot 16", "stack_slot 32"),
        (
            "v16 = load.i32x4 align 16 v3",
            "v16 = load.i32x4 align 1 v5",
        ),
        (
            "v13 = vextract v15, 2",
            "v20 = load.i32x4 v2\n v13 = vextract v20, 2",
        ),
        ("v13 = vextract v15, 2", "v13 = iconst.i32 7"),
        ("v16 = load", "jump block1\nblock1:\n v16 = load"),
        ("decl @effect(ptr)", "decl @effect(ptr) returns_twice"),
    ] {
        let mut s = fixture().replace(from, to);
        if to.contains("returns_twice") {
            s = s.replace("v16 = load", "call @effect(v0)\n v16 = load");
        }
        let mut m = parse(&s);
        pass(&mut m);
        assert!(recovered(&m).is_none(), "{from} -> {to}\n{m}");
    }
}
#[test]
fn splats_aliases_locations_and_residual_reads() {
    for splat in [false, true] {
        let s = if splat {
            fixture()
                .replace("store v11,", "store v10,")
                .replace("store v12,", "store v10,")
                .replace("store v13,", "store v10,")
        } else {
            fixture()
        };
        let mut m = parse(&s);
        m.files.push("constructor.c".into());
        let f = &mut m.functions[0];
        // Aliases are resolved for byte facts and immutable vector producers.
        let source = f
            .layout_insts()
            .find(|&(_, i)| {
                matches!(
                    f.inst_data(i),
                    InstData::Load {
                        ty: Type::Vector(_),
                        ..
                    }
                )
            })
            .unwrap()
            .1;
        let copy = f.insert_inst(
            keel_ir::InsertPoint::Before(source),
            f.inst_data(source).clone(),
        );
        f.replace_all_uses(f.first_result(source), f.first_result(copy));
        let reload = f
            .layout_insts()
            .filter(|&(_, i)| matches!(f.inst_data(i), InstData::Load { .. }))
            .last()
            .unwrap()
            .1;
        let loc = Some(keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 7,
            col: 3,
            scope: None,
        });
        f.set_inst_loc(reload, loc);
        assert!(pass(&mut m));
        let f = &m.functions[0];
        let (_, i) = f
            .layout_insts()
            .find(|&(_, i)| {
                matches!(
                    f.inst_data(i),
                    InstData::VectorSplat { .. } | InstData::VectorShuffle { .. }
                )
            })
            .unwrap();
        assert_eq!(f.inst_loc(i), loc);
        assert_eq!(
            matches!(recovered(&m), Some(InstData::VectorSplat { .. })),
            splat
        );
        assert!(!f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Store { addr, .. } if *addr != Value::new(2))));
    }
    let mut m = parse(&fixture().replace(
        "return",
        "jump block1\nblock1:\n v20 = load.ptr v3\n call @effect(v20)\n return",
    ));
    assert!(pass(&mut m));
    assert!(recovered(&m).is_some());
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|&(_, i)| matches!(m.functions[0].inst_data(i), InstData::Store { .. }))
            .count(),
        5
    );
}
#[test]
fn scalar_parameters_and_single_source_permutations() {
    let s = "func @f(i32) -> i32x4 {\n ss0 = stack_slot 16, align 16\nblock0(v0: i32):\n v1 = stack_addr ss0\n v2 = iconst.i64 4\n v3 = ptradd v1,v2\n v4 = iconst.i64 8\n v5 = ptradd v1,v4\n v6 = iconst.i64 12\n v7 = ptradd v1,v6\n store v0,v1\n store v0,v3\n store v0,v5\n store v0,v7\n v8 = load.i32x4 v1\n return v8\n}";
    let mut m = parse(s);
    assert!(pass(&mut m));
    assert!(
        matches!(recovered(&m), Some(InstData::VectorSplat { ty: Type::Vector(VectorType::I32x4), arg }) if *arg == Value::new(0))
    );
    let mut m = parse(&fixture().replace("vextract v15", "vextract v14"));
    assert!(pass(&mut m));
    let Some(InstData::VectorShuffle { args, lanes }) = recovered(&m) else {
        panic!("{m}")
    };
    assert_eq!(args[0], args[1]);
    assert_eq!(lanes, &[3, 0, 1, 2]);
}

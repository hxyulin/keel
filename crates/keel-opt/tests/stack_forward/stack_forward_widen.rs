//! Final byte facts, immutable snapshots, and conservative ownership for low widening.
use keel_ir::{ConvertOp, EntityRef, Inst, InstData, Module, Type, Value, VectorType};

const PAIR: &str = "store v6, v2\n store v8, v4";
fn fixture() -> String {
    format!(
        "decl @effect(ptr)\nfunc @f(ptr, ptr) {{
 ss0 = stack_slot 16, align 16
block0(v0: ptr, v1: ptr):
 v2 = stack_addr ss0
 v3 = iconst.i64 8
 v4 = ptradd v2, v3
 v5 = load.i32x4 align 1 v0
 v10 = vextract v5, 0
 v6 = sext.i64 v10
 v7 = vextract v5, 1
 v8 = sext.i64 v7
 {PAIR}
 v9 = load.i64x2 align 16 v2
 store align 1 v9, v1
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
    keel_opt::find_pass("stack-forward")
        .unwrap()
        .run(&mut m.functions[0])
}
fn widen(m: &Module) -> Option<Inst> {
    let f = &m.functions[0];
    f.layout_insts().find_map(|(_, i)| {
        matches!(
            f.inst_data(i),
            InstData::Convert {
                op: ConvertOp::SwidenLow,
                ..
            }
        )
        .then_some(i)
    })
}
fn count(m: &Module, kind: fn(&InstData) -> bool) -> usize {
    let f = &m.functions[0];
    f.layout_insts()
        .filter(|&(_, i)| kind(f.inst_data(i)))
        .count()
}

#[test]
fn final_bytes_matrix() {
    // 35 bounded scenarios; exact forwarding is tested separately.
    let cases = [
        ("pair", "", "", true),
        ("overwritten zero", "store v3, v2\n store v3, v4", "", true),
        ("repaired torn", "store v7, v2", "", true),
        (
            "repaired overlap",
            "v20 = iconst.i64 4\n v21 = ptradd v2, v20\n store v6, v21",
            "",
            true,
        ),
        ("ordinary external call", "", "call @effect(v0)", true),
        (
            "shared extract",
            "call @effect(v0)",
            "v20 = sext.i64 v7\n store v20, v1",
            true,
        ),
        ("shared extension", "", "store v6, v1", true),
        ("shared source", "", "store v5, v1", true),
        ("torn", "", "store v7, v2", false),
        (
            "final overlap",
            "",
            "v20 = iconst.i64 4\n v21 = ptradd v2, v20\n store v6, v21",
            false,
        ),
        ("volatile store", "", "store volatile v6, v2", false),
        ("atomic load", "", "v20 = atomic_load.i64 relaxed v2", false),
        ("escape after", "", "call @effect(v2)", false),
        ("escape before", "call @effect(v2)", "", false),
        ("stored address", "", "store v2, v1", false),
        ("converted address", "", "v20 = ptrtoint.i64 v2", false),
        (
            "frame observer",
            "",
            "v20 = intrinsic frame_address()",
            false,
        ),
        (
            "opaque marks",
            "",
            "v20 = intrinsic stack_save()\n intrinsic stack_restore(v20)",
            false,
        ),
        ("missing pair", "REMOVE_PAIR", "", false),
        ("one half", "HALF_PAIR", "", false),
        ("omitted zero lane", "ZERO_HALF", "", false),
        ("unsigned", "UEXT", "", false),
        ("high", "HIGH", "", false),
        ("reversed", "REVERSE", "", false),
        ("repeated", "REPEAT", "", false),
        ("mixed source", "MIXED", "", false),
        ("short source", "SHORT", "", false),
        ("extra cast", "CAST", "", false),
        ("wrong slot", "SLOT", "", false),
        ("offset reload", "OFFSET", "", false),
        ("volatile reload", "VOLATILE", "", false),
        ("returns twice", "TWICE", "", false),
        ("cross block", "SPLIT", "", false),
        ("read before write", "EARLY", "", false),
        ("atomic write", "", "atomic_store relaxed v6, v2", false),
    ];
    assert_eq!(cases.len(), 35);
    for (name, before, after, expected) in cases {
        let mut s = fixture();
        s = match before {
            "REMOVE_PAIR" => s.replace(PAIR, ""),
            "HALF_PAIR" => s.replace(PAIR, "store v6, v2"),
            "ZERO_HALF" => s.replace(PAIR, "store v6, v2\n store v3, v4"),
            "UEXT" => s.replace("sext.i64", "uext.i64"),
            "HIGH" => s.replace("v5, 0", "v5, 2").replace("v5, 1", "v5, 3"),
            "REVERSE" => s
                .replace("v5, 0", "v5, 3")
                .replace("v5, 1", "v5, 0")
                .replace("v5, 3", "v5, 1"),
            "REPEAT" => s.replace("v5, 1", "v5, 0"),
            "MIXED" => s.replace(
                "v7 = vextract v5",
                "v20 = load.i32x4 v0\n v7 = vextract v20",
            ),
            "SHORT" => s.replace("load.i32x4", "load.i16x8"),
            "CAST" => s.replace(
                "v8 = sext.i64 v7",
                "v20 = sext.i64 v7\n v21 = trunc.i32 v20\n v8 = sext.i64 v21",
            ),
            "SLOT" => s.replace("stack_slot 16", "stack_slot 32"),
            "OFFSET" => s
                .replace("load.i64x2 align 16 v2", "load.i64x2 align 1 v4")
                .replace("stack_slot 16", "stack_slot 24"),
            "VOLATILE" => s.replace("load.i64x2", "load.i64x2 volatile"),
            "TWICE" => s
                .replace("decl @effect(ptr)", "decl @effect(ptr) returns_twice")
                .replace(PAIR, &format!("{PAIR}\n call @effect(v0)")),
            "SPLIT" => s.replace("v9 = load", "jump block1\nblock1:\n v9 = load"),
            "EARLY" => s.replace(PAIR, "").replace(
                "store align 1 v9, v1",
                &format!("{PAIR}\n store align 1 v9, v1"),
            ),
            _ => s.replace(PAIR, &format!("{before}\n {PAIR}\n {after}")),
        };
        // The late escape is genuinely after the candidate reload.
        if name == "escape after" {
            s = s
                .replace("call @effect(v2)", "")
                .replace("return", "call @effect(v2)\n return");
        }
        let mut m = parse(&s);
        let before = m.to_string();
        pass(&mut m);
        if !expected {
            assert_eq!(before, m.to_string(), "failed match: {name}");
        }
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(widen(&m).is_some(), expected, "{name}\n{m}");
        let once = m.to_string();
        assert!(!pass(&mut m), "second pass: {name}");
        assert_eq!(once, m.to_string());
        if expected {
            assert_eq!(
                count(&m, |d| matches!(d, InstData::Load { .. })),
                1,
                "{name}"
            );
        }
    }
}

#[test]
fn external_effect_identity_location_and_full_fixed_point() {
    let s = fixture()
        .replace("v9 = load", "call @effect(v0)\n v9 = load")
        .replace("return", "store align 1 v9, v1\n return");
    let mut m = parse(&s);
    let f = &mut m.functions[0];
    let effects: Vec<_> = f
        .layout_insts()
        .filter(|&(_, i)| match f.inst_data(i) {
            InstData::Call { .. }
            | InstData::Load {
                ty: Type::Vector(VectorType::I32x4),
                ..
            } => true,
            InstData::Store { addr, .. } => *addr == Value::new(1),
            _ => false,
        })
        .map(|(_, i)| (i, f.inst_data(i).clone()))
        .collect();
    let load = f
        .layout_insts()
        .find_map(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Load {
                    ty: Type::Vector(VectorType::I64x2),
                    ..
                }
            )
            .then_some(i)
        })
        .unwrap();
    m.files.push("widen.c".into());
    let file = keel_ir::SourceFile::new(0);
    let loc = Some(keel_ir::SourceLoc {
        file,
        line: 17,
        col: 3,
        scope: None,
    });
    m.functions[0].set_inst_loc(load, loc);
    assert!(pass(&mut m));
    let f = &m.functions[0];
    assert_eq!(f.inst_loc(widen(&m).unwrap()), loc);
    let live: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    let mut previous = None;
    for (i, data) in effects {
        assert_eq!(f.inst_data(i), &data);
        let position = live.iter().position(|&j| i == j).unwrap();
        assert!(previous.is_none_or(|p| p < position));
        previous = Some(position);
    }
    assert_eq!(count(&m, |d| matches!(d, InstData::Store { .. })), 2);
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let normalized = keel_ir::parse(&m.to_string()).unwrap().to_string();
    keel_opt::optimize(&mut m).unwrap();
    assert_eq!(
        normalized,
        keel_ir::parse(&m.to_string()).unwrap().to_string()
    );
}

#[test]
fn residual_read_keeps_all_stores_and_exact_forwarding_stays_first() {
    let s = fixture().replace(
        "return",
        "jump block1\nblock1:\n v20 = load.ptr v2\n call @effect(v20)\n return",
    );
    let mut m = parse(&s);
    assert!(pass(&mut m));
    assert!(widen(&m).is_some());
    assert_eq!(count(&m, |d| matches!(d, InstData::Store { .. })), 3);
    keel_ir::verify_module(&m).unwrap();
    let s = fixture().replace(
        "v9 = load",
        "v20 = load.i64x2 v0\n store v20, v2\n v9 = load",
    );
    let mut m = parse(&s);
    assert!(pass(&mut m));
    assert!(widen(&m).is_none());
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn aliases_live_parameters_and_detached_producers() {
    for kind in [
        "stored",
        "extract",
        "source",
        "address",
        "reload",
        "detached sext",
        "detached extract",
        "detached source",
        "parameter",
        "debug",
    ] {
        let mut m = parse(&fixture());
        let f = &mut m.functions[0];
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        let source = insts
            .iter()
            .copied()
            .find(|&i| {
                matches!(
                    f.inst_data(i),
                    InstData::Load {
                        ty: Type::Vector(VectorType::I32x4),
                        ..
                    }
                )
            })
            .unwrap();
        let extract = insts
            .iter()
            .copied()
            .find(|&i| matches!(f.inst_data(i), InstData::VectorExtract { lane: 0, .. }))
            .unwrap();
        let sext = insts
            .iter()
            .copied()
            .find(|&i| {
                matches!(
                    f.inst_data(i),
                    InstData::Convert {
                        op: ConvertOp::Sext,
                        ..
                    }
                )
            })
            .unwrap();
        let reload = insts
            .iter()
            .copied()
            .find(|&i| {
                matches!(
                    f.inst_data(i),
                    InstData::Load {
                        ty: Type::Vector(VectorType::I64x2),
                        ..
                    }
                )
            })
            .unwrap();
        match kind {
            "stored" | "extract" | "source" | "address" => {
                let i = match kind {
                    "stored" => sext,
                    "extract" => extract,
                    "source" => source,
                    _ => insts[0],
                };
                let copy = f.insert_inst(keel_ir::InsertPoint::Before(i), f.inst_data(i).clone());
                f.replace_all_uses(f.first_result(i), f.first_result(copy));
            }
            "reload" => {
                let other = f.insert_inst(
                    keel_ir::InsertPoint::Before(reload),
                    InstData::Load {
                        ty: Type::Vector(VectorType::I64x2),
                        addr: Value::new(0),
                        flags: Default::default(),
                    },
                );
                f.replace_all_uses(f.first_result(reload), f.first_result(other));
            }
            "detached sext" => f.remove_inst(sext),
            "detached extract" => f.remove_inst(extract),
            "detached source" => f.remove_inst(source),
            "parameter" => {
                let param =
                    f.append_block_param(f.entry_block().unwrap(), Type::Vector(VectorType::I32x4));
                f.sig
                    .params
                    .push(keel_ir::AbiParam::new(Type::Vector(VectorType::I32x4)));
                f.replace_all_uses(f.first_result(source), param);
            }
            "debug" => {
                m.debug_types.push(keel_ir::DebugTypeData::Base {
                    name: "home".into(),
                    encoding: keel_ir::Encoding::Signed,
                    size: 16,
                });
                m.functions[0].debug_vars.push(keel_ir::DebugVar {
                    name: "home".into(),
                    ty: keel_ir::DebugType::new(0),
                    slot: keel_ir::StackSlot::new(0),
                    indirect: false,
                    param: false,
                    scope: None,
                });
            }
            _ => unreachable!(),
        }
        let malformed = kind.starts_with("detached");
        if !malformed {
            keel_ir::verify_module(&m).unwrap();
        }
        let before = m.to_string();
        let matched = !malformed && kind != "reload" && kind != "debug";
        assert_eq!(pass(&mut m), matched, "{kind}");
        assert_eq!(widen(&m).is_some(), matched, "{kind}");
        if matched {
            keel_ir::verify_module(&m).unwrap();
        } else {
            assert_eq!(before, m.to_string(), "{kind}");
        }
    }
}

#[test]
fn shared_pure_producers_survive_cleanup_and_full_can_merge_blocks() {
    for used in ["v10", "v6", "v5"] {
        let s = fixture().replace("return", &format!("store {used}, v1\n return"));
        let mut m = parse(&s);
        let f = &m.functions[0];
        let returned = f
            .layout_insts()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .find_map(|(_, i)| match *f.inst_data(i) {
                InstData::Store { value, .. } => Some(value),
                _ => None,
            })
            .unwrap();
        keel_opt::run_passes(&mut m, &["stack-forward", "dce"]).unwrap();
        assert!(widen(&m).is_some());
        assert!(m.functions[0].is_value_defined(returned));
        keel_ir::verify_module(&m).unwrap();
    }
    let s = fixture().replace("v9 = load", "jump block1\nblock1:\n v9 = load");
    let mut m = parse(&s);
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(widen(&m).is_some(), "{m}");
}

#[test]
fn wrong_vector_width_and_unsupported_extra_conversion_stay_unchanged() {
    for s in [
        fixture().replace("load.i64x2", "load.i32x4"),
        fixture().replace(
            "v8 = sext.i64 v7",
            "v20 = sext.i64 v7\n v21 = bitcast.f64 v20\n v8 = bitcast.i64 v21",
        ),
    ] {
        let mut m = parse(&s);
        let before = m.to_string();
        assert!(!pass(&mut m));
        assert_eq!(before, m.to_string());
    }
}

#[test]
fn nonzero_source_result_is_owned_but_cross_block_producers_are_not_local() {
    let s = fixture()
        .replace(
            "decl @effect(ptr)",
            "decl @effect(ptr)\n decl @vectors() -> (i64, i32x4)",
        )
        .replace("v5 = load.i32x4 align 1 v0", "v20, v5 = call @vectors()");
    let mut m = parse(&s);
    assert!(pass(&mut m));
    assert!(widen(&m).is_some());
    keel_ir::verify_module(&m).unwrap();
    for boundary in ["v10 = vextract", "store v6, v2"] {
        let s = fixture().replace(boundary, &format!("jump block1\nblock1:\n {boundary}"));
        let mut m = parse(&s);
        let before = m.to_string();
        assert!(!pass(&mut m));
        assert_eq!(before, m.to_string());
    }
    let future = fixture().replace(
        "v10 = vextract v5, 0\n v6 = sext.i64 v10",
        "v6 = sext.i64 v10\n v10 = vextract v5, 0",
    );
    let m = keel_ir::parse(&future).unwrap();
    assert!(keel_ir::verify_module(&m).is_err());
}

#[test]
fn consecutive_reloads_and_local_remaining_read_preserve_stores() {
    let s = fixture().replace(
        "store align 1 v9, v1",
        "store align 1 v9, v1\n v20 = load.i64x2 align 16 v2\n \
         store align 1 v20, v1\n v21 = load.ptr v4\n call @effect(v21)",
    );
    let mut m = parse(&s);
    // Alias the derived address as well as the root-address case above.
    let f = &mut m.functions[0];
    let addr = f
        .layout_insts()
        .find_map(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Binary {
                    op: keel_ir::BinaryOp::Ptradd,
                    ..
                }
            )
            .then_some(i)
        })
        .unwrap();
    let copy = f.insert_inst(
        keel_ir::InsertPoint::Before(addr),
        f.inst_data(addr).clone(),
    );
    f.replace_all_uses(f.first_result(addr), f.first_result(copy));
    f.remove_inst(addr);
    keel_ir::verify_module(&m).unwrap();
    assert!(pass(&mut m));
    assert_eq!(
        count(&m, |d| matches!(
            d,
            InstData::Convert {
                op: ConvertOp::SwidenLow,
                ..
            }
        )),
        2
    );
    // The pointer reload cannot forward from integer byte facts. Both private
    // writers must survive along with the two external output stores.
    assert_eq!(count(&m, |d| matches!(d, InstData::Store { .. })), 4);
    assert_eq!(count(&m, |d| matches!(d, InstData::Load { .. })), 2);
    keel_ir::verify_module(&m).unwrap();
    let once = m.to_string();
    assert!(!pass(&mut m));
    assert_eq!(once, m.to_string());
}

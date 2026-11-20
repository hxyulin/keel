mod support;
use keel_ir::{InstData, parse, verify_module};

#[test]
fn single_count_exit_preserves_the_memory_observations_of_each_path() {
    use keel_ir_interp::{Interpreter, Outcome, Val};
    use keel_opt::VectorTarget::{Aarch64Neon, X86Sse2};
    use std::fmt::Write as _;

    let cases = (16..144)
        .map(|block| format!("{block}: block{block}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "func @f(i16, ptr, ptr, ptr, i32) {{
block0(v0:i16, v1:ptr, v2:ptr, v3:ptr, v4:i32):
 v5=iconst.i32 1
 switch v4, block1, [1: block10, {cases}]
block1:
 brif v0, block2, block3
block2:
 v6=load.i64 v1
 store v6, v2
 v7=sext.i32 v0
 v8=icmp.sgt v7, v5
 brif v8, block4(v7), block5(v5)
block3:
 jump block5(v4)
block4(v9:i32):
 v10=iconst.i64 8
 v11=ptradd v2, v10
 v12=iconst.i8 0
 store v12, v11
 v13=isub v9, v5
 v14=icmp.sgt v13, v5
 brif v14, block4(v13), block5(v5)
block5(v15:i32):
 v16=load.i16 v3
 v17=iconst.i16 4
 v18=and v16, v17
 brif v18, block6, block7
block6:
 return
block7:
 v19=load.i64 v2
 jump block8(v19, v15)
block8(v20:i64, v21:i32):
 v22=iconst.i64 24
 v23=ptradd v3, v22
 store v20, v23
 store v21, v3
 return
block10:
 jump block5(v4)
"
    );
    for block in 16..144 {
        writeln!(source, "block{block}:\n return").unwrap();
    }
    source.push_str("}\n");

    let pass = ["specialize-single-count-exit"];
    let original = parse(&source).unwrap();
    verify_module(&original).unwrap();
    let mut candidate = original.clone();
    keel_opt::run_passes_for_target(&mut candidate, &pass, Aarch64Neon).unwrap();
    verify_module(&candidate).unwrap();
    assert_eq!(
        candidate.functions[0].blocks().count(),
        original.functions[0].blocks().count() + 3
    );
    let candidate = parse(&candidate.to_string()).unwrap();
    let execute = |function, count, src, dst, status, mode| {
        let mut interpreter = Interpreter::for_function(function).with_load_trace();
        let image: Vec<_> = (0..64).map(|i| (i * 17 + 3) as u8).collect();
        let base = interpreter.memory_mut().alloc_bytes(&image, 8);
        let result = interpreter.call(
            "f",
            &[
                Val::I16(count),
                Val::Ptr(base + src),
                Val::Ptr(base + dst),
                Val::Ptr(base + status),
                Val::I32(mode),
            ],
        );
        assert!(matches!(result, Outcome::Return(_)), "{result:?}");
        (
            interpreter.memory().read(base, 64).unwrap().to_vec(),
            interpreter.load_trace().to_vec(),
        )
    };
    for count in [0, 1, 2, 3, 0xffff] {
        for (src, dst, status) in [(0, 8, 16), (0, 0, 0), (0, 8, 8), (16, 8, 0)] {
            for mode in [0, 1] {
                assert_eq!(
                    execute(&candidate.functions[0], count, src, dst, status, mode),
                    execute(&original.functions[0], count, src, dst, status, mode),
                );
            }
        }
    }

    let mut other_target = original.clone();
    keel_opt::run_passes_for_target(&mut other_target, &pass, X86Sse2).unwrap();
    assert_eq!(other_target.to_string(), original.to_string());
    for changed in [
        source.replace("load.i64 v1", "load.i64 volatile v1"),
        source.replace("store v21, v3", "store v15, v3"),
    ] {
        let mut module = parse(&changed).unwrap();
        verify_module(&module).unwrap();
        let before = module.to_string();
        keel_opt::run_passes_for_target(&mut module, &pass, Aarch64Neon).unwrap();
        assert_eq!(module.to_string(), before);
    }
}
#[test]
fn complementary_signed_and_unsigned_tests_fold_only_on_dominating_edges() {
    for (first, second) in [("uge", "ult"), ("sge", "slt"), ("eq", "ne"), ("ugt", "ule")] {
        let src = format!(
            "func @f(i64,i64) -> i64 {{
block0(v0:i64,v1:i64):
 v2 = icmp.{first} v0,v1
 brif v2,block1,block2
block1:
 v3 = iconst.i64 0
 return v3
block2:
 v4 = icmp.{second} v0,v1
 brif v4,block3,block4
block3:
 v5 = iconst.i64 1
 return v5
block4:
 v6 = iconst.i64 99
 return v6
}}"
        );
        let original = parse(&src).unwrap();
        let mut m = original.clone();
        keel_opt::run_passes(&mut m, &["branch-facts"]).unwrap();
        verify_module(&m).unwrap();
        assert_eq!(
            m.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Brif { .. }))
                .count(),
            1
        );
        for a in [0, 1, i64::MIN as u64, i64::MAX as u64, u64::MAX] {
            for b in [0, 1, i64::MIN as u64, i64::MAX as u64, u64::MAX] {
                assert_eq!(
                    support::execute(&m.functions[0], &[a, b]),
                    support::execute(&original.functions[0], &[a, b])
                );
            }
        }
    }
}
#[test]
fn duplicate_edges_cannot_establish_comparison_truth() {
    let src = "func @f(i64,i64) -> i64 {
block0(v0:i64,v1:i64):
 v2=icmp.uge v0,v1
 brif v2,block1,block1
block1:
 v3=icmp.ult v0,v1
 brif v3,block2,block3
block2:
 v4=iconst.i64 1
 return v4
block3:
 v5=iconst.i64 0
 return v5
}";
    let mut m = parse(src).unwrap();
    let original = m.to_string();
    keel_opt::run_passes(&mut m, &["branch-facts"]).unwrap();
    assert_eq!(original, m.to_string());
}

fn sum_bounds(ty: &str, guard: &str) -> String {
    format!(
        "func @f({ty},{ty})->i64 {{
block0(v0:{ty},v1:{ty}):
 v2=iconst.{ty} 8
 v3=icmp.{guard} v0,v2
 brif v3,block1,block4
block1:
 v4=icmp.{guard} v1,v2
 brif v4,block2,block4
block2:
 v5=iadd v0,v1
 v6=iconst.{ty} 16
 v7=icmp.ult v5,v6
 brif v7,block3,block5
block3:
 v8=iconst.i64 1
 return v8
block4:
 v9=iconst.i64 0
 return v9
block5:
 v10=iconst.i64 99
 return v10
}}"
    )
}
fn branches(f: &keel_ir::Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Brif { .. }))
        .count()
}

#[test]
fn bounded_addition_proves_a_later_check_for_every_integer_width() {
    for ty in ["i8", "i16", "i32", "i64"] {
        let original = parse(&sum_bounds(ty, "ult")).unwrap();
        let mut optimized = original.clone();
        keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
        verify_module(&optimized).unwrap();
        assert_eq!(branches(&optimized.functions[0]), 2, "{ty}");
        let values = [
            0,
            1,
            7,
            8,
            15,
            16,
            127,
            128,
            255,
            256,
            i64::MAX as u64,
            i64::MIN as u64,
            u64::MAX,
        ];
        for a in values {
            for b in values {
                assert_eq!(
                    support::execute(&original.functions[0], &[a, b]),
                    support::execute(&optimized.functions[0], &[a, b]),
                    "{ty} {a} {b}"
                );
            }
        }
    }
}

#[test]
fn signed_guards_do_not_invent_nonnegative_or_nonwrapping_unsigned_sums() {
    let original = parse(&sum_bounds("i8", "slt")).unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
    verify_module(&optimized).unwrap();
    assert_eq!(branches(&optimized.functions[0]), 3);
    // Exhaust the narrow domain, including both kinds of signed wrap and all
    // negative bit patterns admitted by the signed guards.
    for a in 0..256 {
        for b in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[a, b]),
                support::execute(&optimized.functions[0], &[a, b]),
                "{a} {b}"
            );
        }
    }
}

#[test]
fn range_facts_are_scoped_and_keep_the_false_edge_bound() {
    let original = parse(
        "func @f(i8)->i64 {
block0(v0:i8):
 v1=iconst.i8 8
 v2=icmp.ult v0,v1
 brif v2,block1,block2
block1:
 v3=iconst.i8 16
 v4=icmp.ult v0,v3
 brif v4,block3,block4
block2:
 v5=iconst.i8 16
 v6=icmp.ult v0,v5
 brif v6,block3,block4
block3:
 v7=iconst.i64 1
 return v7
block4:
 v8=iconst.i64 2
 return v8
}",
    )
    .unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
    verify_module(&optimized).unwrap();
    assert_eq!(branches(&optimized.functions[0]), 2);
    for a in 0..256 {
        assert_eq!(
            support::execute(&original.functions[0], &[a]),
            support::execute(&optimized.functions[0], &[a]),
            "{a}"
        );
    }
}

#[test]
fn signed_negative_ranges_and_unsigned_high_ranges_remain_distinct() {
    for (guard, limit, check, check_limit) in [
        ("slt", -4, "slt", 0),
        ("uge", -4, "slt", 0),
        ("sge", 0, "ule", 127),
    ] {
        let source = format!(
            "func @f(i8)->i64 {{
block0(v0:i8):
 v1=iconst.i8 {limit}
 v2=icmp.{guard} v0,v1
 brif v2,block1,block4
block1:
 v3=iconst.i8 {check_limit}
 v4=icmp.{check} v0,v3
 brif v4,block2,block3
block2:
 v5=iconst.i64 1
 return v5
block3:
 v6=iconst.i64 99
 return v6
block4:
 v7=iconst.i64 0
 return v7
}}"
        );
        let original = parse(&source).unwrap();
        let mut optimized = original.clone();
        keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
        verify_module(&optimized).unwrap();
        assert_eq!(branches(&optimized.functions[0]), 1, "{source}");
        for a in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[a]),
                support::execute(&optimized.functions[0], &[a]),
                "{guard} {a}"
            );
        }
    }
}

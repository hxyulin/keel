mod support;
fn apply(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["branch-ranges"]).unwrap();
    assert!(
        !keel_opt::find_pass("branch-ranges")
            .unwrap()
            .run(&mut module.functions[0])
    );
    (original, module)
}
#[test]
fn all_predicate_pairs_and_edge_bounds_preserve_every_i8_input() {
    let predicates = [
        "eq", "ne", "ult", "ule", "ugt", "uge", "slt", "sle", "sgt", "sge",
    ];
    for first in predicates {
        for second in predicates {
            for c in [-128, -1, 0, 1, 16, 127] {
                let source = format!(
                    "func @f(i8) -> i8 {{\nblock0(v0: i8):\n v1 = iconst.i8 {c}\n v2 = icmp.{first} v0, v1\n brif v2, block1, block2\nblock1:\n v3 = iconst.i8 32\n v4 = icmp.{second} v0, v3\n return v4\nblock2:\n v5 = iconst.i8 0\n v6 = icmp.{second} v0, v5\n return v6\n}}"
                );
                let (original, module) = apply(&source);
                for x in 0..256 {
                    assert_eq!(
                        support::execute(&original.functions[0], &[x]),
                        support::execute(&module.functions[0], &[x]),
                        "{source}\n{module}\nx={x}"
                    );
                }
            }
        }
    }
}
#[test]
fn signed_unsigned_and_reversed_literal_guards_work_at_every_width() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for sign in ["u", "s"] {
            for reverse in [false, true] {
                let guard = if reverse {
                    format!("icmp.{sign}gt v1, v0")
                } else {
                    format!("icmp.{sign}lt v0, v1")
                };
                let source = format!(
                    "func @f({ty}) -> i8 {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} 16\n v2 = {guard}\n brif v2, block1, block2\nblock1:\n v3 = iconst.{ty} 32\n v4 = icmp.{sign}lt v0, v3\n return v4\nblock2:\n v5 = iconst.{ty} 8\n v6 = icmp.{sign}lt v0, v5\n return v6\n}}"
                );
                let (original, module) = apply(&source);
                assert_eq!(module.to_string().matches(" = icmp").count(), 1, "{module}");
                if ty != "i128" {
                    for x in (0..256).chain([u64::MAX, 1 << 63, (1 << 63) - 1]) {
                        assert_eq!(
                            support::execute(&original.functions[0], &[x]),
                            support::execute(&module.functions[0], &[x])
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn parallel_edges_joins_and_loop_headers_do_not_inherit_path_local_facts() {
    let sources = [
        "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 16
 v2 = icmp.ult v0, v1
 brif v2, block1, block1
block1:
 v3 = iconst.i8 32
 v4 = icmp.ult v0, v3
 return v4
}",
        "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 16
 v2 = icmp.ult v0, v1
 brif v2, block1, block2
block1:
 jump block3
block2:
 jump block3
block3:
 v3 = iconst.i8 32
 v4 = icmp.ult v0, v3
 return v4
}",
        "func @f(i8) -> i8 {
block0(v0: i8):
 jump block1(v0)
block1(v1: i8):
 v2 = iconst.i8 5
 v3 = icmp.ult v1, v2
 brif v3, block2, block3
block2:
 v4 = iconst.i8 10
 v5 = icmp.ult v1, v4
 v6 = iconst.i8 1
 v7 = iadd v1, v6
 v8 = select v5, v7, v1
 jump block1(v8)
block3:
 v9 = iconst.i8 10
 v10 = icmp.ult v1, v9
 return v10
}",
    ];
    for source in sources {
        let (original, module) = apply(source);
        assert_eq!(module.to_string().matches(" = icmp").count(), 2, "{module}");
        for x in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[x]),
                support::execute(&module.functions[0], &[x])
            );
        }
        for mask in 0..16 {
            let mut module = original.clone();
            let mut passes = vec!["branch-ranges"];
            for (i, p) in ["known-bits", "gvn", "instcombine", "simplify-cfg"]
                .iter()
                .enumerate()
            {
                if mask & (1 << i) != 0 {
                    passes.push(p);
                }
            }
            passes.extend(["branch-ranges", "instcombine", "simplify-cfg", "dce"]);
            keel_opt::run_passes(&mut module, &passes).unwrap();
            for x in 0..256 {
                assert_eq!(
                    support::execute(&original.functions[0], &[x]),
                    support::execute(&module.functions[0], &[x]),
                    "mask={mask}\n{module}"
                );
            }
        }
    }
}
#[test]
fn path_facts_do_not_cross_a_distinct_memory_read_or_signedness_domain() {
    let source = "func @f(ptr) -> i8 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n v2 = iconst.i64 16\n v3 = icmp.ult v1, v2\n brif v3, block1, block2\nblock1:\n v4 = load.i64 v0\n v5 = iconst.i64 32\n v6 = icmp.ult v4, v5\n return v6\nblock2:\n v7 = iconst.i8 0\n return v7\n}";
    let (_, module) = apply(source);
    assert_eq!(module.to_string().matches(" = icmp").count(), 2);
    let source = "func @f(i8) -> i8 {\nblock0(v0: i8):\n v1 = iconst.i8 0\n v2 = icmp.slt v0, v1\n brif v2, block1, block2\nblock1:\n v3 = iconst.i8 127\n v4 = icmp.ult v0, v3\n return v4\nblock2:\n v5 = iconst.i8 0\n return v5\n}";
    let (original, module) = apply(source);
    assert_eq!(module.to_string().matches(" = icmp").count(), 2);
    for x in 0..256 {
        assert_eq!(
            support::execute(&original.functions[0], &[x]),
            support::execute(&module.functions[0], &[x])
        );
    }
}

#[test]
fn pointer_equalities_fold_both_polarities_and_reversed_operands() {
    for predicate in ["eq", "ne"] {
        let source = format!(
            "func @f(ptr, ptr) -> i8 {{
block0(v0: ptr, v1: ptr):
 v2 = icmp.{predicate} v0, v1
 brif v2, block1, block2
block1:
 v3 = icmp.eq v1, v0
 return v3
block2:
 v4 = icmp.ne v0, v1
 return v4
}}"
        );
        let (original, module) = apply(&source);
        assert_eq!(module.to_string().matches(" = icmp").count(), 1, "{module}");
        // The small interpreter models integers only. Pointer equality uses
        // the same raw-bit equality; native pointer cases run in codegen gates.
        let original = keel_ir::parse(&original.to_string().replace("ptr", "i64")).unwrap();
        let module = keel_ir::parse(&module.to_string().replace("ptr", "i64")).unwrap();
        for a in [0, 1, 2, 255, u64::MAX, 1 << 63] {
            for b in [0, 1, 2, 255, u64::MAX, 1 << 63] {
                assert_eq!(
                    support::execute(&original.functions[0], &[a, b]),
                    support::execute(&module.functions[0], &[a, b]),
                    "{module}\na={a}, b={b}"
                );
            }
        }
    }
}

#[test]
fn pointer_facts_do_not_escape_siblings_or_parallel_edges() {
    for destinations in ["block1, block2", "block1, block1"] {
        let source = format!(
            "func @f(ptr, ptr) -> i8 {{
block0(v0: ptr, v1: ptr):
 v2 = icmp.ne v0, v1
 brif v2, {destinations}
block1:
 jump block3
block2:
 jump block3
block3:
 v3 = icmp.eq v0, v1
 return v3
}}"
        );
        let (_, module) = apply(&source);
        assert_eq!(module.to_string().matches(" = icmp").count(), 2, "{module}");
    }
}

#[test]
fn pointer_identity_fact_does_not_fold_distinct_loads_or_parameters() {
    let source = "func @f(ptr, ptr) -> i8 {
block0(v0: ptr, v1: ptr):
 v2 = load.ptr v0
 v3 = icmp.ne v2, v1
 brif v3, block1, block2
block1:
 v4 = load.ptr v0
 v5 = icmp.eq v4, v1
 return v5
block2:
 v6 = iconst.i8 0
 return v6
}";
    let (_, module) = apply(source);
    assert_eq!(module.to_string().matches(" = icmp").count(), 2, "{module}");
}

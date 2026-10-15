mod support;
use support::execute;

fn icmps(module: &keel_ir::Module) -> usize {
    module.functions[0]
        .layout_insts()
        .filter(|(_, i)| {
            matches!(
                module.functions[0].inst_data(*i),
                keel_ir::InstData::Icmp { .. }
            )
        })
        .count()
}

#[test]
fn reversed_predicates_merge_at_every_integer_width() {
    let predicates = [
        ("eq", "eq"),
        ("ne", "ne"),
        ("slt", "sgt"),
        ("sle", "sge"),
        ("sgt", "slt"),
        ("sge", "sle"),
        ("ult", "ugt"),
        ("ule", "uge"),
        ("ugt", "ult"),
        ("uge", "ule"),
    ];
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for (a, b) in predicates {
            let source = format!(
                "func @f({ty}, {ty}) -> i8 {{
block0(v0: {ty}, v1: {ty}):
    v2 = icmp.{a} v0, v1
    v3 = icmp.{b} v1, v0
    v4 = iadd v2, v3
    return v4
}}"
            );
            let original = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&original).unwrap();
            let mut module = original.clone();
            keel_opt::run_passes(&mut module, &["gvn"]).unwrap();
            assert_eq!(icmps(&module), 1, "{ty}, {a}, {b}");
            assert!(
                !keel_opt::find_pass("gvn")
                    .unwrap()
                    .run(&mut module.functions[0])
            );
            if ty == "i128" {
                continue;
            }
            for x in [
                0,
                1,
                127,
                128,
                255,
                32767,
                32768,
                65535,
                2147483647,
                2147483648,
                u64::MAX,
                1 << 63,
            ] {
                for y in [
                    0,
                    1,
                    127,
                    128,
                    255,
                    32767,
                    32768,
                    65535,
                    2147483647,
                    2147483648,
                    u64::MAX,
                    1 << 63,
                ] {
                    assert_eq!(
                        execute(&module.functions[0], &[x, y]),
                        execute(&original.functions[0], &[x, y]),
                        "{ty}, {a}, {b}, {x}, {y}"
                    );
                }
            }
        }
    }
}

#[test]
fn signed_and_unsigned_predicates_are_distinct() {
    let source = "func @f(i8, i8) -> i8 {
block0(v0: i8, v1: i8):
    v2 = icmp.slt v0, v1
    v3 = icmp.ugt v1, v0
    v4 = xor v2, v3
    return v4
}";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut module, &["gvn"]).unwrap();
    assert_eq!(icmps(&module), 2);
    assert_eq!(execute(&module.functions[0], &[255, 1]), 1);
}

#[test]
fn sibling_branches_do_not_share_a_nondominating_comparison() {
    let source = "func @f(i64, i64, i8) -> i8 {
block0(v0: i64, v1: i64, v2: i8):
    brif v2, block1, block2
block1:
    v3 = icmp.slt v0, v1
    return v3
block2:
    v4 = icmp.sgt v1, v0
    return v4
}";
    let original = keel_ir::parse(source).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["gvn"]).unwrap();
    assert_eq!(icmps(&module), 2);
    for condition in [0, 1] {
        assert_eq!(
            execute(&module.functions[0], &[u64::MAX, 1, condition]),
            execute(&original.functions[0], &[u64::MAX, 1, condition])
        );
    }
}

#[test]
fn comparison_equivalence_survives_the_pass_matrix() {
    let source = "func @f(i64, i64) -> i8 {
block0(v0: i64, v1: i64):
    v2 = icmp.ule v0, v1
    v3 = icmp.uge v1, v0
    v4 = iadd v2, v3
    return v4
}";
    let original = keel_ir::parse(source).unwrap();
    for subset in 0..32 {
        let other = ["const-fold", "sccp", "simplify-cfg", "dce", "gcm"];
        let mut names = vec!["gvn"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        for x in [0, 1, 17, u64::MAX] {
            for y in [0, 1, 19, u64::MAX] {
                assert_eq!(
                    execute(&module.functions[0], &[x, y]),
                    execute(&original.functions[0], &[x, y])
                );
            }
        }
    }
}

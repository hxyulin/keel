use keel_ir::{InstData, Purpose, Type, parse, verify_module};
use keel_opt::private_aggregate;

const SOURCE: &str =
    "func @pair(ptr byval [2 x i64], ptr byval {i64,i64}, ptr) -> i64 noinline local {
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = iconst.i64 8
 v4 = ptradd v0,v3
 v5 = load.i64 v4
 v6 = iconst.i64 99
 store v6,v0
 store v6,v2
 v7 = load.i64 v1
 v8 = iadd v5,v7
 return v8
}
func @entry(ptr) -> i64 {
block0(v0:ptr):
 v1 = call @pair(v0,v0,v0)
 return v1
}";
fn check(s: &str, expected: usize) -> keel_ir::Module {
    let mut m = parse(s).unwrap();
    verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(private_aggregate::run(&mut m), expected);
    verify_module(&m).unwrap();
    if expected == 0 {
        assert_eq!(before, m.to_string());
    }
    assert_eq!(private_aggregate::run(&mut m), 0);
    m
}
#[test]
fn independent_homes_and_call_snapshots() {
    let mut m = check(SOURCE, 1);
    let f = &m.functions[0];
    assert_eq!(
        f.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::I64, Type::I64, Type::I64, Type::I64, Type::Ptr]
    );
    assert!(f.sig.params.iter().all(|p| p.purpose == Purpose::Normal));
    assert_eq!(
        f.stack_slots()
            .map(|(_, s)| (s.size, s.align))
            .collect::<Vec<_>>(),
        [(16, 8), (16, 8)]
    );
    let loads: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| {
            matches!(f.inst_data(i), InstData::Load { ty: Type::I64, .. }).then_some(i)
        })
        .collect();
    // The initial high-word read is gone. The read after mutation stays.
    assert_eq!(loads.len(), 1);
    assert_eq!(
        f.resolve(f.first_result(loads[0])),
        f.first_result(loads[0])
    );
    assert_eq!(
        m.functions[1]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[1].inst_data(*i), InstData::Load { .. }))
            .count(),
        4
    );
    for (_, i) in m.functions[1].layout_insts() {
        if let InstData::Load { flags, .. } = m.functions[1].inst_data(i) {
            assert_eq!(flags.align, 1);
        }
    }
    keel_opt::run_passes(
        &mut m,
        &[
            "stack-forward",
            "stack-promote",
            "dce",
            "internal-fast-calls",
        ],
    )
    .unwrap();
    verify_module(&m).unwrap();
}
#[test]
fn public_addresses_padding_constraints_and_observers_are_unchanged() {
    for s in [
        SOURCE.replace(" noinline local", ""),
        SOURCE.replace(" noinline local", " weak_odr"),
        SOURCE.replace("[2 x i64]", "{i8,i64}"),
        SOURCE.replace("[2 x i64]", "[3 x i64]"),
        SOURCE.replace("store v6,v0", "store align 16 v6,v0"),
        format!("{SOURCE}\ndata @fp = align 8 {{ptr @pair}}"),
        format!(
            "{SOURCE}\nfunc @address()->ptr {{\nblock0:\n v0 = symbol_addr @pair\n return v0\n}}"
        ),
        SOURCE.replace(" return v8", " asm \"nop\", \"\"()\n return v8"),
        SOURCE.replace(" return v8", " v9 = intrinsic frame_address()\n return v8"),
    ] {
        check(&s, 0);
    }
    let mut debug = parse(SOURCE).unwrap();
    debug.files.push("fixture.c".into());
    let before = debug.to_string();
    assert_eq!(private_aggregate::run(&mut debug), 0);
    assert_eq!(before, debug.to_string());
}
#[test]
fn recursive_calls_and_sret_preserve_result_contract() {
    let s = "func @rec(ptr sret [2 x i64], ptr byval [2 x i64], i64) noinline local {
block0(v0:ptr,v1:ptr,v2:i64):
 v3 = iconst.i64 0
 v4 = icmp.eq v2,v3
 brif v4,block1,block2
block1:
 v5 = load.i64 v1
 store v5,v0
 return
block2:
 call @rec(v0,v1,v3)
 return
}";
    let m = check(s, 1);
    assert!(matches!(
        m.functions[0].sig.params[0].purpose,
        Purpose::Sret(_)
    ));
    assert_eq!(m.functions[0].sig.params.len(), 4);
}
#[test]
fn named_registration_is_appended_and_cleanup_is_full_only() {
    assert_eq!(
        keel_opt::PASSES.last().unwrap().name(),
        "private-aggregate-cleanup"
    );
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"private-aggregate"));
        assert_eq!(
            tier.pipeline().contains(&"private-aggregate-cleanup"),
            tier == keel_opt::OptimizationTier::Full
        );
    }
    let full = keel_opt::OptimizationTier::Full.pipeline();
    assert_eq!(
        &full[..2],
        ["private-aggregate-cleanup", "inline-cheap-reserve"]
    );
    assert_eq!(
        full.iter()
            .filter(|&&p| p == "private-aggregate-cleanup")
            .count(),
        2
    );
    assert_eq!(
        keel_opt::OptimizationTier::Full
            .pipeline()
            .iter()
            .rev()
            .take(2)
            .copied()
            .collect::<Vec<_>>(),
        ["internal-fast-calls", "private-aggregate-cleanup"]
    );
    let mut m = parse(SOURCE).unwrap();
    keel_opt::run_passes(&mut m, &["private-aggregate"]).unwrap();
    verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .sig
            .params
            .iter()
            .all(|p| p.purpose == Purpose::Normal)
    );
}

#[test]
fn conditional_cleanup_matches_the_explicit_pipeline_and_skips_unrelated_modules() {
    let mut expected = parse(SOURCE).unwrap();
    keel_opt::run_passes(
        &mut expected,
        &[
            "private-aggregate",
            "stack-forward",
            "stack-promote",
            "canonicalize",
            "instcombine",
            "dce",
        ],
    )
    .unwrap();
    let mut actual = parse(SOURCE).unwrap();
    keel_opt::run_passes(&mut actual, &["private-aggregate-cleanup"]).unwrap();
    verify_module(&actual).unwrap();
    assert_eq!(actual.to_string(), expected.to_string());

    let mut unrelated = parse("func @plain(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 0\n v2 = iadd v0,v1\n return v2\n}").unwrap();
    let before = unrelated.to_string();
    assert!(!private_aggregate::run_with_cleanup(&mut unrelated));
    assert_eq!(unrelated.to_string(), before);
    // Demonstrate that an unconditional cleanup would alter this module.
    keel_opt::run_passes(&mut unrelated, &["canonicalize", "instcombine", "dce"]).unwrap();
    assert_ne!(unrelated.to_string(), before);
}

#[test]
fn definition_restrict_contracts_are_conservatively_excluded() {
    let mut m = parse(SOURCE).unwrap();
    m.functions[0].restrict_params.push(2);
    verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(private_aggregate::run(&mut m), 0);
    assert_eq!(before, m.to_string());
}
#[test]
fn tail_and_returns_twice_endpoints_are_excluded() {
    let s = SOURCE.replace(
        " v1 = call @pair(v0,v0,v0)\n return v1",
        " return_call @pair(v0,v0,v0)",
    );
    check(&s, 0);
    check(
        &SOURCE.replace("noinline local", "returns_twice noinline local"),
        0,
    );
}

#[test]
fn explicit_custom_pipeline_verifies_module_rewrite() {
    let mut m = parse(SOURCE).unwrap();
    keel_opt::run_pipeline(&mut m, &[&private_aggregate::PrivateAggregate]).unwrap();
    verify_module(&m).unwrap();
    assert_eq!(m.functions[0].sig.params.len(), 5);
}

// Probe the restricted ARM argument-register projection, including scalar
// exhaustion between pairs. SysV's six GPRs do not impose this exclusion:
// both its register and stack aggregate reads already follow parameter order.
#[test]
fn source_order_eligibility_accounts_for_scalar_banks_and_sret() {
    fn signature(params: &[&str]) -> String {
        let args = params
            .iter()
            .enumerate()
            .map(|(i, p)| format!("v{i}:{}", p.split_whitespace().next().unwrap()))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "func @probe({}) noinline local {{\nblock0({args}):\n return\n}}",
            params.join(",")
        )
    }
    let pair = "ptr byval [2 x i64]";
    for (params, expected) in [
        (vec![pair; 4], 1), // x86 mixed register/stack; ARM all-register
        (vec![pair; 5], 0),
        (vec![pair; 10], 0),
        ([vec!["i64"; 8], vec![pair; 3]].concat(), 1), // all-stack
        ([vec!["i64"; 7], vec![pair; 2]].concat(), 1), // closes ARM GPR bank
        ([vec!["i64"; 6], vec![pair; 2]].concat(), 0),
        ([vec!["f64"; 8], vec![pair; 4]].concat(), 1),
        ([vec!["f64"; 8], vec![pair; 5]].concat(), 0),
        ([vec!["i128"; 3], vec![pair; 2]].concat(), 0),
        ([vec!["i128"; 4], vec![pair; 2]].concat(), 1),
        ([vec![pair], vec!["i128"; 3], vec![pair]].concat(), 0),
        ([vec!["ptr sret [3 x i64]"], vec![pair; 4]].concat(), 1),
        ([vec!["ptr sret [3 x i64]"], vec![pair; 5]].concat(), 0),
    ] {
        check(&signature(&params), expected);
    }
}

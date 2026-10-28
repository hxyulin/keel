//! Source policies survive declarations, definitions, direct calls and IR text.
use keel_ir::{OptimizationOption, Target};

fn compile(source: &str, target: Target) -> keel_ir::Module {
    keel_c::compile(
        source,
        "policies.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap()
}

#[test]
fn ordered_options_and_late_hints_reach_definitions_and_direct_callees() {
    for target in Target::NATIVE {
        let module = compile(
            r#"
            long work(long x) __attribute__((optimize(2, "no-unroll-loops")));
            long caller(long x) { return work(x); }
            long work(long x) { return x + 3; }
            long work(long) __attribute__((__cold__, __optimize__("Os", "no-inline"),
                flatten, optnone, no_instrument_function, no_profile_instrument_function));
        "#,
            target,
        );
        keel_ir::verify_module(&module).unwrap();
        let attrs = module.function_attrs("work").unwrap();
        assert!(attrs.cold && !attrs.hot && attrs.flatten && attrs.optnone);
        assert!(attrs.no_instrument_function && attrs.no_profile_instrument_function);
        assert_eq!(
            attrs.optimize.iter().cloned().collect::<Vec<_>>(),
            vec![
                OptimizationOption::Level(2),
                OptimizationOption::Options("no-unroll-loops".into()),
                OptimizationOption::Options("Os".into()),
                OptimizationOption::Options("no-inline".into()),
            ]
        );
        let printed = module.to_string();
        let again = keel_ir::parse(&printed).unwrap();
        keel_ir::verify_module(&again).unwrap();
        assert_eq!(printed, again.to_string());
        for (_, callee) in module
            .functions
            .iter()
            .find(|f| f.name == "caller")
            .unwrap()
            .callees()
        {
            assert_eq!(&callee.attrs, attrs);
        }
        let external = compile(
            "long external(long) __attribute__((hot,optimize(1))); long call(long x) { return external(x); }",
            target,
        );
        assert!(external.function_attrs("external").unwrap().hot);
        let pointer = compile(
            "long * __attribute__((cold,optimize(\"Os\"))) pointer(long *p) { return p; }",
            target,
        );
        assert!(pointer.function_attrs("pointer").unwrap().cold);
        assert_eq!(
            pointer
                .function_attrs("pointer")
                .unwrap()
                .optimize
                .iter()
                .count(),
            1
        );
    }
}

#[test]
fn policies_reject_conflicts_malformed_arguments_and_non_function_positions() {
    for source in [
        "int f(void) __attribute__((hot,cold));",
        "int f(void) __attribute__((hot)); int f(void) __attribute__((cold));",
        "int f(void) __attribute__((naked,interrupt));",
        "int f(void) __attribute__((naked,always_inline));",
        "int f(void) __attribute__((cold(1)));",
        "int f(void) __attribute__((interrupt(\"IRQ\")));",
        "int f(void) __attribute__((optimize()));",
        "int f(void) __attribute__((optimize(-1)));",
        "int level; int f(void) __attribute__((optimize(level)));",
        "int f(void) __attribute__((optimize(\"a\\0b\")));",
        "int x __attribute__((cold));",
        "int f(int x __attribute__((hot)));",
        "typedef int T __attribute__((optnone));",
        "typedef void (__attribute__((naked)) *F)(void);",
        "typedef void (__attribute__((interrupt)) *F)(void);",
        "struct S { int x __attribute__((flatten)); };",
    ] {
        assert!(
            keel_c::compile(
                source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false,
                }
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn special_entries_are_metadata_and_never_become_ordinary_executable_functions() {
    for policy in ["naked", "interrupt"] {
        for target in Target::NATIVE {
            let module = compile(
                &format!("void handler(void) __attribute__(({policy})); void handler(void) {{}}"),
                target,
            );
            keel_ir::verify_module(&module).unwrap();
            assert_eq!(
                module
                    .special_entry_contract()
                    .map(|(name, contract)| (name, contract.to_string())),
                Some(("handler", policy.to_owned()))
            );
            let printed = module.to_string();
            assert_eq!(keel_ir::parse(&printed).unwrap().to_string(), printed);
            let mut optimized = module.clone();
            keel_opt::optimize(&mut optimized).unwrap();
            assert_eq!(optimized.to_string(), printed);
            let error = keel_codegen::compile(&optimized, target).unwrap_err();
            assert!(
                error.message.contains(policy) && error.message.contains("not implemented"),
                "{error}"
            );
        }
    }
}

#[test]
fn metadata_only_attributes_are_not_advertised_as_implemented_capabilities() {
    let source = "__has_attribute(cold) __has_attribute(hot) __has_attribute(optimize) __has_attribute(optnone) __has_attribute(flatten) __has_attribute(no_instrument_function) __has_attribute(no_profile_instrument_function) __has_attribute(naked) __has_attribute(interrupt)";
    for target in Target::NATIVE {
        let text = keel_c::preprocess(
            source,
            "queries.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let values: Vec<_> = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .flat_map(str::split_whitespace)
            .collect();
        assert_eq!(values, vec!["0"; 9], "{target:?}: {text}");
    }
}

#[test]
fn inlining_does_not_duplicate_an_existing_callee_option_list() {
    let mut module = compile(
        r#"
        long work(long) __attribute__((cold,optimize("Os")));
        static __attribute__((always_inline)) inline long helper(long x) { return work(x); }
        long twice(long x) { return work(x) + helper(x); }
    "#,
        Target::X86_64LinuxGnu,
    );
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let attrs = module.function_attrs("work").unwrap();
    assert_eq!(attrs.optimize.iter().count(), 1);
    let caller = module.functions.iter().find(|f| f.name == "twice").unwrap();
    assert!(caller.layout_insts().all(|(_, i)| !matches!(caller.inst_data(i), keel_ir::InstData::Call { func, .. } if caller.callee(*func).name == "helper")));
    for (_, decl) in caller.callees().filter(|(_, d)| d.name == "work") {
        assert_eq!(&decl.attrs, attrs);
    }
}

#[test]
fn stored_hints_do_not_yet_change_native_emission() {
    for target in Target::NATIVE {
        let baseline = compile("long work(long x) { return x + 3; }", target);
        let expected = keel_codegen::compile(&baseline, target).unwrap();
        for attributes in [
            "cold,optimize(\"Os\")",
            "hot,optimize(3)",
            "optnone",
            "flatten",
            "no_instrument_function,no_profile_instrument_function",
        ] {
            let module = compile(
                &format!("long work(long x) __attribute__(({attributes})) {{ return x + 3; }}"),
                target,
            );
            assert_eq!(
                keel_codegen::compile(&module, target).unwrap(),
                expected,
                "{target:?}/{attributes}"
            );
        }
    }
}

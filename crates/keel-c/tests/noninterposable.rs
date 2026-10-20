use keel_ir::Target;

#[test]
fn explicit_binding_merges_in_either_order_without_changing_abi_or_exports() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for source in [
            "__attribute__((keel_noninterposable)) signed char helper(short); signed char helper(short x) { return x; }",
            "signed char helper(short x) { return x; } signed char helper(short) __attribute__((__keel_noninterposable__));",
            "signed char helper(short); signed char helper(short x) __attribute__((keel_noninterposable)) { return x; }",
        ] {
            let tail = "signed char caller(short x) { return helper(x); }";
            let mut m = keel_c::compile(
                &format!("{source} {tail}"),
                "binding.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap();
            keel_ir::verify_module(&m).unwrap();
            let helper = &m.functions[0];
            assert!(helper.attrs.noninterposable);
            assert!(!helper.local);
            let sig = helper.sig.clone();
            let r = m.functions[1].callees().next().unwrap().0;
            assert!(m.functions[1].callee(r).attrs.noninterposable);
            assert_eq!(sig, m.functions[1].callee(r).sig);
            // Contract plumbing by itself does not relax the ordinary inliner.
            keel_opt::run_passes(&mut m, &["inline"]).unwrap();
            assert!(m.functions[1].layout_insts().any(|(_, i)| matches!(
                m.functions[1].inst_data(i),
                keel_ir::InstData::Call { .. }
            )));
            assert_eq!(sig, m.functions[0].sig);
            keel_codegen::compile(&m, target).unwrap();
        }
        let m = keel_c::compile(
            "int helper(int x) { return x; }",
            "ordinary.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!m.functions[0].attrs.noninterposable);
        assert!(!m.functions[0].local);
    }
}

#[test]
fn binding_contract_is_not_an_indirect_function_type_promise() {
    let m = keel_c::compile("int helper(int x) __attribute__((keel_noninterposable)) { return x; } int caller(int (*p)(int), int x) { return p(x); }", "binding.c", Default::default()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.functions[1].callees().next().is_none());
    assert!(!m.functions[1].attrs.noninterposable);
    assert!(
        keel_c::compile(
            "int f(void) __attribute__((keel_noninterposable(1)));",
            "bad.c",
            Default::default()
        )
        .is_err()
    );
    let source = "#if !__has_attribute(keel_noninterposable)\n#error missing binding attribute\n#endif\nint f(void) { return 1; }";
    keel_c::compile(source, "feature.c", Default::default()).unwrap();
}

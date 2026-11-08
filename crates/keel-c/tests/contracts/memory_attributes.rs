use keel_ir::{InstData, MemoryAccess, Target};

fn compile(source: &str, target: Target) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "memory_attributes.c",
        keel_c::Options {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn memory_promises_accumulate_on_definitions_declarations_and_direct_calls() {
    for target in [
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
        Target::X86_64LinuxGnu,
    ] {
        for (attribute, access) in [
            ("pure", MemoryAccess::Read),
            ("__const__", MemoryAccess::None),
        ] {
            for declarations in [
                format!("__attribute__(({attribute})) int helper(int); int helper(int);"),
                format!("int helper(int); int helper(int) __attribute__(({attribute}));"),
                format!(
                    "int helper(int) __attribute__(({attribute})); int helper(int x) {{ return x + 1; }}"
                ),
            ] {
                let module = compile(
                    &format!("{declarations} int caller(int x) {{ return helper(x); }}"),
                    target,
                );
                let caller = module
                    .functions
                    .iter()
                    .find(|f| f.name == "caller")
                    .unwrap();
                let contract = caller
                    .layout_insts()
                    .find_map(|(_, i)| {
                        matches!(caller.inst_data(i), InstData::Call { .. })
                            .then(|| caller.effective_call_contract(i).unwrap())
                    })
                    .unwrap();
                assert_eq!(contract.memory.argmem, access, "{declarations}");
                assert_eq!(contract.memory.other, access);
                assert_eq!(contract.memory.inaccessible, access);
                assert!(
                    !contract.nofree
                        && !contract.nosync
                        && !contract.nocallback
                        && !contract.preserves_access
                );
                if let Some(helper) = module.functions.iter().find(|f| f.name == "helper") {
                    assert_eq!(helper.attrs.contract, contract);
                } else {
                    assert_eq!(
                        module
                            .decls
                            .iter()
                            .find(|f| f.name == "helper")
                            .unwrap()
                            .attrs
                            .contract,
                        contract
                    );
                }
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
}

#[test]
fn allocation_and_nullness_are_independent_return_promises() {
    for (attributes, noalias, nonnull) in [
        ("malloc", true, false),
        ("returns_nonnull", false, true),
        ("__malloc__, __returns_nonnull__", true, true),
        ("malloc(release, 1)", false, false),
        ("malloc(release, 1), returns_nonnull", false, true),
    ] {
        let module = compile(
            &format!(
                "void release(void *); void *make(void) __attribute__(({attributes})); void *caller(void) {{ return make(); }}"
            ),
            Target::Aarch64AppleDarwin,
        );
        let make = module.decls.iter().find(|f| f.name == "make").unwrap();
        let facts = make
            .attrs
            .contract
            .returns
            .first()
            .map(|p| p.1)
            .unwrap_or_default();
        assert_eq!(facts.return_noalias, noalias, "{attributes}");
        assert_eq!(facts.nonnull, nonnull, "{attributes}");
        assert_eq!(facts.readable_bytes, 0);
        assert_eq!(facts.readable_or_null_bytes, 0);
        assert_eq!(make.attrs.contract.memory, Default::default());
    }
}

#[test]
fn contracts_do_not_leak_to_indirect_calls_or_pointer_qualifiers() {
    let module = compile(
        "int helper(int) __attribute__((pure)); int caller(int (*p)(int), const int *q) { return p(*q); }",
        Target::X86_64LinuxGnu,
    );
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .unwrap();
    assert!(caller.attrs.contract.is_unknown());
    for (_, i) in caller.layout_insts() {
        if matches!(caller.inst_data(i), InstData::CallIndirect { .. }) {
            assert!(
                caller
                    .effective_call_contract(i)
                    .is_none_or(|c| c.is_unknown())
            );
        }
    }
}

#[test]
fn pointer_return_promises_reject_nonpointer_results_and_arguments() {
    for source in [
        "int f(void) __attribute__((malloc));",
        "struct S { int x; }; struct S f(void) __attribute__((returns_nonnull));",
        "int f(void) __attribute__((pure(1)));",
        "void *f(void) __attribute__((returns_nonnull(1)));",
    ] {
        assert!(
            keel_c::compile(source, "invalid.c", Default::default()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn return_promises_merge_across_redeclarations_and_factory_layers() {
    let module = compile(
        "void *make(void) __attribute__((malloc)); void *make(void) __attribute__((returns_nonnull)); void *make(void); void *caller(void) { return make(); }",
        Target::X86_64LinuxGnu,
    );
    let make = module.decls.iter().find(|f| f.name == "make").unwrap();
    let facts = make.attrs.contract.returns[0].1;
    assert!(facts.nonnull && facts.return_noalias);
    let module = compile(
        "int (* __attribute__((returns_nonnull)) factory(void))(int); int caller(int x) { return factory()(x); }",
        Target::X86_64LinuxGnu,
    );
    let factory = module.decls.iter().find(|f| f.name == "factory").unwrap();
    assert!(factory.attrs.contract.returns[0].1.nonnull);
}

#[test]
fn aliases_cannot_strengthen_target_contracts() {
    for (attribute, result, body) in [
        ("pure", "int", "return 1;"),
        ("returns_nonnull", "void *", "return 0;"),
    ] {
        let source = format!(
            "{result} target(void) {{ {body} }} {result} alias(void) __attribute__((alias(\"target\"), {attribute}));"
        );
        let error = keel_c::compile(
            &source,
            "alias.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error.message.contains("incompatible function contracts"),
            "{error}"
        );
        let source = format!("{result} target(void) __attribute__(({attribute})); {source}");
        compile(&source, Target::X86_64LinuxGnu);
    }
}

#[test]
fn attribute_queries_report_the_new_contracts() {
    let source = "#if !__has_attribute(pure) || !__has_attribute(__const__) || !__has_attribute(malloc) || !__has_attribute(returns_nonnull)\n#error missing contract attribute\n#endif\nint f(void) { return 1; }\n";
    keel_c::preprocess(source, "query.c", &Default::default()).unwrap();
}

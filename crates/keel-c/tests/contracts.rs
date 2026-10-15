use keel_ir::{InstData, Target};

#[test]
fn noreturn_declarations_terminate_direct_calls_and_remove_dead_effects() {
    let declarations = [
        "_Noreturn void stop(void);",
        "void stop(void) __attribute__((noreturn));",
        "__attribute__((__noreturn__)) void stop(void);",
        "void stop(void) __attribute__((noreturn)); void stop(void);",
        "void stop(void); _Noreturn void stop(void);",
        "int stop(void) __attribute__((noreturn));",
    ];
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for decl in declarations {
            for body in ["stop(); effect(); return 1;", "return (stop(), effect());"] {
                let source = format!("{decl} int effect(void); int f(void) {{ {body} }}");
                let mut module = keel_c::compile(
                    &source,
                    "contracts.c",
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                )
                .unwrap();
                keel_ir::verify_module(&module).unwrap();
                let f = module.functions.iter().find(|f| f.name == "f").unwrap();
                let (_, call) = f.layout_insts().find(|(_, i)| matches!(f.inst_data(*i), InstData::Call { func, .. } if f.callee(*func).name == "stop")).unwrap();
                let insts = f.block_insts(f.inst_block(call));
                let position = insts.iter().position(|&i| i == call).unwrap();
                assert!(
                    matches!(f.inst_data(insts[position + 1]), InstData::Trap),
                    "{source}\n{module}"
                );
                keel_codegen::compile(&module, target).unwrap();
                keel_opt::optimize(&mut module).unwrap();
                keel_ir::verify_module(&module).unwrap();
                let f = module.functions.iter().find(|f| f.name == "f").unwrap();
                assert!(!f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == "effect")), "{source}\n{module}");
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
}

#[test]
fn ordinary_calls_and_indirect_calls_keep_their_continuations() {
    for source in [
        "void stop(void); int effect(void); int f(void) { stop(); return effect(); }",
        // A declaration contract on stop says nothing about an arbitrary
        // function pointer that happens to have the same C signature.
        "void stop(void) __attribute__((noreturn)); int effect(void); int f(void (*p)(void)) { p(); return effect(); }",
    ] {
        let mut module = keel_c::compile(source, "contracts.c", Default::default()).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let f = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == "effect")));
    }
}

#[test]
fn builtin_va_list_has_the_target_layout_and_pointer_member_alignment() {
    for (target, size) in [
        (Target::Aarch64AppleDarwin, 8),
        (Target::X86_64LinuxGnu, 24),
    ] {
        let source = format!(
            r#"
            _Static_assert(sizeof(__builtin_va_list) == {size}, "va_list size");
            _Static_assert(_Alignof(__builtin_va_list) == 8, "va_list alignment");
            long first(int count, ...) {{
                __builtin_va_list ap;
                __builtin_va_start(ap, count);
                long value = __builtin_va_arg(ap, long);
                __builtin_va_end(ap);
                return value;
            }}
        "#
        );
        let mut module = keel_c::compile(
            &source,
            "va-list.c",
            keel_c::Options {
                target,
                debug_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

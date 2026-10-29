#[path = "contracts/attribute_consumers.rs"]
mod attribute_consumers;
#[path = "contracts/attribute_corrections.rs"]
mod attribute_corrections;
#[path = "contracts/attribute_factory_ownership.rs"]
mod attribute_factory_ownership;
#[path = "contracts/attribute_ownership.rs"]
mod attribute_ownership;

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

#[test]
fn function_attributes_merge_across_declarations_and_definitions() {
    for declarations in [
        "static inline int helper(int); static int helper(int) __attribute__((always_inline,nothrow));",
        "static int helper(int) __attribute__((__always_inline__,__nothrow__)); static inline int helper(int);",
    ] {
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            let source = format!(
                "{declarations} static int helper(int x) {{ return x+1; }} int f(int x) {{ return helper(x); }}"
            );
            let m = keel_c::compile(
                &source,
                "attributes.c",
                keel_c::Options {
                    target,
                    debug_info: true,
                },
            )
            .unwrap();
            keel_ir::verify_module(&m).unwrap();
            let helper = m.function_attrs("helper").unwrap();
            assert!(helper.inlinehint && helper.alwaysinline && helper.nounwind);
            assert!(!helper.noreturn);
            let f = m.functions.iter().find(|f| f.name == "f").unwrap();
            assert_eq!(&f.callees().next().unwrap().1.attrs, helper);
            keel_codegen::compile(&m, target).unwrap();
            let mut m = m;
            keel_opt::optimize(&mut m).unwrap();
            keel_ir::verify_module(&m).unwrap();
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

#[test]
fn declaration_contracts_stay_separate_from_indirect_signatures() {
    let m = keel_c::compile("void stop(void) __attribute__((noreturn)); void safe(void) __attribute__((nothrow)); int f(void (*p)(void)) { safe(); p(); return 7; }", "attributes.c", Default::default()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let safe = m.decls.iter().find(|d| d.name == "safe").unwrap();
    assert!(safe.attrs.nounwind && !safe.attrs.noreturn);
    let f = &m.functions[0];
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::CallIndirect { .. }))
    );
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Return { .. }))
    );
    let m = keel_c::compile(
        "void stop(void) __attribute__((noreturn)); void f(void) { stop(); }",
        "attributes.c",
        Default::default(),
    )
    .unwrap();
    let stop = m.decls.iter().find(|d| d.name == "stop").unwrap();
    assert!(stop.attrs.noreturn && !stop.attrs.nounwind);
}

#[test]
fn explicit_inline_conflicts_are_diagnosed_in_both_orders() {
    for source in [
        "int f(void) __attribute__((always_inline,noinline));",
        "int f(void) __attribute__((always_inline)); int f(void) __attribute__((noinline));",
        "int f(void) __attribute__((noinline)); int f(void) __attribute__((always_inline));",
        "int f(void) __attribute__((noinline)); __attribute__((always_inline)) int f(void) { return 1; }",
        "void f(void) __attribute__((returns_twice,noreturn));",
    ] {
        let error = keel_c::compile(source, "attributes.c", Default::default()).unwrap_err();
        assert!(error.to_string().contains("conflicts"), "{source}: {error}");
    }
    for source in [
        "int f(void) __attribute__((noinline(1)));",
        "void f(void) __attribute__((nothrow(1)));",
    ] {
        assert!(keel_c::compile(source, "attributes.c", Default::default()).is_err());
    }
    let m = keel_c::compile("static inline int h(int x) __attribute__((noinline)); static int h(int x) { return x; } int f(int x) { return h(x); }", "attributes.c", Default::default()).unwrap();
    let h = m.functions.iter().find(|f| f.name == "h").unwrap();
    assert!(h.attrs.inlinehint && h.attrs.noinline);
    assert!(!h.attrs.nounwind);
    let mut m = m;
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. }))
    );
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_attribute_semantics_debug_info_and_unexpected_return_trap() {
    use std::os::unix::process::ExitStatusExt;
    use std::{fs, path::Path, process::Command};
    fn command(cmd: &mut Command) {
        let result = cmd.output().unwrap();
        assert!(result.status.success(), "{cmd:?}: {result:?}");
    }
    // Keep fixtures/executables under target/tmp for provenance inspection.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("function-attributes-native");
    fs::create_dir_all(&dir).unwrap();
    let source = "static inline int inc(int x) __attribute__((always_inline,nothrow));\nstatic int inc(int x) { volatile int y=x; return y+1; }\nstatic int twice(int x) __attribute__((noinline));\nstatic int twice(int x) { return 2*x; }\nint main(void) { return twice(inc(20)) == 42 ? 0 : 1; }\n";
    let trap = "void stop(void) __attribute__((noreturn));\nint main(void) { stop(); return 9; }\n";
    let inline_trap = "static void stop(void) __attribute__((noreturn,always_inline));\nstatic void stop(void) { return; }\nint main(void) { stop(); return 9; }\n";
    for (name, src) in [
        ("semantics", source),
        ("trap", trap),
        ("inline-trap", inline_trap),
    ] {
        let path = dir.join(format!("{name}.c"));
        fs::write(&path, src).unwrap();
        let m = keel_c::compile(
            src,
            path.to_str().unwrap(),
            keel_c::Options {
                debug_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        for optimized in [false, true] {
            let mut m = m.clone();
            if optimized {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            if name == "semantics" && optimized {
                let main = m.functions.iter().find(|f| f.name == "main").unwrap();
                assert!(!main.layout_insts().any(|(_, i)| matches!(main.inst_data(i), InstData::Call {func, ..} if main.callee(*func).name == "inc")));
                assert!(main.layout_insts().any(|(_, i)| matches!(main.inst_data(i), InstData::Call {func, ..} if main.callee(*func).name == "twice")));
            }
            if name == "inline-trap" && optimized {
                let main = m.functions.iter().find(|f| f.name == "main").unwrap();
                assert!(
                    !main
                        .layout_insts()
                        .any(|(_, i)| matches!(main.inst_data(i), InstData::Call { .. }))
                );
            }
            let stem = format!("{name}-{}", if optimized { "opt" } else { "none" });
            let asm = keel_codegen::compile(&m, Default::default()).unwrap();
            assert!(asm.contains("debug_info"));
            let asm_path = dir.join(format!("{stem}.s"));
            let exe = dir.join(&stem);
            fs::write(&asm_path, asm).unwrap();
            let mut clang = Command::new("clang");
            clang.arg("-g").arg(&asm_path);
            if name == "trap" {
                // Separate definition avoids teaching Clang the caller's promise.
                let shim = dir.join("returning-stop.c");
                fs::write(&shim, "void stop(void) {}\n").unwrap();
                clang.arg(shim);
            }
            command(clang.arg("-o").arg(&exe));
            let status = Command::new(&exe).status().unwrap();
            if name == "semantics" {
                assert!(status.success());
            } else {
                assert!(matches!(status.signal(), Some(4 | 5)), "{status:?}");
            }
            println!("{exe:?}: {status}");
        }
    }
}

#[test]
fn inline_policy_does_not_change_c_definition_emission() {
    for (storage, local) in [
        ("inline", true),
        ("static inline", true),
        ("extern inline", false),
    ] {
        let source = format!(
            "{storage} int h(int x) __attribute__((noinline)); {storage} int h(int x) {{ return x+1; }} int f(int x) {{ return h(x); }}"
        );
        let m = keel_c::compile(&source, "inline-emission.c", Default::default()).unwrap();
        let h = m.functions.iter().find(|f| f.name == "h").unwrap();
        assert_eq!(h.local, local, "{storage}");
        assert!(h.attrs.inlinehint && h.attrs.noinline);
        keel_ir::verify_module(&m).unwrap();
    }
}

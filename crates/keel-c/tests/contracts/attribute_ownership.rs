//! Correctness assertions for ROUND2 and attribute-specific ROUND3 ownership.
use keel_ir::{Function, FunctionAttrs, InstData, Target};
const TARGETS: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];
fn calls(f: &Function, name: &str) -> bool {
    f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == name))
}
fn block_source(storage: &str, declaration: &str, late: bool, policy: &str) -> String {
    let definition =
        format!("{storage} inline int h(int x) __attribute__(({policy})){{return x+1;}}");
    let caller = format!("int f(int x){{{declaration} int h(int);return h(x);}}");
    // An early static prototype establishes genuine internal linkage before
    // the block extern; no extra non-inline FILE declaration otherwise.
    let prefix = if storage == "static" {
        "static inline int h(int);"
    } else {
        ""
    };
    if late {
        format!("{prefix}{definition}{caller}")
    } else {
        format!("{prefix}{caller}{definition}")
    }
}
#[test]
fn block_declarations_merge_contracts_without_requesting_external_emission() {
    for storage in ["", "static"] {
        for declaration in ["extern", ""] {
            for late in [false, true] {
                for policy in ["", "noinline", "always_inline"] {
                    let source = block_source(storage, declaration, late, policy);
                    let source = if policy == "always_inline" {
                        format!("{source} __typeof__(&h) keep_h=h;")
                    } else {
                        source
                    };
                    check_matrix(&source, |m, optimized| {
                        let h = m.functions.iter().find(|f| f.name == "h");
                        assert_eq!(
                            h.is_some(),
                            policy == "always_inline" || !optimized || policy == "noinline",
                            "{source}\n{m}"
                        );
                        if let Some(h) = h {
                            assert!(h.local, "{source}\n{m}");
                        } else {
                            assert!(m.decls.iter().any(|d| d.name == "h"), "{source}\n{m}");
                        }
                        assert_eq!(
                            calls(m.functions.iter().find(|f| f.name == "f").unwrap(), "h"),
                            policy != "always_inline" && (!optimized || policy == "noinline"),
                            "{source}\n{m}"
                        );
                    });
                }
            }
        }
    }
    for late in [false, true] {
        let definition = "inline int h(int x){return x+1;}";
        let caller =
            "int f(int x){extern int h(int) __attribute__((noinline,nothrow));return h(x);}";
        let source = if late {
            format!("{definition}{caller}")
        } else {
            format!("{caller}{definition}")
        };
        check_matrix(&source, |m, _| {
            let h = m.functions.iter().find(|f| f.name == "h").unwrap();
            assert!(h.local && h.attrs.noinline && h.attrs.nounwind);
            assert!(calls(
                m.functions.iter().find(|f| f.name == "f").unwrap(),
                "h"
            ));
        });
        let source = source.replace(
            "inline int h",
            "inline __attribute__((always_inline)) int h",
        );
        assert!(
            keel_c::compile(&source, "conflict.c", Default::default())
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
    }
}
fn check_matrix(source: &str, check: impl Fn(&keel_ir::Module, bool)) {
    for target in TARGETS {
        for debug_info in [false, true] {
            let original = keel_c::compile(
                source,
                "ownership.c",
                keel_c::Options { target, debug_info },
            )
            .unwrap_or_else(|e| panic!("{source}: {e}"));
            for optimized in [false, true] {
                let mut m = original.clone();
                if optimized {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                check(&m, optimized);
                keel_codegen::compile(&m, target).unwrap();
            }
        }
    }
}
fn factories(attr: &str) -> Vec<String> {
    [
        format!("void (* __attribute__(({attr})) get(void))(void)"),
        format!("void (* __attribute__(({attr})) (get(void)))(void)"),
        format!("void (__attribute__(({attr})) (*get(void)))(void)"),
        format!("void (__attribute__(({attr})) (*(get(void))))(void)"),
        format!("void ((__attribute__(({attr})) (*get(void))))(void)"),
        format!("void (*(__attribute__(({attr})) (*get(void)))(void))(void)"),
    ]
    .into_iter()
    .map(|d| format!("{d}{{return 0;}} int main(void){{return get()==0?0:1;}}"))
    .collect()
}
#[test]
fn nested_composition_distinguishes_declaration_and_function_type_contracts() {
    for attr in [
        "noreturn",
        "nothrow",
        "noinline",
        "always_inline",
        "returns_twice",
    ] {
        for source in factories(attr) {
            check_matrix(&source, |m, _| {
                let get = m.functions.iter().find(|f| f.name == "get").unwrap();
                assert_eq!(
                    get.attrs,
                    FunctionAttrs {
                        noinline: attr == "noinline",
                        alwaysinline: attr == "always_inline",
                        nounwind: attr == "nothrow",
                        ..Default::default()
                    },
                    "{source}\n{m}"
                );
                assert_eq!(get.sig.returns_twice, attr == "returns_twice");
                let main = m.functions.iter().find(|f| f.name == "main").unwrap();
                assert!(
                    !main
                        .layout_insts()
                        .any(|(_, i)| matches!(main.inst_data(i), InstData::Trap)),
                    "{source}\n{m}"
                );
                assert!(
                    main.layout_insts()
                        .any(|(_, i)| matches!(main.inst_data(i), InstData::Return { .. }))
                );
            });
        }
    }
}
#[test]
fn nested_pointer_variables_parameters_and_typedefs_keep_indirect_continuations() {
    for attr in [
        "noreturn",
        "nothrow",
        "returns_twice",
        "noinline,always_inline",
    ] {
        for source in [
            format!("void (__attribute__(({attr})) (*p))(void); int f(void){{p();return 7;}}"),
            format!("int f(void (__attribute__(({attr})) (*p))(void)){{p();return 7;}}"),
            format!(
                "typedef void (__attribute__(({attr})) (*P))(void); int f(P p){{p();return 7;}}"
            ),
        ] {
            check_matrix(&source, |m, _| {
                let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                assert_eq!(f.attrs, FunctionAttrs::default(), "{source}\n{m}");
                assert!(!f.sig.returns_twice);
                assert!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::CallIndirect { .. }))
                );
                assert!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Return { .. }))
                );
                assert!(
                    !f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Trap))
                );
            });
        }
    }
}

#[test]
fn named_pointer_returning_functions_retain_their_own_contracts() {
    // Independently vary pointer depth, parentheses, placement and declaration
    // order. A function type with the same ABI is not the same contract owner.
    for attr in [
        "noinline",
        "always_inline",
        "nothrow",
        "returns_twice",
        "noreturn",
    ] {
        for declaration in [
            format!("int * __attribute__(({attr})) h(void);"),
            format!("int ** __attribute__(({attr})) h(void);"),
            format!("int * __attribute__(({attr})) (h)(void);"),
            format!("int * __attribute__(({attr})) (h(void));"),
            format!("int * __attribute__(({attr})) ((h)(void));"),
            format!("int * (__attribute__(({attr})) h)(void);"),
            format!("int *h(void) __attribute__(({attr}));"),
            format!("__attribute__(({attr})) int *h(void);"),
            format!("int * const __attribute__(({attr})) h(void);"),
        ] {
            for late in [false, true] {
                let plain = if declaration.contains("**") {
                    "int **h(void);"
                } else {
                    "int *h(void);"
                };
                let source = if late {
                    format!("{plain}{declaration}")
                } else {
                    format!("{declaration}{plain}")
                } + "int f(void){h();return 7;}";
                check_matrix(&source, |m, _| {
                    let h = m.decls.iter().find(|d| d.name == "h").unwrap();
                    assert_eq!(h.attrs.noinline, attr == "noinline", "{source}\n{m}");
                    assert_eq!(
                        h.attrs.alwaysinline,
                        attr == "always_inline",
                        "{source}\n{m}"
                    );
                    assert_eq!(h.attrs.nounwind, attr == "nothrow", "{source}\n{m}");
                    assert_eq!(h.attrs.noreturn, attr == "noreturn", "{source}\n{m}");
                    assert_eq!(
                        h.sig.returns_twice,
                        attr == "returns_twice",
                        "{source}\n{m}"
                    );
                });
            }
        }
    }
    check_matrix(
        "static int *__attribute__((noinline)) h(void){return 0;} int f(void){return h()==0;}",
        |m, _| {
            assert!(calls(
                m.functions.iter().find(|f| f.name == "f").unwrap(),
                "h"
            ));
        },
    );
    // A function suffix can introduce a new owner for the next nested layer,
    // while attributes already collected by its pointer layer keep their owner.
    for attr in [
        "noinline",
        "always_inline",
        "nothrow",
        "returns_twice",
        "noreturn",
    ] {
        let source = format!(
            "void (*(__attribute__(({attr})) get)(void))(void); int f(void){{get();return 7;}}"
        );
        check_matrix(&source, |m, _| {
            let get = m.decls.iter().find(|d| d.name == "get").unwrap();
            assert_eq!(get.attrs.noinline, attr == "noinline", "{source}\n{m}");
            assert_eq!(
                get.attrs.alwaysinline,
                attr == "always_inline",
                "{source}\n{m}"
            );
            assert_eq!(get.attrs.nounwind, attr == "nothrow", "{source}\n{m}");
            assert_eq!(get.attrs.noreturn, attr == "noreturn", "{source}\n{m}");
            assert_eq!(
                get.sig.returns_twice,
                attr == "returns_twice",
                "{source}\n{m}"
            );
        });
    }
    check_matrix(
        "void (*__attribute__((noreturn,returns_twice)) (__attribute__((noinline)) get)(void))(void); int f(void){get();return 7;}",
        |m, _| {
            let get = m.decls.iter().find(|d| d.name == "get").unwrap();
            assert!(get.attrs.noinline && !get.attrs.noreturn && get.sig.returns_twice);
        },
    );
    // Typedef-supplied function pointers are not functions constructed by an
    // enclosing declarator layer. Grouping must still preserve named promises.
    check_matrix(
        "typedef void (*P)(void); P (__attribute__((noinline,noreturn)) h)(void); int f(void){h();return 7;}",
        |m, _| {
            let h = m.decls.iter().find(|d| d.name == "h").unwrap();
            assert!(h.attrs.noinline && h.attrs.noreturn);
        },
    );
    for attr in ["noreturn", "noinline", "returns_twice"] {
        let source = format!("typedef void (*P)(void); P *__attribute__(({attr})) h(void);");
        assert!(
            keel_c::compile(&source, "ambiguous.c", Default::default())
                .unwrap_err()
                .to_string()
                .contains("ambiguous function attribute ownership")
        );
    }
    // Declaration policies at nested factory placements are still named:
    // contradictory policies must conflict in either declaration order.
    for late in [false, true] {
        let named = "void (*get(void))(void) __attribute__((noinline,nothrow));";
        let pointer = "void (__attribute__((always_inline,returns_twice)) (*get(void)))(void);";
        let source = if late {
            format!("{named}{pointer}")
        } else {
            format!("{pointer}{named}")
        } + "int f(void){get();return 7;}";
        assert!(
            keel_c::compile(&source, "conflict.c", Default::default())
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_round2_links_returns_and_traps() {
    use std::{fs, os::unix::process::ExitStatusExt, path::Path, process::Command};
    fn run(cmd: &mut Command) -> std::process::Output {
        let o = cmd.output().unwrap();
        assert!(o.status.success(), "{cmd:?}: {o:?}");
        o
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("attribute-ownership");
    fs::create_dir_all(&dir).unwrap();
    let mut fixtures = Vec::new();
    for storage in ["", "static"] {
        for declaration in ["", "extern"] {
            for late in [false, true] {
                fixtures.push((
                    format!("block-{storage}-{declaration}-{late}"),
                    block_source(storage, declaration, late, "noinline"),
                    "int h(int x){return x+1;} int f(int); int main(void){return f(41)==42?0:1;}",
                    false,
                ));
            }
        }
    }
    for (i, source) in factories("noreturn").into_iter().enumerate() {
        fixtures.push((format!("factory-{i}"), source, "", false));
    }
    fixtures.push(("named-noinline".into(), "static int *__attribute__((noinline)) h(void){return 0;} int main(void){return h()==0?0:1;}".into(), "", false));
    for (i, decl) in [
        "int *__attribute__((noreturn)) h(void);",
        "int *h(void) __attribute__((noreturn));",
    ]
    .into_iter()
    .enumerate()
    {
        fixtures.push((
            format!("named-trap-{i}"),
            format!("{decl} int main(void){{h();return 0;}}"),
            "int *h(void){return 0;}",
            true,
        ));
    }
    fixtures.push((
        "nested-named-trap".into(),
        "void (*(__attribute__((noreturn)) h)(void))(void); int main(void){h();return 0;}".into(),
        "void (*h(void))(void){return 0;}",
        true,
    ));
    for (name, source, driver, trap) in fixtures {
        let c = dir.join(format!("{name}.c"));
        fs::write(&c, &source).unwrap();
        let d = dir.join(format!("{name}-driver.c"));
        fs::write(&d, driver).unwrap();
        let link = |input: &Path, exe: &Path, optimized: bool| {
            let mut cmd = Command::new("clang");
            cmd.arg("-std=c11")
                .arg(if optimized { "-O2" } else { "-O0" })
                .arg(input);
            if !driver.is_empty() {
                cmd.arg(&d);
            }
            run(cmd.arg("-o").arg(exe));
        };
        // Clang only checks that the fixture is valid and exits 0. Its behavior
        // after a violated noreturn promise is not the oracle for Keel's trap.
        if !trap {
            let exe = dir.join(format!("{name}-clang"));
            link(&c, &exe, false);
            run(&mut Command::new(&exe));
        }
        // The IR tests above cover debug info on and off.
        for optimized in [false, true] {
            let stem = format!("{name}-opt{optimized}");
            let mut m = keel_c::compile(&source, c.to_str().unwrap(), Default::default()).unwrap();
            if optimized {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            let asm = dir.join(format!("{stem}.s"));
            fs::write(&asm, keel_codegen::compile(&m, Default::default()).unwrap()).unwrap();
            let exe = dir.join(format!("{stem}-keel"));
            link(&asm, &exe, optimized);
            let status = Command::new(&exe).status().unwrap();
            if trap {
                // Match the native trap instruction on each enabled target.
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                let trap_signal = 5; // brk: SIGTRAP
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                let trap_signal = 4; // ud2: SIGILL
                assert_eq!(status.signal(), Some(trap_signal), "{stem}: {status}");
            } else {
                assert!(status.success(), "{stem}: {status}");
            }
            println!("{stem}-keel: {status}");
        }
    }
}

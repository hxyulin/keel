//! Regressions for independent function-attribute review round 1 (F1/F2).
use keel_ir::{Function, FunctionAttrs, InstData, Target};

const TARGETS: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];

fn calls(f: &Function, name: &str) -> bool {
    f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == name))
}

fn linkage_source(storage: &str, declaration: &str, late: bool, policy: &str) -> String {
    let definition =
        format!("{storage} inline int h(int x) __attribute__(({policy})) {{ return x+1; }}");
    let declaration = format!("{declaration} int h(int);");
    let declarations = if late {
        format!("{definition}\n{declaration}")
    } else {
        let prior_static = if storage == "static" {
            "static int h(int);\n"
        } else {
            ""
        };
        format!("{prior_static}{declaration}\n{definition}")
    };
    format!("{declarations}\nint f(int x) {{ return h(x); }}")
}

#[test]
fn inline_emission_is_finalized_after_all_declarations() {
    for target in TARGETS {
        for debug_info in [false, true] {
            for storage in ["", "static", "extern"] {
                for declaration in ["extern", "", "inline"] {
                    for late in [false, true] {
                        for policy in ["", "always_inline", "noinline"] {
                            let source = linkage_source(storage, declaration, late, policy);
                            let m = keel_c::compile(
                                &source,
                                "linkage.c",
                                keel_c::Options { target, debug_info },
                            )
                            .unwrap();
                            let local = storage == "static"
                                || (storage.is_empty() && declaration == "inline");
                            for optimized in [false, true] {
                                let mut m = m.clone();
                                if optimized {
                                    keel_opt::optimize(&mut m).unwrap();
                                }
                                keel_ir::verify_module(&m).unwrap();
                                let h = m.functions.iter().find(|f| f.name == "h");
                                assert_eq!(
                                    h.is_some(),
                                    !optimized || !local || policy == "noinline",
                                    "{source}\n{m}"
                                );
                                if let Some(h) = h {
                                    assert_eq!(h.local, local, "{source}\n{m}");
                                } else {
                                    assert!(m.decls.iter().any(|d| d.name == "h"), "{source}\n{m}");
                                }
                                let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                                let mandatory_public_inline = policy == "always_inline" && !local;
                                assert_eq!(
                                    calls(f, "h"),
                                    !mandatory_public_inline
                                        && (!optimized || !local || policy == "noinline"),
                                    "{source}\n{m}"
                                );
                                keel_codegen::compile(&m, target).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}

fn pointer_source(attr: &str, nested: bool) -> String {
    let name = if nested { "(get(void))" } else { "get(void)" };
    format!(
        "void (* __attribute__(({attr})) {name})(void) {{ return 0; }}\nint main(void) {{ return get()==0 ? 0 : 1; }}"
    )
}

#[test]
fn pointer_contracts_do_not_leak_into_factory_or_indirect_calls() {
    for attr in [
        "noreturn",
        "nothrow",
        "always_inline",
        "noinline",
        "returns_twice",
    ] {
        for target in TARGETS {
            for debug_info in [false, true] {
                let mut sources = vec![pointer_source(attr, false), pointer_source(attr, true)];
                sources.push(format!("void (* __attribute__(({attr})) get(void))(void);\nvoid (*get(void))(void) {{ return 0; }}\nint main(void) {{ return get()==0 ? 0 : 1; }}"));
                // Contracts on variables, parameters and typedefs do not promise
                // anything about indirect calls or the enclosing function.
                sources.push(format!("typedef void (* __attribute__(({attr})) P)(void); int f(P p) {{ p(); return 7; }}"));
                sources.push(format!(
                    "int f(void (* __attribute__(({attr})) p)(void)) {{ p(); return 7; }}"
                ));
                sources.push(format!(
                    "void (* __attribute__(({attr})) p)(void); int f(void) {{ p(); return 7; }}"
                ));
                sources.push(format!("typedef void (* __attribute__(({attr})) (P))(void); int f(P p) {{ p(); return 7; }}"));
                sources.push(format!(
                    "int f(void (* __attribute__(({attr})) (p))(void)) {{ p(); return 7; }}"
                ));
                sources.push(format!(
                    "void (* __attribute__(({attr})) (p))(void); int f(void) {{ p(); return 7; }}"
                ));
                sources.push(format!("void (*(* __attribute__(({attr})) get(void))(void))(void) {{ return 0; }} int main(void) {{ return get()==0 ? 0 : 1; }}"));
                for source in sources {
                    let m = keel_c::compile(
                        &source,
                        "pointer.c",
                        keel_c::Options { target, debug_info },
                    )
                    .unwrap();
                    for optimized in [false, true] {
                        let mut m = m.clone();
                        if optimized {
                            keel_opt::optimize(&mut m).unwrap();
                        }
                        keel_ir::verify_module(&m).unwrap();
                        for f in &m.functions {
                            // Noreturn follows the returned function type;
                            // declaration-only attributes belong to named get.
                            assert_eq!(
                                f.attrs,
                                FunctionAttrs {
                                    noinline: f.name == "get" && attr == "noinline",
                                    alwaysinline: f.name == "get" && attr == "always_inline",
                                    nounwind: f.name == "get" && attr == "nothrow",
                                    ..Default::default()
                                },
                                "{source}\n{m}"
                            );
                            assert_eq!(
                                f.sig.returns_twice,
                                f.name == "get" && attr == "returns_twice",
                                "{source}\n{m}"
                            );
                            assert!(
                                !f.layout_insts()
                                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Trap)),
                                "{source}\n{m}"
                            );
                            assert!(
                                f.layout_insts().any(|(_, i)| matches!(
                                    f.inst_data(i),
                                    InstData::Return { .. }
                                )),
                                "{source}\n{m}"
                            );
                        }
                        if source.contains("int f(") {
                            let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                            assert!(f.layout_insts().any(|(_, i)| matches!(
                                f.inst_data(i),
                                InstData::CallIndirect { .. }
                            )));
                        }
                        keel_codegen::compile(&m, target).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn named_factory_and_parenthesized_direct_attributes_are_retained() {
    for declaration in [
        "void (*get(void))(void) __attribute__((noreturn,nothrow,noinline));",
        "void (get)(void) __attribute__((noreturn,nothrow,noinline));",
        "__attribute__((noreturn,nothrow,noinline)) void (*get(void))(void);",
        // The nested noreturn qualifies the returned function, independently
        // of final named declaration policies.
        "void (* __attribute__((noreturn)) get(void))(void) __attribute__((noreturn,nothrow,noinline));",
    ] {
        for target in TARGETS {
            let source = format!("{declaration} int main(void) {{ get(); return 0; }}");
            let m = keel_c::compile(
                &source,
                "direct.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap();
            let get = m.decls.iter().find(|f| f.name == "get").unwrap();
            assert!(
                get.attrs.noreturn && get.attrs.nounwind && get.attrs.noinline,
                "{m}"
            );
            assert!(
                m.functions[0]
                    .layout_insts()
                    .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::Trap))
            );
            keel_ir::verify_module(&m).unwrap();
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_linkage_and_pointer_factories_match_clang() {
    use std::{fs, path::Path, process::Command};
    fn command(cmd: &mut Command) {
        let output = cmd.output().unwrap();
        assert!(output.status.success(), "{cmd:?}: {output:?}");
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("attribute-corrections");
    fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    fs::write(
        &driver,
        "int h(int); int f(int); int main(void) { return h(41)==42 && f(41)==42 ? 0 : 1; }\n",
    )
    .unwrap();
    let mut fixtures = Vec::new();
    for declaration in ["extern", ""] {
        for late in [false, true] {
            for policy in ["", "always_inline", "noinline"] {
                fixtures.push((
                    format!(
                        "link-{}-{late}-{policy}",
                        if declaration.is_empty() {
                            "plain"
                        } else {
                            declaration
                        }
                    ),
                    linkage_source("", declaration, late, policy),
                    true,
                ));
            }
        }
    }
    for attr in ["noreturn", "nothrow", "always_inline", "noinline"] {
        for nested in [false, true] {
            fixtures.push((
                format!("pointer-{attr}-{nested}"),
                pointer_source(attr, nested),
                false,
            ));
        }
    }
    for (name, source, external) in fixtures {
        let c = dir.join(format!("{name}.c"));
        fs::write(&c, &source).unwrap();
        for debug_info in [false, true] {
            for optimized in [false, true] {
                let stem = format!("{name}-debug{debug_info}-opt{optimized}");
                let reference = dir.join(format!("{stem}-clang"));
                let mut clang = Command::new("clang");
                clang
                    .arg("-std=c11")
                    .arg(if optimized { "-O2" } else { "-O0" });
                if debug_info {
                    clang.arg("-g");
                }
                clang.arg(&c);
                if external {
                    clang.arg(&driver);
                }
                command(clang.arg("-o").arg(&reference));
                command(&mut Command::new(&reference));
                let mut m = keel_c::compile(
                    &source,
                    c.to_str().unwrap(),
                    keel_c::Options {
                        debug_info,
                        ..Default::default()
                    },
                )
                .unwrap();
                if optimized {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                let asm = dir.join(format!("{stem}.s"));
                fs::write(&asm, keel_codegen::compile(&m, Default::default()).unwrap()).unwrap();
                let exe = dir.join(&stem);
                let mut clang = Command::new("clang");
                if debug_info {
                    clang.arg("-g");
                }
                clang.arg(&asm);
                if external {
                    clang.arg(&driver);
                }
                command(clang.arg("-o").arg(&exe));
                command(&mut Command::new(&exe));
                println!("{stem}: Keel and Clang exit 0");
            }
        }
    }
}

//! ROUND3: declaration attributes follow the named function, while noreturn
//! still follows its function-type owner. The retained Clang oracle includes a
//! caller in every row so external declarations cannot disappear as unused.
use keel_ir::{Function, FunctionAttrs, InstData, Target};

const TARGETS: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];
const ATTRS: [&str; 4] = ["noinline", "always_inline", "nothrow", "returns_twice"];
fn expected(attr: &str) -> FunctionAttrs {
    FunctionAttrs {
        noinline: attr == "noinline",
        alwaysinline: attr == "always_inline",
        nounwind: attr == "nothrow",
        ..Default::default()
    }
}
fn calls(f: &Function, name: &str) -> bool {
    f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == name))
}
fn source(attr: &str, final_placement: bool) -> String {
    let decl = if final_placement {
        format!("static void (*h(void))(void) __attribute__(({attr}))")
    } else {
        format!("static void (*__attribute__(({attr})) h(void))(void)")
    };
    format!("{decl} {{return 0;}} int main(void){{return h()==0?0:1;}}")
}
#[test]
fn named_factory_contracts_and_inliner_legality() {
    for attr in ATTRS {
        for final_placement in [false, true] {
            let source = source(attr, final_placement);
            let source = if attr == "always_inline" {
                format!("{source} __typeof__(&h) keep_h=h;")
            } else {
                source
            };
            for target in TARGETS {
                for debug_info in [false, true] {
                    let original = keel_c::compile(
                        &source,
                        "factory.c",
                        keel_c::Options { target, debug_info },
                    )
                    .unwrap();
                    let h = original.functions.iter().find(|f| f.name == "h").unwrap();
                    assert_eq!(h.attrs, expected(attr), "{source}");
                    assert_eq!(h.sig.returns_twice, attr == "returns_twice", "{source}");
                    for optimized in [false, true] {
                        let mut m = original.clone();
                        if optimized {
                            keel_opt::optimize(&mut m).unwrap();
                        }
                        keel_ir::verify_module(&m).unwrap();
                        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
                        assert_eq!(
                            calls(main, "h"),
                            attr != "always_inline"
                                && (!optimized || matches!(attr, "noinline" | "returns_twice")),
                            "{source}\n{m}"
                        );
                        assert!(
                            !main
                                .layout_insts()
                                .any(|(_, i)| matches!(main.inst_data(i), InstData::Trap))
                        );
                        keel_codegen::compile(&m, target).unwrap();
                    }
                }
            }
        }
    }
}
#[test]
fn accepted_clang_placement_oracle_preserves_named_declarations() {
    let oracle = include_str!("../fixtures/attribute-placement-oracle.tsv");
    for attr in ATTRS {
        let (mut accepted, mut rejected) = (0, 0);
        for line in oracle.lines() {
            let (named, source) = line.split_once('\t').unwrap();
            let source = source.replace("noinline", attr);
            let Ok(_) = keel_c::compile(&source, "oracle.c", Default::default()) else {
                rejected += 1;
                continue;
            };
            accepted += 1;
            for target in TARGETS {
                for debug_info in [false, true] {
                    let original = keel_c::compile(
                        &source,
                        "oracle.c",
                        keel_c::Options { target, debug_info },
                    )
                    .unwrap();
                    for optimized in [false, true] {
                        let mut m = original.clone();
                        if optimized {
                            keel_opt::optimize(&mut m).unwrap();
                        }
                        keel_ir::verify_module(&m).unwrap();
                        let h = m
                            .decls
                            .iter()
                            .find(|d| d.name == "h")
                            .expect("referenced declaration retained");
                        assert_eq!(
                            h.attrs,
                            if named == "1" {
                                expected(attr)
                            } else {
                                FunctionAttrs::default()
                            },
                            "{source}\n{m}"
                        );
                        assert_eq!(
                            h.sig.returns_twice,
                            named == "1" && attr == "returns_twice",
                            "{source}"
                        );
                        assert!(calls(
                            m.functions.iter().find(|f| f.name == "use").unwrap(),
                            "h"
                        ));
                        keel_codegen::compile(&m, target).unwrap();
                    }
                }
            }
        }
        assert_eq!(
            (accepted, rejected),
            (65, 0),
            "{attr}: all retained Clang placements parse and preserve their named contracts"
        );
    }
}
#[test]
fn mixed_factory_redeclarations_diagnose_named_conflicts() {
    for attr in ["noinline,always_inline", "returns_twice,noreturn"] {
        let source = format!(
            "void (*__attribute__(({attr})) h(void))(void) __attribute__((noreturn)); int use(void){{h();return 0;}}"
        );
        assert!(
            keel_c::compile(&source, "conflict.c", Default::default())
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
    }
    for (first, second) in [("noinline", "always_inline"), ("returns_twice", "noreturn")] {
        for reverse in [false, true] {
            for final_first in [false, true] {
                let a = if final_first {
                    format!("void (*h(void))(void) __attribute__(({first}));")
                } else {
                    format!("void (*__attribute__(({first})) h(void))(void);")
                };
                let b = format!("void (*h(void))(void) __attribute__(({second}));");
                let source = if reverse {
                    format!("{b}{a}")
                } else {
                    format!("{a}{b}")
                } + "int use(void){h();return 0;}";
                let err = keel_c::compile(&source, "conflict.c", Default::default()).unwrap_err();
                assert!(err.to_string().contains("conflicts"), "{source}: {err}");
            }
        }
    }
}

#[test]
fn compatible_factory_redeclarations_accumulate_named_metadata() {
    for reverse in [false, true] {
        let a = "static void (*__attribute__((returns_twice,noinline)) h(void))(void);";
        let b = "static void (*h(void))(void) __attribute__((nothrow));";
        let source = if reverse {
            format!("{b}{a}")
        } else {
            format!("{a}{b}")
        } + "static inline void (*h(void))(void){return 0;} int use(void){return h()==0;}";
        for target in TARGETS {
            for debug_info in [false, true] {
                let original =
                    keel_c::compile(&source, "merge.c", keel_c::Options { target, debug_info })
                        .unwrap();
                for optimized in [false, true] {
                    let mut m = original.clone();
                    if optimized {
                        keel_opt::optimize(&mut m).unwrap();
                    }
                    keel_ir::verify_module(&m).unwrap();
                    let h = m.functions.iter().find(|f| f.name == "h").unwrap();
                    assert_eq!(
                        h.attrs,
                        FunctionAttrs {
                            inlinehint: true,
                            noinline: true,
                            nounwind: true,
                            ..Default::default()
                        }
                    );
                    assert!(h.sig.returns_twice && h.local);
                    assert!(calls(
                        m.functions.iter().find(|f| f.name == "use").unwrap(),
                        "h"
                    ));
                    keel_codegen::compile(&m, target).unwrap();
                }
            }
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_factory_contracts_match_clang() {
    use std::{fs, path::Path, process::Command};
    fn run(cmd: &mut Command) -> std::process::Output {
        let o = cmd.output().unwrap();
        assert!(o.status.success(), "{cmd:?}: {o:?}");
        o
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("attribute-factory-ownership");
    fs::create_dir_all(&dir).unwrap();
    for attr in ATTRS {
        for final_placement in [false, true] {
            let source = source(attr, final_placement);
            let c = dir.join(format!("{attr}-final{final_placement}.c"));
            fs::write(&c, &source).unwrap();
            // Clang only checks that the fixture is valid and exits 0.
            let reference = dir.join(format!("{attr}-final{final_placement}-clang"));
            run(Command::new("clang")
                .args(["-std=c11", "-O0"])
                .arg(&c)
                .arg("-o")
                .arg(&reference));
            run(&mut Command::new(&reference));
            // named_factory_contracts_and_inliner_legality covers debug info
            // on and off.
            for optimized in [false, true] {
                let stem = format!("{attr}-final{final_placement}-opt{optimized}");
                let mut m =
                    keel_c::compile(&source, c.to_str().unwrap(), Default::default()).unwrap();
                if optimized {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                let asm = dir.join(format!("{stem}.s"));
                fs::write(&asm, keel_codegen::compile(&m, Default::default()).unwrap()).unwrap();
                let exe = dir.join(format!("{stem}-keel"));
                run(Command::new("clang")
                    .arg("-std=c11")
                    .arg(if optimized { "-O2" } else { "-O0" })
                    .arg(&asm)
                    .arg("-o")
                    .arg(&exe));
                run(&mut Command::new(&exe));
                println!("{stem}-keel: exit 0");
            }
        }
    }
}

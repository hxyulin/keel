use keel_ir::{AsmClobber, AsmOperand, InstData, Target};
const SOURCE: &str = include_str!("../fixtures/inline_asm.c");

fn compile(source: &str, debug_info: bool) -> keel_ir::Module {
    keel_c::compile(
        source,
        "inline-asm.c",
        keel_c::Options {
            target: Target::Aarch64AppleDarwin,
            debug_info,
        },
    )
    .unwrap()
}

#[test]
fn contracts_survive_optimization_and_emit_macho() {
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut module = compile(SOURCE, debug);
            keel_ir::verify_module(&module).unwrap();
            let asms = |m: &keel_ir::Module| {
                m.functions
                    .iter()
                    .flat_map(|f| {
                        f.layout_insts()
                            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::InlineAsm { .. }))
                    })
                    .count()
            };
            let count = asms(&module);
            assert!(count >= 14);
            let named = module.functions.iter().find(|f| f.name == "named").unwrap();
            let asm = named
                .layout_insts()
                .find_map(|(_, i)| match named.inst_data(i) {
                    InstData::InlineAsm { asm, .. } => Some(asm),
                    _ => None,
                })
                .unwrap();
            assert_eq!(asm.template, "add %x0, %x1, %x2");
            assert_eq!(asm.clobbers, [AsmClobber::Flags]);
            let suffix = module
                .functions
                .iter()
                .find(|f| f.name == "named_digit_suffix")
                .unwrap();
            assert!(suffix.layout_insts().any(|(_, i)| matches!(
                suffix.inst_data(i), InstData::InlineAsm { asm, .. }
                if asm.template == "mov %0, #%c[1]0"
            )));
            assert!(module.functions.iter().any(|f| f.layout_insts().any(|(_, i)|
                matches!(f.inst_data(i), InstData::InlineAsm { asm, .. } if asm.inputs.contains(&AsmOperand::Tied(0))))));
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            assert!(asms(&module) >= count, "pinned asm removed");
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
            let program =
                keel_codegen::compile_program(&module, Target::Aarch64AppleDarwin).unwrap();
            let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(&object[..4], &[0xcf, 0xfa, 0xed, 0xfe]);
        }
    }
}

#[test]
fn invalid_or_unsupported_source_is_diagnosed() {
    let cases = [
        ("asm goto(\"\"::::done); done:;", "asm goto"),
        ("asm(\"\"::::done); done:;", "asm goto"),
        ("asm(\"mov x0, #1\");", "basic asm"),
        ("asm(\"\" : \"=m\"(x));", "output constraint"),
        ("asm(\"\" : \"+m\"(x));", "output constraint"),
        ("asm(\"\" : \"=rm\"(x));", "output constraint"),
        ("asm(\"\" : \"=r,r\"(x));", "output constraint"),
        ("asm(\"\" : \"=r\"(1));", "not assignable"),
        ("const int c=1; asm(\"\" : \"=r\"(c));", "modifiable"),
        (
            "typedef const int C; C c=1; asm(\"\" : \"+r\"(c));",
            "modifiable",
        ),
        (
            "const struct S {int x;} s={1}; asm(\"\" : \"=r\"(s.x));",
            "modifiable",
        ),
        ("_Atomic int c; asm(\"\" : \"+r\"(c));", "non-atomic"),
        ("double _Complex c; asm(\"\" : \"=w\"(c));", "operand type"),
        ("double _Complex c; asm(\"\" :: \"w\"(c));", "operand type"),
        (
            "typedef int V __attribute__((vector_size(16))); V v; asm(\"\" :: \"r\"(v));",
            "operand type",
        ),
        ("asm(\"\" :: \"g\"(x));", "input constraint"),
        ("asm(\"\" :: \"{x0}\"(x));", "input constraint"),
        ("asm(\"\" :: \"i\"(x));", "integer constant"),
        ("asm(\"\" :: \"i\"((x++, 7)));", "integer constant"),
        ("asm(\"\" :: \"m\"(x+1));", "not assignable"),
        (
            "struct S {int a:3;} s; asm(\"\" : \"=r\"(s.a));",
            "bit-field",
        ),
        (
            "struct S {int a:3;} s; asm(\"\" :: \"m\"(s.a));",
            "bit-field",
        ),
        ("asm(\"\" : \"=w\"(x));", "operand type"),
        ("__int128 y=1; asm(\"\" :: \"r\"(y));", "operand type"),
        (
            "double d; asm(\"\" : \"=w\"(d) : \"0\"(x));",
            "operand type",
        ),
        ("asm(\"\" : \"=r\"(x) : \"1\"(x));", "input constraint"),
        ("asm(\"\" : [a] \"=r\"(x) : [a] \"r\"(x));", "duplicate"),
        ("asm(\"%[absent]\" :: \"r\"(x));", "unknown asm operand"),
        ("asm(\"%1\" :: \"r\"(x));", "out of range"),
        ("asm(\"%2\" : \"+r\"(x) : \"r\"(x));", "out of range"),
        ("asm(\"%[bad\" :: \"r\"(x));", "unclosed"),
        ("asm(\"%=\" :::);", "requires an operand"),
        ("asm(\"%z0\" :: \"r\"(x));", "modifier"),
        ("asm(\"\" ::: \"sp\");", "another architecture"),
        ("asm(\"\" ::: \"x18\");", "reserved"),
        ("asm(\"\" ::: \"x29\");", "frame pointer"),
        ("asm(\"\" ::: \"w18\");", "reserved"),
        ("asm(\"\" ::: \"fp\");", "frame pointer"),
        ("long y __asm__(\"_x0\");", "automatic locals/register"),
        ("asm(L\"nop\");", "narrow"),
        ("asm(\"nop\\0\");", "embedded NUL"),
    ];
    for (body, expected) in cases {
        let source = format!("void f(int x) {{ {body} }}");
        let err = keel_c::compile(
            &source,
            "bad-asm.c",
            keel_c::Options {
                target: Target::Aarch64AppleDarwin,
                debug_info: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains(expected),
            "expected {expected}: {source}\n{err}"
        );
    }
    let module = keel_c::compile(
        "void f(void){asm(\"nop\");}",
        "x86.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
}

#[test]
fn backend_operand_contracts_are_enforced() {
    // Equivalent matching initializers share one physical input register.
    for source in [
        "void f(int x){asm(\"\" : \"+r\"(x) : \"0\"(x));}",
        "void f(int x){asm(\"\" : \"=r\"(x) : \"0\"(x), \"0\"(x));}",
    ] {
        let module = compile(source, false);
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    }
    for (source, expected) in [
        (
            "void f(int n){int a[n]; asm(\"\" ::: \"x19\");}",
            "base pointer",
        ),
        ("void f(long x){asm(\"%d0\" :: \"r\"(x));}", "does not fit"),
        (
            "void f(long x){asm(\"\" :: \"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x),\"r\"(x));}",
            "too many asm operands",
        ),
        (
            "void f(long x){asm(\"\" :: \"r\"(x) : \"w0\",\"w1\",\"w2\",\"w3\",\"w4\",\"w5\",\"w6\",\"w7\",\"w8\",\"x16\",\"x17\");}",
            "too many asm operands",
        ),
        (
            "void f(double x){asm(\"\" :: \"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x),\"w\"(x));}",
            "too many asm operands",
        ),
    ] {
        let module = compile(source, false);
        keel_ir::verify_module(&module).unwrap();
        let err = keel_codegen::compile(&module, Target::Aarch64AppleDarwin)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{err}");
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_clang_differential_from_actual_objects() {
    use std::process::Command;
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-inline-asm");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("test.c");
    std::fs::write(&source, SOURCE).unwrap();
    let reference = dir.join("clang");
    let status = Command::new("clang")
        .args(["-std=gnu11", "-O2"])
        .arg(&source)
        .arg("-o")
        .arg(&reference)
        .status()
        .unwrap();
    assert!(status.success());
    let want = Command::new(&reference).output().unwrap();
    assert!(
        want.status.success(),
        "reference: {}",
        String::from_utf8_lossy(&want.stdout)
    );
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut module = compile(SOURCE, debug);
            keel_ir::verify_module(&module).unwrap();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let program =
                keel_codegen::compile_program(&module, Target::Aarch64AppleDarwin).unwrap();
            let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            let path = dir.join(format!("test-{debug}-{optimize}.o"));
            let exe = dir.join(format!("test-{debug}-{optimize}"));
            std::fs::write(&path, object).unwrap();
            let linked = Command::new("clang")
                .arg(&path)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            let got = Command::new(&exe).output().unwrap();
            assert_eq!(
                got.status.code(),
                want.status.code(),
                "debug={debug} optimize={optimize}: {}",
                String::from_utf8_lossy(&got.stdout)
            );
            assert_eq!(got.stdout, want.stdout, "debug={debug} optimize={optimize}");
        }
    }
}

#[test]
fn unused_outputs_and_empty_barriers_are_pinned() {
    let source = r#"
        void keep(long x) {
            long unused;
            asm inline("mov %0, %1" : "=r"(unused) : "r"(x));
            __asm__ __volatile__("" ::: "memory", "cc", "x9");
        }
    "#;
    let mut module = compile(source, false);
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = module.functions.iter().find(|f| f.name == "keep").unwrap();
    let contracts: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| match f.inst_data(i) {
            InstData::InlineAsm { asm, .. } => Some(asm.constraints()),
            _ => None,
        })
        .collect();
    assert_eq!(contracts, ["=r,r", "~{memory},~{cc},~{x9}"]);
}

#[test]
fn symbol_labels_still_work_on_both_targets() {
    for (target, label) in [
        (Target::Aarch64AppleDarwin, "_renamed"),
        (Target::X86_64LinuxGnu, "renamed"),
    ] {
        let source = format!(
            "extern long original(long) __asm__(\"{label}\"); long f(long x){{return original(x);}}"
        );
        let m = keel_c::compile(
            &source,
            "labels.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        let text = keel_codegen::compile(&m, target).unwrap();
        assert!(text.contains(label));
    }
}

use keel_ir::{InstData, Intrinsic, Target, Type, ValueDef};
use std::path::Path;
use std::process::Command;

fn compile(source: &str, target: Target) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "memory-builtins.c",
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
fn explicit_memory_builtins_use_existing_intrinsics_and_return_destination() {
    for target in Target::ALL {
        for (name, id) in [
            ("memcpy", Intrinsic::Memcpy),
            ("memmove", Intrinsic::Memmove),
            ("memset", Intrinsic::Memset),
        ] {
            for length in ["0", "7", "4096", "n"] {
                let second = if id == Intrinsic::Memset { "fill" } else { "s" };
                let source = format!(
                    "void *f(void *d, const void *s, int fill, unsigned long n) {{ return __builtin_{name}(d, {second}, {length}); }}"
                );
                let mut module = compile(&source, target);
                let f = module.functions.iter().find(|f| f.name == "f").unwrap();
                let intrinsic_args = f
                    .layout_insts()
                    .find_map(|(_, i)| match f.inst_data(i) {
                        InstData::Intrinsic { id: found, args } if *found == id => Some(args),
                        _ => None,
                    })
                    .expect("explicit memory intrinsic");
                assert_eq!(f.value_type(intrinsic_args[2]), Type::I64);
                if id == Intrinsic::Memset {
                    assert_eq!(f.value_type(intrinsic_args[1]), Type::I8);
                }
                if let Ok(n) = length.parse::<i64>() {
                    let ValueDef::Result(inst, _) = f.value_def(f.resolve(intrinsic_args[2]))
                    else {
                        panic!("constant length lost")
                    };
                    assert!(matches!(f.inst_data(inst), InstData::Iconst { imm, .. } if *imm == n));
                }
                assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Return { args } if f.resolve(args[0]) == f.resolve(intrinsic_args[0]))));
                assert!(
                    !f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. }))
                );
                keel_codegen::compile(&module, target).unwrap();
                keel_opt::optimize(&mut module).unwrap();
                keel_ir::verify_module(&module).unwrap();
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
}

#[test]
fn library_builtin_designators_keep_addresses_and_parenthesized_calls() {
    for target in Target::ALL {
        for name in ["memcpy", "memmove", "memset"] {
            let second = if name == "memset" {
                "int"
            } else {
                "const void *"
            };
            let source = format!(
                "void *{name}(void *, {second}, unsigned long); void *(*p)(void *, {second}, unsigned long) = __builtin_{name}; void *(*q)(void *, {second}, unsigned long) = &__builtin_{name}; void *parenthesized(void *d, {second} s) {{ return (__builtin_{name})(d,s,1); }} void *direct(void *d, {second} s) {{ return __builtin_{name}(d,s,1); }}"
            );
            let mut module = compile(&source, target);
            let parenthesized = module
                .functions
                .iter()
                .find(|f| f.name == "parenthesized")
                .unwrap();
            assert!(parenthesized.layout_insts().any(|(_, i)| matches!(parenthesized.inst_data(i), InstData::Call { func, .. } if parenthesized.callee(*func).name == name)));
            let direct = module
                .functions
                .iter()
                .find(|f| f.name == "direct")
                .unwrap();
            assert!(direct.layout_insts().any(|(_, i)| matches!(direct.inst_data(i), InstData::Intrinsic { id, .. } if id.info().name == name)));
            assert!(module.data.iter().any(|data| data.name == "p"));
            assert!(module.data.iter().any(|data| data.name == "q"));
            keel_codegen::compile(&module, target).unwrap();
            keel_opt::optimize(&mut module).unwrap();
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn memory_null_pointers_accept_zero_integer_constant_expressions_only() {
    for target in Target::ALL {
        for zero in [
            "1-1",
            "ZERO",
            "sizeof(char)-1",
            "(int)0.0",
            "(unsigned long)(2-2)",
            "0 && effect()",
        ] {
            for name in ["memcpy", "memmove", "memset"] {
                let source = format!(
                    "enum {{ ZERO = 2-2 }}; int effect(void); unsigned long f(void) {{ return sizeof(__builtin_{name}({zero},0,0)); }}"
                );
                // Unevaluated: valid null-pointer typing does not execute a
                // library call with invalid pointers, even at length zero.
                compile(&source, target);
                if name != "memset" {
                    compile(
                        &format!(
                            "enum {{ ZERO = 2-2 }}; int effect(void); unsigned long f(void) {{ return sizeof(__builtin_{name}(0,{zero},0)); }}"
                        ),
                        target,
                    );
                }
            }
        }
        for value in ["n", "n-n", "1-0", "(effect(),0)", "(0,0)", "1.0-1.0"] {
            let source = format!(
                "int effect(void); unsigned long f(int n) {{ return sizeof(__builtin_memcpy({value},0,0)); }}"
            );
            let error = keel_c::compile(
                &source,
                "non-null-memory.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("memory builtin needs an object pointer"),
                "{source}\n{error}"
            );
        }
        // The explicitly requested sizeof reducer is kept verbatim.
        compile(
            "unsigned long f(void) { return sizeof(__builtin_memcpy(1-1, 0, 0)); }",
            target,
        );
    }
}

#[test]
fn checked_memory_returns_twice_never_creates_mismatched_signatures() {
    for target in Target::ALL {
        for source in [
            "void *__memcpy_chk(void *, const void *, unsigned long, unsigned long) __attribute__((returns_twice)); void *f(void *d, const void *s) { return __builtin___memcpy_chk(d,s,1,1); }",
            "void *f(void *d, const void *s) { return __builtin___memcpy_chk(d,s,1,1); } void *__memcpy_chk(void *, const void *, unsigned long, unsigned long) __attribute__((returns_twice));",
        ] {
            let error = keel_c::compile(
                source,
                "checked-attributes-exact.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("incompatible checked memory builtin attributes: returns_twice"),
                "{error}"
            );
        }
        for name in ["memcpy", "memmove", "memset"] {
            let second_ty = if name == "memset" {
                "int"
            } else {
                "const void *"
            };
            let second = if name == "memset" { "257" } else { "s" };
            let prototype =
                format!("void *__{name}_chk(void *, {second_ty}, unsigned long, unsigned long)");
            for late_attribute in [false, true] {
                for ordinary_first in [false, true] {
                    let builtin = format!("__builtin___{name}_chk(d,{second},1,1)");
                    let ordinary = format!("__{name}_chk(d,{second},1,1)");
                    let body = if ordinary_first {
                        format!("{ordinary}; return {builtin};")
                    } else {
                        format!("{builtin}; return {ordinary};")
                    };
                    let attribute = format!("{prototype} __attribute__((returns_twice));");
                    let function = format!("void *f(void *d, const void *s) {{ {body} }}");
                    let source = if late_attribute {
                        format!("{prototype}; {function} {attribute}")
                    } else {
                        format!("{attribute} {function}")
                    };
                    let error = keel_c::compile(
                        &source,
                        "checked-attributes.c",
                        keel_c::Options {
                            target,
                            ..Default::default()
                        },
                    )
                    .unwrap_err()
                    .to_string();
                    assert!(
                        error.contains(
                            "incompatible checked memory builtin attributes: returns_twice"
                        ),
                        "{source}\n{error}"
                    );
                    // Without the incompatible attribute both import orders
                    // must produce a verifiable module and valid backend ABI.
                    let valid = format!("{prototype}; {function}");
                    let module = compile(&valid, target);
                    keel_codegen::compile(&module, target).unwrap();
                }
            }
        }
    }
}

#[test]
fn every_checked_memory_redeclaration_is_validated_before_merging() {
    for target in Target::ALL {
        for name in ["memcpy", "memmove", "memset"] {
            let second_ty = if name == "memset" {
                "int"
            } else {
                "const void *"
            };
            let second = if name == "memset" { "0" } else { "s" };
            let canonical =
                format!("void *__{name}_chk(void *, {second_ty}, unsigned long, unsigned long);");
            let use_builtin = format!(
                "void *f(void *d, const void *s) {{ return __builtin___{name}_chk(d,{second},1,1); }}"
            );
            let conflicts = [
                format!("int __{name}_chk(void *, {second_ty}, unsigned long, unsigned long);"),
                format!("void *__{name}_chk(void *, {second_ty}, unsigned long);"),
                format!("void *__{name}_chk(void *, {second_ty}, unsigned, unsigned long);"),
                format!(
                    "void *__{name}_chk(void *d, {second_ty} s, unsigned n, unsigned long size) {{ return d; }}"
                ),
            ];
            for conflict in conflicts {
                for late in [false, true] {
                    let source = if late {
                        format!("{canonical} {use_builtin} {conflict}")
                    } else {
                        format!("{canonical} {conflict} {use_builtin}")
                    };
                    let error = keel_c::compile(
                        &source,
                        "checked-redeclaration.c",
                        keel_c::Options {
                            target,
                            ..Default::default()
                        },
                    )
                    .unwrap_err()
                    .to_string();
                    assert!(
                        error.contains("invalid checked memory builtin prototype"),
                        "{source}\n{error}"
                    );
                }
            }
            let definition = format!(
                "void *__{name}_chk(void *d, {second_ty} s, unsigned long n, unsigned long size) {{ return d; }}"
            );
            let module = compile(
                &format!("{canonical} {use_builtin} {canonical} {definition}"),
                target,
            );
            keel_codegen::compile(&module, target).unwrap();
        }
        // The narrowly scoped validation leaves the pre-existing policy for
        // unrelated function redeclarations alone.
        compile(
            "void *other(void *); int other(void *); int f(void) { return 0; }",
            target,
        );
    }
}

#[test]
fn explicit_pointer_casts_set_memory_builtin_qualifiers() {
    for target in Target::ALL {
        for source in [
            "void *f(volatile int *p) { return __builtin_memset((void *)p,0,1); }",
            "void *f(void *d, const volatile int *p) { return __builtin_memcpy(d,(const void *)p,1); }",
            "void *f(const int *p) { return __builtin_memset((void *)p,0,0); }",
        ] {
            // Typing/codegen only: the last case must not be used to write
            // an object defined const. Cast sugar follows the target type.
            let module = compile(source, target);
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn ordinary_memory_symbols_keep_calls_and_volatile_arguments_keep_accesses() {
    let source = "void *memcpy(void *, const void *, unsigned long); void *f(void *d, const void *s) { return memcpy(d,s,7); }";
    for target in Target::ALL {
        let module = compile(source, target);
        let f = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == "memcpy")));
        assert!(!module.to_string().contains("intrinsic memcpy"));
        let module = compile(
            "void *f(void *volatile *d, const void *volatile *s, volatile unsigned long *n) { return __builtin_memcpy(*d, *s, *n); }",
            target,
        );
        let f = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert_eq!(f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { flags, .. } if flags.volatile)).count(), 3);
    }
}

#[test]
fn zero_length_still_evaluates_each_argument_once() {
    for target in Target::ALL {
        for (name, second) in [("memcpy", "s()"), ("memmove", "s()"), ("memset", "fill()")] {
            let source = format!(
                "void *d(void); const void *s(void); int fill(void); int zero(void); void *f(void) {{ return __builtin_{name}(d(),{second},(zero(),0)); }}"
            );
            let module = compile(&source, target);
            let f = module.functions.iter().find(|f| f.name == "f").unwrap();
            let calls: Vec<_> = f
                .layout_insts()
                .filter_map(|(_, i)| match f.inst_data(i) {
                    InstData::Call { func, .. } => Some(f.callee(*func).name.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                calls,
                ["d", if name == "memset" { "fill" } else { "s" }, "zero"]
            );
        }
    }
}

#[test]
fn memory_builtin_argument_diagnostics() {
    for source in [
        "void f(void *p) { __builtin_memcpy(p,p); }",
        "void f(void *p) { __builtin_memmove(p,p,1,2); }",
        "void f(void *p) { __builtin_memcpy(1,p,0); }",
        "void f(void *p) { __builtin_memcpy(p,1,0); }",
        "void f(void *p) { __builtin_memcpy(p,p,p); }",
        "void f(void *p) { __builtin_memset(p,p,1); }",
        "void f(void *p) { __builtin_memcpy(1.0,p,0); }",
        "void f(void *p) { __builtin_memcpy(p,f,0); }",
        "void f(const int *p) { __builtin_memset(p,1,0); }",
        "void f(volatile int *p) { __builtin_memset(p,1,0); }",
        "void f(void *d, volatile int *p) { __builtin_memmove(d,p,0); }",
        "typedef const int C; void f(C *p) { __builtin_memset(p,1,0); }",
        "void f(const int *restrict p) { __builtin_memset(p,1,0); }",
        "void f(int *const restrict *p) { __builtin_memset(p,1,0); }",
    ] {
        let error = keel_c::compile(source, "invalid-memory.c", Default::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("builtin") && error.contains("invalid-memory.c:1:"),
            "{source}\n{error}"
        );
    }
    // Null pointer constants are a typing issue, separate from runtime
    // pointer validity. Do not execute calls with invalid pointers.
    compile(
        "void f(void) { __builtin_memcpy(0,0,0); }",
        Target::Aarch64AppleDarwin,
    );
    compile(
        "void f(void *p) { __builtin_memset(p,257.0,7.9); }",
        Target::Aarch64AppleDarwin,
    );
}

#[test]
fn checked_memory_calls_preserve_bound_and_prototype_checks() {
    for target in Target::ALL {
        for name in ["memcpy", "memmove", "memset"] {
            let second = if name == "memset" { "257" } else { "s" };
            for bound in ["3", "bound()", "__builtin_object_size(d,0)"] {
                let source = format!(
                    "unsigned long bound(void); void *f(void *d, const void *s) {{ return __builtin___{name}_chk(d,{second},7,{bound}); }}"
                );
                let module = compile(&source, target);
                let f = module.functions.iter().find(|f| f.name == "f").unwrap();
                let args = f
                    .layout_insts()
                    .find_map(|(_, i)| match f.inst_data(i) {
                        InstData::Call { func, args }
                            if f.callee(*func).name == format!("__{name}_chk") =>
                        {
                            Some(args)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(args.len(), 4, "{module}");
                if bound == "bound()" {
                    assert_eq!(f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { func, .. } if f.callee(*func).name == "bound")).count(), 1);
                }
                assert!(!module.to_string().contains("intrinsic mem"));
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
    for source in [
        "int __memcpy_chk(void *, const void *, unsigned long, unsigned long); void f(void *p) { __builtin___memcpy_chk(p,p,7,3); }",
        "void *__memcpy_chk(void *, const void *, unsigned long); void f(void *p) { __builtin___memcpy_chk(p,p,7,3); }",
        "void *__memset_chk(); void f(void *p) { __builtin___memset_chk(p,0,7,3); }",
        "void f(void *p) { __builtin___memcpy_chk(p,p,7,3); } int __memcpy_chk(void *, const void *, unsigned long, unsigned long);",
        "int __memmove_chk; void f(void *p) { __builtin___memmove_chk(p,p,7,3); }",
    ] {
        let error = keel_c::compile(source, "checked.c", Default::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid checked memory builtin prototype"),
            "{error}"
        );
    }
}

#[test]
fn checked_memory_runtime_declarations_are_canonical_and_reusable() {
    for target in Target::ALL {
        for name in ["memcpy", "memmove", "memset"] {
            for declared in [false, true] {
                let second = if name == "memset" {
                    "int"
                } else {
                    "const void *"
                };
                let declaration = if declared {
                    format!("void *__{name}_chk(void *, {second}, unsigned long, unsigned long);")
                } else {
                    String::new()
                };
                let value = if name == "memset" { "257" } else { "s" };
                let source = format!(
                    "{declaration} void *f(void *d, const void *s) {{ return __builtin___{name}_chk(d,{value},7,3); }} void *g(void *d, const void *s) {{ return __builtin___{name}_chk(d,{value},7,3); }}"
                );
                let mut module = compile(&source, target);
                let runtime = format!("__{name}_chk");
                assert_eq!(
                    module
                        .decls
                        .iter()
                        .filter(|decl| decl.name == runtime)
                        .count(),
                    1
                );
                let decl = module
                    .decls
                    .iter()
                    .find(|decl| decl.name == runtime)
                    .unwrap();
                assert_eq!(
                    decl.sig.param_types(),
                    [
                        Type::Ptr,
                        if name == "memset" {
                            Type::I32
                        } else {
                            Type::Ptr
                        },
                        Type::I64,
                        Type::I64
                    ]
                );
                assert_eq!(decl.sig.return_types(), [Type::Ptr]);
                keel_opt::optimize(&mut module).unwrap();
                keel_ir::verify_module(&module).unwrap();
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
}

#[test]
fn memory_builtin_feature_queries_match_support() {
    let source = "#if !__has_builtin(__builtin_memcpy) || !__has_builtin(__builtin_memmove) || !__has_builtin(__builtin_memset)\n#error missing memory builtin\n#endif\n#if __has_builtin(__builtin___memcpy_chk) || __has_builtin(__builtin___memmove_chk) || __has_builtin(__builtin___memset_chk)\n#error conditional checked calls must not be advertised\n#endif\nint f(void) { return 0; }\n";
    for target in Target::ALL {
        let text = keel_c::preprocess(
            source,
            "features.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        compile(&text, target);
    }
}

#[test]
fn native_fortify_headers_have_explicit_compatibility_behavior() {
    let source = "#include <string.h>\nvoid *f(void *d, const void *s, unsigned long n) { memcpy(d,s,n); memmove(d,s,n); return memset(d,257,n); }\n";
    let sysroot = if cfg!(target_os = "macos") {
        Some(
            String::from_utf8(
                checked(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"])).stdout,
            )
            .unwrap()
            .trim()
            .into(),
        )
    } else {
        None
    };
    let text = keel_c::preprocess(
        source,
        "fortify-memory.c",
        &keel_c::PreprocessOptions {
            sysroot,
            defines: vec![
                ("_FORTIFY_SOURCE".into(), Some("2".into())),
                ("__OPTIMIZE__".into(), Some("1".into())),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    // Darwin gates memory fortification on __has_builtin; Keel must leave
    // these as ordinary calls. Other SDKs may select checked builtins,
    // whose canonical prototypes preserve all four arguments.
    if cfg!(target_os = "macos") {
        let module = compile(&text, keel_c::Options::default().target);
        let f = module.functions.iter().find(|f| f.name == "f").unwrap();
        let calls: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                InstData::Call { func, .. } => Some(f.callee(*func).name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(calls, ["memcpy", "memmove", "memset"]);
    } else {
        let module = compile(&text, keel_c::Options::default().target);
        keel_ir::verify_module(&module).unwrap();
    }
}

fn checked(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn memory_builtins_native_differential_clang() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/memory_builtins.c");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-builtins-differential");
    std::fs::create_dir_all(&dir).unwrap();
    let mut want = None;
    for level in ["-O0", "-O2"] {
        let exe = dir.join(format!("clang{level}"));
        checked(
            Command::new("clang")
                .args(["-std=c11", "-fno-builtin", level, "-o"])
                .arg(&exe)
                .arg(&source),
        );
        let output = checked(&mut Command::new(&exe)).stdout;
        assert!(
            output.starts_with(b"0 "),
            "oracle failed: {}",
            String::from_utf8_lossy(&output)
        );
        if let Some(want) = &want {
            assert_eq!(&output, want);
        } else {
            want = Some(output);
        }
    }
    let source = std::fs::read_to_string(source).unwrap();
    for optimized in [false, true] {
        let mut module = compile(&source, keel_c::Options::default().target);
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let asm = keel_codegen::compile(&module, keel_c::Options::default().target).unwrap();
        let assembly = dir.join(format!("keel-{optimized}.s"));
        let executable = dir.join(format!("keel-{optimized}"));
        std::fs::write(&assembly, asm).unwrap();
        checked(
            Command::new("clang")
                .arg(&assembly)
                .arg("-o")
                .arg(&executable),
        );
        assert_eq!(
            checked(&mut Command::new(&executable)).stdout,
            *want.as_ref().unwrap(),
            "optimized={optimized}"
        );
    }
}

#[test]
fn finite_checked_bound_fails_in_child_without_out_of_bounds_storage() {
    // The actual allocation can hold the entire write. Only the explicit
    // smaller bound must cause failure, so an unchecked call would exit 0.
    let source = "int main(void) { unsigned char d[64]; unsigned long n = 8, bound = 1; return __builtin___memset_chk(d,257,n,bound) != d; }";
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-checked-bound-failure");
    std::fs::create_dir_all(&dir).unwrap();
    let c_source = dir.join("failure.c");
    std::fs::write(&c_source, source).unwrap();
    let oracle = dir.join("clang");
    checked(
        Command::new("clang")
            .args(["-std=c11", "-O0", "-o"])
            .arg(&oracle)
            .arg(&c_source),
    );
    let outcome = Command::new(&oracle).current_dir(&dir).output().unwrap();
    assert!(
        !outcome.status.success(),
        "Clang oracle unexpectedly ignored the finite bound"
    );
    for optimized in [false, true] {
        let mut module = compile(source, keel_c::Options::default().target);
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let asm = keel_codegen::compile(&module, keel_c::Options::default().target).unwrap();
        let assembly = dir.join(format!("keel-{optimized}.s"));
        let executable = dir.join(format!("keel-{optimized}"));
        std::fs::write(&assembly, asm).unwrap();
        checked(
            Command::new("clang")
                .arg(&assembly)
                .arg("-o")
                .arg(&executable),
        );
        let outcome = Command::new(&executable)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            !outcome.status.success(),
            "optimized={optimized} ignored the finite checked bound"
        );
    }
}

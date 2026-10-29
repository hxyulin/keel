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
fn prefetch_accepts_optional_constant_controls_without_memory_access() {
    for target in Target::NATIVE {
        let mut module = compile(
            "void f(void *p) { __builtin_prefetch(p); __builtin_prefetch(p,1); __builtin_prefetch(p,2,0); __builtin_prefetch(p,0,3); __builtin_prefetch(0); }",
            target,
        );
        let f = &module.functions[0];
        assert!(!f.layout_insts().any(|(_, i)| matches!(
            f.inst_data(i),
            InstData::Load { .. } | InstData::Call { .. } | InstData::Intrinsic { .. }
        )));
        keel_codegen::compile(&module, target).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, target).unwrap();
        let mut volatile = compile(
            "void f(void * volatile *p) { __builtin_prefetch(*p); }",
            target,
        );
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut volatile).unwrap();
            }
            let f = &volatile.functions[0];
            assert_eq!(f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { flags, .. } if flags.volatile)).count(), 1, "prefetch must evaluate its volatile address operand");
        }
    }
}

#[test]
fn prefetch_rejects_invalid_addresses_and_controls() {
    for expression in [
        "__builtin_prefetch()",
        "__builtin_prefetch(p,0,3,0)",
        "__builtin_prefetch(42)",
        "__builtin_prefetch(p,n)",
        "__builtin_prefetch(p,-1)",
        "__builtin_prefetch(p,3)",
        "__builtin_prefetch(p,0,4)",
        "__builtin_prefetch(p,0,1.0)",
        "__builtin_prefetch(p,0,(n++,3))",
        "int value = __builtin_prefetch(p)",
    ] {
        let source = format!("void f(void *p, int n) {{ {expression}; }}");
        assert!(
            keel_c::compile(&source, "prefetch.c", Default::default()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn prefetch_evaluates_address_side_effects_once_without_dereferencing_it() {
    let source = "volatile int count; void *next(void) { ++count; return (void *)0; } int main(void) { int a[2]; int *p = a; void * volatile v = 0; __builtin_prefetch(next()); __builtin_prefetch(p++,1,0); __builtin_prefetch(v,0,3); return count != 1 || p != a+1; }";
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prefetch-side-effects");
    std::fs::create_dir_all(&dir).unwrap();
    let c_source = dir.join("prefetch.c");
    std::fs::write(&c_source, source).unwrap();
    let oracle = dir.join("clang");
    checked(Command::new("clang").arg(&c_source).arg("-o").arg(&oracle));
    checked(&mut Command::new(oracle));
    for optimized in [false, true] {
        let target = keel_c::Options::default().target;
        let mut module = compile(source, target);
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let asm = keel_codegen::compile(&module, target).unwrap();
        let assembly = dir.join(format!("keel-{optimized}.s"));
        let executable = dir.join(format!("keel-{optimized}"));
        std::fs::write(&assembly, asm).unwrap();
        checked(
            Command::new("clang")
                .arg(&assembly)
                .arg("-o")
                .arg(&executable),
        );
        checked(&mut Command::new(executable));
    }
}

#[test]
fn explicit_memory_builtins_use_existing_intrinsics_and_return_destination() {
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
        for name in ["memcpy", "memmove", "memset"] {
            let second = if name == "memset" {
                "int"
            } else {
                "const void *"
            };
            let source = format!(
                "void *{name}(void *, {second}, unsigned long); void *(*p)(void *, {second}, unsigned long) = __builtin_{name}; void *(*q)(void *, {second}, unsigned long) = &__builtin_{name}; void *parenthesized(void *d, {second} s) {{ return (__builtin_{name})(d,s,1); }} void *direct(void *d, {second} s) {{ return __builtin_{name}(d,s,1); }}"
            );
            let mut module = keel_c::compile_with_policies(
                &source,
                "memory-builtins.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
                keel_c::FunctionBinding::default(),
                keel_c::LibraryBuiltins::disabled(),
            )
            .unwrap();
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
fn disabled_memory_symbols_keep_calls_and_volatile_arguments_keep_accesses() {
    let source = "void *memcpy(void *, const void *, unsigned long); void *f(void *d, const void *s) { return memcpy(d,s,7); }";
    for target in Target::NATIVE {
        let module = keel_c::compile_with_policies(
            source,
            "memory-builtins.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
            keel_c::FunctionBinding::default(),
            keel_c::LibraryBuiltins::disabled(),
        )
        .unwrap();
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
        for name in ["memcpy", "memmove", "memset"] {
            let second = if name == "memset" { "257" } else { "s" };
            for bound in ["3", "bound()"] {
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
    // Darwin gates memory fortification on __has_builtin; Keel leaves
    // canonical ordinary calls available for hosted builtin recognition. Other SDKs may select checked builtins,
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
        assert!(calls.is_empty(), "{module}");
        for name in ["memcpy", "memmove", "memset"] {
            assert!(
                module.to_string().contains(&format!("intrinsic {name}")),
                "{module}"
            );
        }
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

#[test]
fn hosted_canonical_memory_calls_have_frontend_intrinsic_provenance() {
    for target in Target::NATIVE {
        for (name, id, second, parameter) in [
            ("memcpy", Intrinsic::Memcpy, "s", "const void *"),
            ("memmove", Intrinsic::Memmove, "s", "const void *"),
            ("memset", Intrinsic::Memset, "13", "int"),
        ] {
            let source = format!(
                "void *{name}(void *, {parameter}, unsigned long); void *f(void *d, const void *s, unsigned long n) {{ return ({name})(d,{second},n); }}"
            );
            let module = compile(&source, target);
            let f = module.functions.iter().find(|f| f.name == "f").unwrap();
            assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Intrinsic { id: got, .. } if *got == id)), "{module}");
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. })),
                "{module}"
            );
            let policy = keel_c::LibraryBuiltins::hosted().disable(match name {
                "memcpy" => keel_c::LibraryBuiltin::Memcpy,
                "memmove" => keel_c::LibraryBuiltin::Memmove,
                _ => keel_c::LibraryBuiltin::Memset,
            });
            let module = keel_c::compile_with_policies(
                &source,
                "memory-builtins.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
                keel_c::FunctionBinding::default(),
                policy,
            )
            .unwrap();
            let f = module.functions.iter().find(|f| f.name == "f").unwrap();
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. })),
                "{module}"
            );
            let explicit = source.replace(&format!("({name})("), &format!("__builtin_{name}("));
            let module = keel_c::compile_with_policies(
                &explicit,
                "memory-builtins.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
                keel_c::FunctionBinding::default(),
                keel_c::LibraryBuiltins::disabled(),
            )
            .unwrap();
            assert!(
                module.to_string().contains(&format!("intrinsic {name}")),
                "{module}"
            );
        }
    }
}

#[test]
fn library_memory_recognition_requires_final_unambiguous_external_declarations() {
    let cases = [
        "void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s) { return ((void *(*)(void *,const void *,unsigned int))memcpy)(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s) { return ((void *(*)(void *,const void *,unsigned long))(void *)memcpy)(d,s,2); }",
        "const void *memcpy(void *,const void *,unsigned long); const void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "volatile void *memcpy(void *,const void *,unsigned long); volatile void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "const void *memcpy(void *,const void *,unsigned long); void *memcpy(void *,const void *,unsigned long); const void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "static void *memcpy(void *d,const void *s,unsigned long n) { return d; } void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s) { return memcpy(d,s,2); } void *memcpy(void *d,const void *s,unsigned long n) { return d; }",
        "void *memcpy(void *,const void *,unsigned long) __attribute__((weak)); void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned long) __asm__(\"_custom_copy\"); void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "int memcpy(void *,const void *,unsigned long); int f(void *d,const void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned int); void *f(void *d,const void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,void *,unsigned long); void *f(void *d,void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const volatile void *,unsigned long); void *f(void *d,const volatile void *s) { return memcpy(d,s,2); }",
        "void *memcpy(); void *f(void *d,const void *s) { return memcpy(d,s,2UL); } void *memcpy(void *,const void *,unsigned long);",
        "int memcpy(void *,const void *,unsigned long); void *memcpy(void *,const void *,unsigned long); int f(void *d,const void *s) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s,void *(*memcpy)(void *,const void *,unsigned long)) { return memcpy(d,s,2); }",
        "void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s) { void *(*p)(void *,const void *,unsigned long)=memcpy; return p(d,s,2); }",
    ];
    for target in Target::NATIVE {
        for source in cases {
            let source = if target == Target::Aarch64AppleDarwin {
                source.replace("__asm__(\"memcpy\")", "__asm__(\"_memcpy\")")
            } else {
                source.to_string()
            };
            let module = compile(&source, target);
            assert!(
                !module.to_string().contains("intrinsic memcpy"),
                "{source}\n{module}"
            );
        }
    }
}

#[test]
fn alternate_c_identity_for_library_linker_symbol_prevents_builtin_recognition() {
    for target in Target::NATIVE {
        let symbol = if target == Target::Aarch64AppleDarwin {
            "_memcpy"
        } else {
            "memcpy"
        };
        let source = format!(
            "void *custom(void *,const void *,unsigned long) __asm__(\"{symbol}\"); void *custom(void *d,const void *s,unsigned long n) {{ return d; }} void *memcpy(void *,const void *,unsigned long); void *f(void *d,const void *s) {{ return memcpy(d,s,2); }}"
        );
        let module = keel_c::compile(
            &source,
            "library-alias.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        // The existing IR verifier rejects duplicate definitions/declarations of
        // a linker symbol. Before that rejection, no builtin may bypass it.
        assert!(!module.to_string().contains("intrinsic memcpy"), "{module}");
        assert!(keel_ir::verify_module(&module).is_err());
    }
}

#[test]
fn qualified_typedef_prototypes_recognize_memory_builtins() {
    let source = "typedef unsigned long size_t; typedef void V; typedef const V CV; V *memcpy(V *restrict, CV *restrict, size_t); void *copy(V *restrict d, CV *restrict s, size_t n) { return memcpy(d,s,n); }";
    for target in Target::NATIVE {
        let module = compile(source, target);
        assert!(module.to_string().contains("intrinsic memcpy"), "{module}");
    }
}

#[test]
fn per_operation_policy_does_not_disable_other_memory_builtins() {
    let source = "void *memcpy(void *,const void *,unsigned long); void *memmove(void *,const void *,unsigned long); void *memset(void *,int,unsigned long); void f(void *d,const void *s,unsigned long n) { memcpy(d,s,n); memmove(d,s,n); memset(d,17,n); }";
    for target in Target::NATIVE {
        let policy = keel_c::LibraryBuiltins::disabled()
            .enable(keel_c::LibraryBuiltin::Memcpy)
            .enable(keel_c::LibraryBuiltin::Memmove)
            .enable(keel_c::LibraryBuiltin::Memset)
            .disable(keel_c::LibraryBuiltin::Memcpy);
        let module = keel_c::compile_with_policies(
            source,
            "policy.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
            keel_c::FunctionBinding::default(),
            policy,
        )
        .unwrap();
        let f = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(f.layout_insts().any(|(_,i)| matches!(f.inst_data(i),InstData::Call { func,.. } if f.callee(*func).name=="memcpy")), "{module}");
        for id in [Intrinsic::Memmove, Intrinsic::Memset] {
            assert!(
                f.layout_insts().any(
                    |(_, i)| matches!(f.inst_data(i),InstData::Intrinsic { id: got,.. } if *got==id)
                ),
                "{module}"
            );
        }
        keel_ir::verify_module(&module).unwrap();
    }
}

fn has_fsqrt(module: &keel_ir::Module) -> bool {
    module.to_string().contains("fsqrt")
}

fn calls(module: &keel_ir::Module, name: &str) -> bool {
    let f = module.functions.iter().find(|f| f.name == "f").unwrap();
    f.layout_insts().any(|(_, i)| {
        matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == name)
    })
}

#[test]
fn library_sqrt_is_fsqrt_with_a_libm_fallback_where_libm_sets_errno() {
    for target in Target::NATIVE {
        for (name, ty) in [
            ("sqrt", "double"),
            ("sqrtf", "float"),
            ("sqrtl", "long double"),
        ] {
            if name == "sqrtl" && target != Target::Aarch64AppleDarwin {
                continue;
            }
            let builtin = keel_c::LibraryBuiltin::ALL
                .into_iter()
                .find(|builtin| builtin.name() == name)
                .unwrap();
            let source = format!("{ty} {name}({ty}); {ty} f({ty} x) {{ return {name}(x); }}");
            let module = compile(&source, target);
            assert!(has_fsqrt(&module), "{target:?} {module}");
            // Darwin's libm never sets errno; Linux libm sets it for x < 0.
            assert_eq!(
                calls(&module, name),
                target != Target::Aarch64AppleDarwin,
                "{target:?} {module}"
            );
            for policy in [
                keel_c::LibraryBuiltins::disabled(),
                keel_c::LibraryBuiltins::hosted().disable(builtin),
            ] {
                let module = keel_c::compile_with_policies(
                    &source,
                    "sqrt.c",
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                    keel_c::FunctionBinding::default(),
                    policy,
                )
                .unwrap();
                assert!(!has_fsqrt(&module) && calls(&module, name), "{module}");
            }
        }
        // Disabling one does not disable the others.
        let source = "double sqrt(double); float sqrtf(float); float f(float x) { return sqrt(x) + sqrtf(x); }";
        let module = keel_c::compile_with_policies(
            source,
            "sqrt.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
            keel_c::FunctionBinding::default(),
            keel_c::LibraryBuiltins::hosted().disable(keel_c::LibraryBuiltin::Sqrt),
        )
        .unwrap();
        assert!(has_fsqrt(&module) && calls(&module, "sqrt"), "{module}");
        for mut module in [compile(source, target), module] {
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
            keel_opt::optimize(&mut module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn library_sqrt_recognition_requires_the_canonical_external_declaration() {
    let cases = [
        "double sqrt(double); double f(double x) { double (*p)(double) = sqrt; return p(x); }",
        "double sqrt(double); double f(double x, double (*sqrt)(double)) { return sqrt(x); }",
        "float sqrt(float); float f(float x) { return sqrt(x); }",
        "double sqrtf(double); double f(double x) { return sqrtf(x); }",
        "double sqrt(); double f(double x) { return sqrt(x); }",
        "double sqrt(double, ...); double f(double x) { return sqrt(x); }",
        "static double sqrt(double x) { return x; } double f(double x) { return sqrt(x); }",
        "double sqrt(double); double f(double x) { return sqrt(x); } double sqrt(double x) { return x; }",
        "double sqrt(double) __attribute__((weak)); double f(double x) { return sqrt(x); }",
        "double sqrt(double) __asm__(\"_other\"); double f(double x) { return sqrt(x); }",
        "double sqrt(double *); double f(double *x) { return sqrt(x); }",
    ];
    for target in Target::NATIVE {
        for source in cases {
            assert!(!has_fsqrt(&compile(source, target)), "{target:?} {source}");
        }
        if target != Target::Aarch64AppleDarwin {
            // `long double` is not `double` here, so `sqrtl` is never this operation.
            let source = "double sqrtl(double); double f(double x) { return sqrtl(x); }";
            assert!(!has_fsqrt(&compile(source, target)), "{target:?}");
        }
    }
}

/// `run.rs::programs` runs `programs/library_sqrt.c` against clang. This
/// checks that the system `<math.h>` declarations select fsqrt.
#[test]
fn library_sqrt_from_system_headers_is_fsqrt() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/library_sqrt.c");
    let preprocessed = checked(Command::new("clang").args(["-E", "-P"]).arg(&source)).stdout;
    let text = String::from_utf8(preprocessed).unwrap();
    let module = compile(&text, keel_c::Options::default().target);
    keel_ir::verify_module(&module).unwrap();
    assert!(has_fsqrt(&module));
}

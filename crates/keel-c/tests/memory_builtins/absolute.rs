use super::*;

#[test]
fn absolute_calls_lower_with_policy_and_binding_guards() {
    for target in Target::NATIVE {
        for (name, ty) in [
            ("abs", "int"),
            ("labs", "long"),
            ("llabs", "long long"),
            ("fabs", "double"),
            ("fabsf", "float"),
            ("fabsl", "long double"),
        ] {
            if name == "fabsl" && target != Target::Aarch64AppleDarwin {
                continue;
            }
            let source = format!("{ty} {name}({ty}); {ty} f({ty} x) {{ return {name}(x); }}");
            let operation = keel_c::LibraryBuiltin::ALL
                .into_iter()
                .find(|op| op.name() == name)
                .unwrap();
            let mut module = compile(&source, target);
            assert!(!calls(&module, name), "{target:?} {module}");
            keel_codegen::compile(&module, target).unwrap();
            keel_opt::optimize(&mut module).unwrap();
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
            for policy in [
                keel_c::LibraryBuiltins::disabled(),
                keel_c::LibraryBuiltins::hosted().disable(operation),
            ] {
                let module = keel_c::compile_with_policies(
                    &source,
                    "absolute.c",
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                    keel_c::FunctionBinding::default(),
                    policy,
                )
                .unwrap();
                assert!(calls(&module, name), "{target:?} {module}");
                let explicit = source.replace(
                    &format!("return {name}(x)"),
                    &format!("return __builtin_{name}(x)"),
                );
                let module = keel_c::compile_with_policies(
                    &explicit,
                    "absolute.c",
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                    keel_c::FunctionBinding::default(),
                    policy,
                )
                .unwrap();
                assert!(!calls(&module, name), "{target:?} {module}");
            }
            let defined = format!("{source} {ty} {name}({ty} x) {{ return x + 1; }}");
            assert!(calls(&compile(&defined, target), name), "custom {name}");
            let symbol = if target == Target::Aarch64AppleDarwin {
                "_custom_absolute"
            } else {
                "custom_absolute"
            };
            let renamed = format!(
                "{ty} {name}({ty}) __asm__(\"{symbol}\"); {ty} f({ty} x) {{ return {name}(x); }}"
            );
            assert!(calls(&compile(&renamed, target), "custom_absolute"));
        }
    }
}

#[test]
fn absolute_evaluates_once_and_handles_signed_and_float_boundaries() {
    let source = r#"
        int count;
        int next(void) { ++count; return -17; }
        int abs(int); long labs(long); long long llabs(long long);
        double fabs(double); float fabsf(float);
        int main(void) {
            if (abs(next()) != 17 || count != 1) return 1;
            volatile long x = -9223372036854775807L;
            if (labs(x) != 9223372036854775807L || llabs(x) != 9223372036854775807LL) return 2;
            volatile int values[5] = {0, 1, -1, 2147483647, -2147483647};
            for (int i = 0; i < 5; ++i) {
                int v = values[i];
                if (abs(v) != (v < 0 ? -v : v)) return 3;
            }
            volatile double zero = -0.0;
            if (__builtin_signbit(fabs(zero))) return 4;
            volatile double nan = __builtin_nan("");
            if (!__builtin_isnan(fabs(nan))) return 5;
            if (fabsf(-17.5f) != 17.5f) return 6;
            return 0;
        }
    "#;
    let target = keel_c::Options::default().target;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("absolute-boundaries");
    std::fs::create_dir_all(&dir).unwrap();
    let c_source = dir.join("absolute.c");
    std::fs::write(&c_source, source).unwrap();
    let oracle = dir.join("clang");
    checked(Command::new("clang").arg(&c_source).arg("-o").arg(&oracle));
    checked(&mut Command::new(oracle));
    for optimized in [false, true] {
        let mut module = compile(source, target);
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        let assembly = dir.join(format!("keel-{optimized}.s"));
        let executable = dir.join(format!("keel-{optimized}"));
        std::fs::write(&assembly, keel_codegen::compile(&module, target).unwrap()).unwrap();
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
fn absolute_explicit_builtins_report_support_and_fold_constants() {
    let source = "#if !__has_builtin(__builtin_abs) || !__has_builtin(__builtin_labs) || !__has_builtin(__builtin_llabs)\n#error missing absolute builtin\n#endif\n_Static_assert(__builtin_abs(-17) == 17, \"constant abs\"); int f(void) { return __builtin_abs(-17); }";
    for target in Target::NATIVE {
        let text = keel_c::preprocess(
            source,
            "absolute.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        compile(&text, target);
    }
}

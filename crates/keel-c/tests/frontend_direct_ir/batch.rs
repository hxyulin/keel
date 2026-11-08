use keel_c::{BatchCompiler, FunctionBinding, Input, Options};
use std::num::NonZeroUsize;

#[test]
fn batch_preserves_order_diagnostics_and_debug_ir_across_reuse() {
    let pool = BatchCompiler::new(NonZeroUsize::new(4).unwrap()).unwrap();
    for debug_info in [false, true] {
        let options = Options {
            debug_info,
            ..Default::default()
        };
        let inputs = [
            Input {
                source: "int f(int x) { return x + 1; }",
                path: "first.c",
                options,
                binding: FunctionBinding::SemanticInterposition,
            },
            Input {
                source: "int broken( {",
                path: "bad.c",
                options,
                binding: FunctionBinding::SemanticInterposition,
            },
            Input {
                source: "static int g(int x) { return x * 2; } int h(void) { return g(3); }",
                path: "last.c",
                options,
                binding: FunctionBinding::NoSemanticInterposition,
            },
        ];
        let results = pool.compile(&inputs);
        assert_eq!(results.len(), inputs.len());
        for (input, result) in inputs.iter().zip(results) {
            let serial = keel_c::compile_with_binding(
                input.source,
                input.path,
                input.options,
                input.binding,
            );
            match (serial, result) {
                (Ok(serial), Ok(parallel)) => {
                    keel_ir::verify_module(&parallel).unwrap();
                    assert_eq!(
                        format!("{serial:?}"),
                        format!("{parallel:?}"),
                        "{}",
                        input.path
                    );
                }
                (Err(serial), Err(parallel)) => {
                    assert_eq!(serial.to_string(), parallel.to_string())
                }
                _ => panic!("batch result differs for {}", input.path),
            }
        }
    }
    assert!(pool.compile(&[]).is_empty());
}

#[test]
fn raw_batch_reuses_lexing_with_private_macros_and_exact_object_bytes() {
    use keel_c::{PreprocessCache, PreprocessOptions, RawInput};
    use keel_ir::Target;
    let root = std::env::temp_dir().join(format!("keel-raw-batch-{}", std::process::id()));
    std::fs::create_dir_all(root.join("first")).unwrap();
    std::fs::create_dir_all(root.join("second")).unwrap();
    // Splices, digraphs, stringification, location builtins and include_next
    // exercise lexical spellings and provenance independently of macro state.
    let header = "#ifndef SHARED_H\n#define SHARED_H\n#define STR_(x) #x\n#define STR(x) STR_(x)\n#define CAT_(a,b) a %:%: b\n#define CAT(a,b) CAT_(a,b)\nstatic const char *file = __FILE__;\nstatic const char *word = STR(VALUE);\nstatic int CAT(hel,per)(void) { return VAL\\\nUE + __LINE__; }\n#include_next <shared.h>\n#endif\n";
    std::fs::write(root.join("first/shared.h"), header).unwrap();
    std::fs::write(root.join("second/shared.h"), "#define EXTRA 3\n").unwrap();
    let path1 = root.join("a.c").to_string_lossy().into_owned();
    let path2 = root.join("b.c").to_string_lossy().into_owned();
    let source = "#include <shared.h>\n#include <shared.h>\nint f(void) { return helper()+EXTRA+file[0]+word[0]+__COUNTER__; }\n";
    let pool = BatchCompiler::new(NonZeroUsize::new(2).unwrap()).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let mut cache = PreprocessCache::new();
        let mut a = PreprocessOptions {
            target,
            include_dirs: vec![root.join("first"), root.join("second")],
            defines: vec![("VALUE".into(), Some("11".into()))],
            ..Default::default()
        };
        let mut b = a.clone();
        b.defines = vec![("VALUE".into(), Some("29".into()))];
        for changed in [false, true] {
            if changed {
                // Same length replacement must invalidate cached lexical data.
                std::fs::write(
                    root.join("first/shared.h"),
                    header.replace("+ __LINE__", "- __LINE__"),
                )
                .unwrap();
            }
            let inputs = [(&path1, &a), (&path2, &b)];
            for (path, pp) in inputs {
                let plain = keel_c::preprocess(source, path, pp).unwrap();
                let reused = cache.preprocess(source, path, pp).unwrap();
                assert_eq!(plain, reused, "preprocessed output for {path}");
            }
            let options = Options {
                target,
                debug_info: true,
                ..Default::default()
            };
            let inputs: Vec<_> = inputs
                .iter()
                .map(|(path, pp)| RawInput {
                    source,
                    path,
                    preprocessing: pp,
                    options,
                    binding: FunctionBinding::SemanticInterposition,
                })
                .collect();
            let results = pool.compile_raw(&inputs, &mut cache);
            for (input, result) in inputs.iter().zip(results) {
                let text = keel_c::preprocess(source, input.path, input.preprocessing).unwrap();
                let mut plain = keel_c::compile(&text, input.path, options).unwrap();
                let mut reused = result.unwrap();
                keel_opt::optimize(&mut plain).unwrap();
                keel_opt::optimize(&mut reused).unwrap();
                let object = |m: &keel_ir::Module| {
                    let program = keel_codegen::compile_program(m, target).unwrap();
                    keel_asm::assemble_program_for(target, &program).unwrap()
                };
                assert_eq!(object(&plain), object(&reused), "object for {}", input.path);
            }
            assert!(cache.lexical_hits() >= 6, "header lexing was not reused");
            // Change search ordering without retaining any lookup/guard state.
            a.include_dirs.reverse();
            let plain = keel_c::preprocess("#include <shared.h>\nEXTRA\n", &path1, &a).unwrap();
            let reused = cache
                .preprocess("#include <shared.h>\nEXTRA\n", &path1, &a)
                .unwrap();
            assert_eq!(plain, reused);
            a.include_dirs.reverse();
        }
        std::fs::write(root.join("first/shared.h"), header).unwrap();
        cache.clear();
        assert_eq!(cache.lexical_hits(), 0);
    }
    std::fs::remove_dir_all(root).unwrap();
}

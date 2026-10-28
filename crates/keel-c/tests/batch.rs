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

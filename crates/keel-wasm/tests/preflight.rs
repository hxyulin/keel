use keel_ir::{Module, parse};
use keel_wasm::{
    Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_wasm32_runtime_module,
    validate_wasm32_runtime_module,
};

fn options() -> Wasm32RuntimeOptions {
    Wasm32RuntimeOptions {
        memory: Wasm32MemoryOptions {
            scalar: Options::default(),
            pages: 1,
            export_name: "memory".into(),
        },
        data_floor: 1024,
        stack_bottom: 32768,
        stack_top: 65536,
    }
}
fn rejected(module: &Module, options: &Wasm32RuntimeOptions) {
    let before = format!("{module:?}");
    let check = validate_wasm32_runtime_module(module, options).unwrap_err();
    assert_eq!(
        check,
        emit_wasm32_runtime_module(module, options).unwrap_err()
    );
    assert_eq!(before, format!("{module:?}"));
}

#[test]
fn complete_preflight_error_parity_without_export_pruning() {
    for source in [
        "decl data @external",
        "data @x = align 1 { ptr @x }",
        "data @x = align 4 { ptr @missing }",
        "data @x = align 4 { zero 65536 }",
        "decl @missing(i32)->i32",
        "func @unused(f64) {\nblock0(v0:f64):\n return\n}",
        "func @unused()->i32 {\nblock0:\n v0=iconst.i32 1\n v1=clz v0\n return v1\n}",
        "func @unused() {\n ss0=stack_slot 4, align 32\nblock0:\n return\n}",
        "func @unused() {\n ss0=stack_slot 4294967295, align 16\nblock0:\n return\n}",
        "decl @missing(i32)->i32\nfunc @unused(i32)->i32 {\nblock0(v0:i32):\n v1=call @missing(v0)\n return v1\n}",
    ] {
        rejected(&parse(source).unwrap(), &options());
    }
    let mut invalid = parse("func @f() {\nblock0:\n return\n}").unwrap();
    let b = invalid.functions[0].entry_block().unwrap();
    let ret = invalid.functions[0].block_insts(b)[0];
    invalid.functions[0].remove_inst(ret);
    rejected(&invalid, &options());
}

#[test]
fn memory_placement_and_export_error_parity() {
    let module = parse("func @f() {\nblock0:\n return\n}").unwrap();
    for case in 0..10 {
        let mut opts = options();
        match case {
            0 => opts.memory.pages = 65537,
            1 => opts.memory.export_name.clear(),
            2 => opts.data_floor = 0,
            3 => opts.stack_bottom = 32769,
            4 => opts.stack_top = 65552,
            5 => opts.memory.scalar.exports.push(Export {
                name: "missing".into(),
                function: "missing".into(),
            }),
            6 => {
                opts.memory.scalar.exports = vec![
                    Export {
                        name: "same".into(),
                        function: "f".into()
                    };
                    2
                ]
            }
            7 => opts.memory.scalar.exports.push(Export {
                name: "memory".into(),
                function: "f".into(),
            }),
            8 => opts.stack_bottom = 65552,
            9 => opts.data_floor = 32769,
            _ => unreachable!(),
        }
        rejected(&module, &opts);
    }
}

#[test]
fn valid_preflight_is_nonmutating_and_does_not_materialize_sparse_bss() {
    let module = parse("data @x = align 4 { i32 7, ptr @x + 8 }\nfunc @f()->i32 {\n ss0=stack_slot 4, align 4\nblock0:\n v0=stack_addr ss0\n v1=iconst.i32 42\n store v1,v0\n v2=load.i32 v0\n return v2\n}").unwrap();
    let before = format!("{module:?}");
    let mut opts = options();
    opts.memory.scalar.exports.push(Export {
        name: "f".into(),
        function: "f".into(),
    });
    validate_wasm32_runtime_module(&module, &opts).unwrap();
    let first = emit_wasm32_runtime_module(&module, &opts).unwrap();
    validate_wasm32_runtime_module(&module, &opts).unwrap();
    assert_eq!(first, emit_wasm32_runtime_module(&module, &opts).unwrap());
    assert_eq!(before, format!("{module:?}"));
    let sparse = parse("data @bss = align 16 { zero 4294963200 }").unwrap();
    let mut large = options();
    large.memory.pages = 65536;
    large.stack_bottom = 0xfffffff0;
    large.stack_top = 0xfffffff0;
    validate_wasm32_runtime_module(&sparse, &large).unwrap();
    validate_wasm32_runtime_module(&Module::default(), &options()).unwrap();
}

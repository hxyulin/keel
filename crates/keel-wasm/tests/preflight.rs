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
fn rejected(module: &Module, options: &Wasm32RuntimeOptions) -> keel_wasm::Diagnostic {
    let before = format!("{module:?}");
    let check = validate_wasm32_runtime_module(module, options).unwrap_err();
    assert_eq!(
        check,
        emit_wasm32_runtime_module(module, options).unwrap_err()
    );
    assert_eq!(before, format!("{module:?}"));
    check
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

#[test]
fn unused_contract_tables_and_call_descriptors_are_not_pruned() {
    use keel_ir::{AbiParam, CallArgs, InstData, Signature, Type};
    let original = parse("func @unused() {\nblock0:\n return\n}").unwrap();
    for case in 0..5 {
        let mut module = original.clone();
        let f = &mut module.functions[0];
        let expected = match case {
            0 => {
                f.import_function("missing", Signature::new(&[], &[]));
                "calls @missing, which is not declared"
            }
            1 => {
                let callee = f.import_function("unused", Signature::new(&[], &[]));
                f.callee_mut(callee).attrs.used = true;
                "callee @unused attributes disagree with module"
            }
            2 => {
                f.import_signature(Signature::new(&[], &[]));
                "indirect-call contract"
            }
            3 => {
                f.attrs.retain = true;
                "unsupported linkage/lifecycle/section attributes"
            }
            4 => {
                f.import_function("unused", Signature::new(&[Type::I32], &[]));
                "but it is declared as"
            }
            _ => unreachable!(),
        };
        if (2..=3).contains(&case) {
            keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).unwrap();
        }
        assert!(rejected(&module, &options()).0.contains(expected));
    }
    let mut module = parse("func @unused(i32)->i32 {\nblock0(v0:i32):\n v1=call @callee(v0)\n return v1\n}\nfunc @callee(i32)->i32 {\nblock0(v0:i32):\n return v0\n}").unwrap();
    let f = &mut module.functions[0];
    let call = f.block_insts(f.entry_block().unwrap())[0];
    if let InstData::Call { args, .. } = f.inst_data_mut(call) {
        *args = CallArgs::with_abi(args.iter().map(|&v| (v, AbiParam::new(Type::I32))));
    } else {
        panic!("expected direct call");
    }
    keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).unwrap();
    assert!(
        rejected(&module, &options())
            .0
            .contains("unsupported instruction")
    );
}

#[test]
fn diagnostic_precedence_retains_original_admission_order() {
    // Simultaneous failures pin the original emitter's first-error ordering:
    // options, placement, real-module verifier, declarations, exports,
    // function contracts/instructions, then static frame tables.
    let mut module = parse("data @unused_data = align 4 { i32 1 }\nfunc @unused() {\n ss0=stack_slot 4, align 32\nblock0:\n return\n}").unwrap();
    module.data[0].thread_local = true;
    // This passes placement but fails verification on the real module.
    module.data[0].local = true;
    module.data[0].hidden = true;
    module.decls.push(keel_ir::FuncDecl {
        attrs: Default::default(),
        name: "missing".into(),
        sig: keel_ir::Signature::new(&[], &[]),
    });
    module.functions[0].attrs.used = true;
    let mut opts = options();
    opts.memory.pages = 65537;
    opts.memory.scalar.exports.push(Export {
        name: "missing".into(),
        function: "missing".into(),
    });
    assert_eq!(
        rejected(&module, &opts).0,
        "wasm32 memory pages must be in 0..=65536"
    );
    opts.memory.pages = 1;
    assert!(
        rejected(&module, &opts)
            .0
            .contains("unsupported data TLS/linkage/section metadata")
    );
    module.data[0].thread_local = false;
    assert!(
        rejected(&module, &opts)
            .0
            .contains("hidden requires a nonlocal definition")
    );
    module.data[0].local = false;
    assert!(
        rejected(&module, &opts)
            .0
            .contains("undefined function @missing")
    );
    module.decls.clear();
    assert!(
        rejected(&module, &opts)
            .0
            .contains("has no defined function")
    );
    opts.memory.scalar.exports.clear();
    assert!(
        rejected(&module, &opts)
            .0
            .contains("unsupported linkage/lifecycle/section attributes")
    );
    module.functions[0].attrs.used = false;
    assert_eq!(
        rejected(&module, &opts).0,
        "software stack slot alignment must be a power of two <=16"
    );
}

#[test]
fn actual_wasm32_layout_keeps_older_entrypoint_contracts() {
    // Adjacent four-byte pointers pass only the explicit WASM32 verifier.
    let module = parse("data @table = align 4 { ptr @table, ptr @table + 8 }").unwrap();
    assert!(keel_ir::verify_module(&module).is_err());
    let before = format!("{module:?}");
    let opts = options();
    validate_wasm32_runtime_module(&module, &opts).unwrap();
    let bytes = emit_wasm32_runtime_module(&module, &opts).unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    assert!(keel_wasm::emit_module(&module, &opts.memory.scalar).is_err());
    assert!(keel_wasm::emit_wasm32_memory_module(&module, &opts.memory).is_err());
    assert_eq!(before, format!("{module:?}"));
}

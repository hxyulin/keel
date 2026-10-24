use super::{InstructionOutput, ValType, W};
#[path = "adjacent_tee.rs"]
mod oracle;

#[test]
fn every_intervening_operator_flushes_and_finish_keeps_trailing_set() {
    // These fragments exercise buffering itself, including control boundaries
    // and effect/trap operators. Complete typed functions are covered by the
    // integration tests; here even partial control fragments must stay byte-identical.
    let barriers = [
        W::Nop,
        W::LocalGet(1),
        W::LocalSet(1),
        W::LocalTee(0),
        W::I32Const(1),
        W::Call(0),
        W::I32DivS,
        W::I32Load(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }),
        W::I32Store(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }),
        W::Block(wasm_encoder::BlockType::Empty),
        W::Loop(wasm_encoder::BlockType::Empty),
        W::If(wasm_encoder::BlockType::Empty),
        W::Else,
        W::End,
        W::Br(0),
        W::BrIf(0),
        W::Return,
        W::Unreachable,
    ];
    for barrier in barriers {
        let mut original = wasm_encoder::Function::new([(2, ValType::I32)]);
        let mut output = InstructionOutput::new(vec![(2, ValType::I32)]);
        for op in [W::LocalSet(0), barrier, W::LocalGet(0), W::End] {
            original.instruction(&op);
            output.op(op);
        }
        let raw = original.into_raw_body();
        assert_eq!(output.finish().into_raw_body(), raw);
    }
    let mut output = InstructionOutput::new(vec![]);
    output.op(W::LocalSet(128));
    assert_eq!(output.finish().into_raw_body(), [0, 0x21, 0x80, 1]);
}

#[test]
fn matching_raw_indices_only_including_multibyte_and_repeated_pairs() {
    for local in [0, 1, 127, 128, 16384] {
        let mut original = wasm_encoder::Function::new([]);
        let mut output = InstructionOutput::new(vec![]);
        for op in [
            W::LocalSet(local),
            W::LocalGet(local),
            W::LocalSet(local),
            W::LocalGet(local),
            W::End,
        ] {
            original.instruction(&op);
            output.op(op);
        }
        let raw = original.into_raw_body();
        let (expected, pairs) = oracle::fold_body(&raw);
        assert_eq!(pairs, 2);
        assert_eq!(output.finish().into_raw_body(), expected);
    }
}

#[test]
fn typed_nonempty_prefix_survives_i32_and_i64_tees() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use wasm_encoder::{CodeSection, ExportKind, ExportSection, FunctionSection, Module, TypeSection};

    let mut output = InstructionOutput::new(vec![(1, ValType::I32), (1, ValType::I64)]);
    for op in [
        W::I64Const(100), // Live prefix below both kinds of folded pair.
        W::I32Const(7),
        W::LocalSet(0),
        W::LocalGet(0),
        W::I64ExtendI32U,
        W::I64Add,
        W::I64Const(9),
        W::LocalSet(1),
        W::LocalGet(1),
        W::I64Add,
        W::LocalGet(0),
        W::I64ExtendI32U,
        W::I64Add,
        W::LocalGet(1),
        W::I64Add,
        W::End,
    ] {
        output.op(op);
    }
    let function = output.finish();
    // Literal expectation independent of both the buffer and decoded oracle.
    assert_eq!(
        function.clone().into_raw_body(),
        [
            2, 1, 0x7f, 1, 0x7e, 0x42, 0xe4, 0, 0x41, 7, 0x22, 0, 0xad, 0x7c, 0x42, 9,
            0x22, 1, 0x7c, 0x20, 0, 0xad, 0x7c, 0x20, 1, 0x7c, 0x0b,
        ]
    );
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I64]);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut exports = ExportSection::new();
    exports.export("value", ExportKind::Func, 0);
    let mut code = CodeSection::new();
    code.function(&function);
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let bytes = module.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    let mut child = Command::new("node")
        .args(["-e", r#"
const assert = require('assert/strict');
const bytes = require('fs').readFileSync(0);
assert(WebAssembly.validate(bytes));
const e = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
for (let i = 0; i < 20; i++) assert.equal(e.value(), 132n);
"#])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node required");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn consecutive_sets_flush_first_and_only_fold_last_immediate_read() {
    let mut output = InstructionOutput::new(vec![(2, ValType::I32)]);
    for op in [
        W::LocalSet(0),
        W::LocalSet(0),
        W::LocalGet(0),
        W::LocalGet(0),
        W::LocalSet(1),
        W::LocalSet(0),
        W::LocalGet(0),
        W::LocalTee(1),
        W::LocalSet(1),
    ] {
        output.op(op);
    }
    assert_eq!(
        output.finish().into_raw_body(),
        [1, 2, 0x7f, 0x21, 0, 0x22, 0, 0x20, 0, 0x21, 1, 0x22, 0, 0x22, 1, 0x21, 1]
    );
}

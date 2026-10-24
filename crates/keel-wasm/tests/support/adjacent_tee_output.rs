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

//! SSE2 low widening and bitwise vector moves; native execution is a separate gate.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
};

const PAIRS: [(u32, u32); 6] = [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)];

fn shape(bits: u32) -> String {
    format!("i{bits}x{}", 128 / bits)
}

fn compile(
    source: &str,
    selection: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let direct = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(direct, keel_asm::x86::assemble_object(&text).unwrap());
    (text, direct)
}

#[test]
fn all_integer_widening_pairs_stay_packed_with_baseline_sse2() {
    let mut source = String::new();
    for (from, to) in PAIRS {
        for op in ["swiden_low", "uwiden_low"] {
            source += &format!(
                "func @{op}_{from}_{to}({a}) -> {b} {{\nblock0(v0: {a}):\n v1 = {op}.{b} v0\n return v1\n}}\n",
                a = shape(from),
                b = shape(to)
            );
        }
    }
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (text, _) = compile(
                &source,
                selection,
                &allocation,
                &keel_codegen::x86_64::REGISTERS,
            );
            for opcode in [
                "punpcklbw",
                "punpcklwd",
                "punpckldq",
                "pcmpgtb",
                "pcmpgtw",
                "pcmpgtd",
            ] {
                assert!(text.contains(opcode), "missing {opcode}");
            }
            assert!(
                !text.contains("ptr ["),
                "register widening unexpectedly accessed memory: {text}"
            );
            assert!(
                !text.contains("pextr") && !text.contains("pmovsx") && !text.contains("pmovzx")
            );
        }
    }
}

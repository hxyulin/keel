//! Independent literal opcode witnesses: no executing target code.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use object::{Object, ObjectSection, ObjectSymbol};
// Full deterministic Baseline/Full/EMPTY body, independent of encoder output.
// Its fixed input sequence ends at an instruction boundary, so an extra 66 or
// REX.W cannot hide immediately before an unprefixed r32 query. The exact tail
// also rejects additional queries, normalization or memory stores.
const INPUT: &[u8] = &[
    0x55, // push rbp
    0x48, 0x89, 0xe5, // mov rbp, rsp
    0x48, 0x83, 0xec, 0x10, // sub rsp, 16
    0x48, 0x89, 0x7d, 0xf0, // mov [rbp-16], rdi
    0x48, 0x8b, 0x45, 0xf0, // mov rax, [rbp-16]
];
fn exact_body(query: &[u8], register: bool) -> Vec<u8> {
    let mut expected = INPUT.to_vec();
    expected.extend_from_slice(query);
    if register {
        // Result home and ABI return reload, both flag-neutral 64-bit MOVs.
        expected.extend_from_slice(&[0x48, 0x89, 0x45, 0xf0, 0x48, 0x8b, 0x45, 0xf0]);
    }
    expected.extend_from_slice(&[0x48, 0x89, 0xec, 0x5d, 0xc3]); // restore frame; ret
    expected
}
#[test]
fn exact_operand_sizes_and_native_memory_writes() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let mut rejected_prefix_mutants = 0;
    for id in keel_ir::Intrinsic::ALL
        .into_iter()
        .filter(|i| i.x86_descriptor_query().is_some())
    {
        let (op, width, bytes) = id.x86_descriptor_query().unwrap();
        let call = if bytes == 0 {
            format!(" v1 = intrinsic {}(v0)\n return v1", id.info().name)
        } else {
            format!(" intrinsic {}(v0)\n return", id.info().name)
        };
        let ty = if bytes == 0 { "i64" } else { "ptr" };
        let ret = if bytes == 0 { " -> i64" } else { "" };
        let source = format!("func @query({ty}){ret} {{\nentry(v0:{ty}):\n{call}\n}}\n");
        let m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Full,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&m, &empty, &pipeline).unwrap();
        let obj = keel_asm::x86::assemble_program_object(&program).unwrap();
        let file = object::File::parse(&*obj).unwrap();
        let sym = file
            .symbols()
            .find(|s| s.name().is_ok_and(|n| n == "query"))
            .unwrap();
        let section = file.section_by_index(sym.section_index().unwrap()).unwrap();
        let data = section.data().unwrap();
        let start = (sym.address() - section.address()) as usize;
        let body = &data[start..start + sym.size() as usize];
        // EMPTY forces the input to reserved RAX. Memory is [RAX]; no
        // synthesized padding store or r16-to-r64 normalization is involved.
        let mut expected = vec![];
        if bytes == 0 {
            if width == 16 {
                expected.push(0x66);
            } else if width == 64 {
                expected.push(0x48);
            }
        }
        expected.extend_from_slice(&match (op, bytes == 0) {
            ("sgdt", false) => [0x0f, 0x01, 0x00],
            ("sidt", false) => [0x0f, 0x01, 0x08],
            ("sldt", false) => [0x0f, 0x00, 0x00],
            ("str", false) => [0x0f, 0x00, 0x08],
            ("smsw", false) => [0x0f, 0x01, 0x20],
            ("sldt", true) => [0x0f, 0x00, 0xc0],
            ("str", true) => [0x0f, 0x00, 0xc8],
            ("smsw", true) => [0x0f, 0x01, 0xe0],
            _ => unreachable!(),
        });
        let complete = exact_body(&expected, bytes == 0);
        assert_eq!(body, complete, "{} complete body", id.info().name);
        if bytes == 0 && width == 32 {
            // DQ-R2-01: all six wrong-width substitutions previously passed the
            // unanchored window count. Mutate the real body at the now-proven
            // query boundary; apply the same full-body equality as above.
            for prefix in [0x66, 0x48] {
                let mut mutant = body.to_vec();
                mutant.insert(INPUT.len(), prefix);
                assert_eq!(
                    mutant
                        .windows(expected.len())
                        .filter(|b| *b == expected)
                        .count(),
                    1,
                    "{} prefix {prefix:02x}: reproduce old false acceptance",
                    id.info().name,
                );
                assert_ne!(
                    mutant,
                    complete,
                    "{} prefix {prefix:02x}: wrong operand size accepted",
                    id.info().name,
                );
                rejected_prefix_mutants += 1;
            }
        }
    }
    assert_eq!(rejected_prefix_mutants, 6);
}

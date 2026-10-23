//! Bounded production witness; full DC native numerical/status proof is pending.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use object::{Object, ObjectSection};
const OPS: [&str; 6] = ["fadd", "fmul", "fsub", "fsubr", "fdiv", "fdivr"];
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;

#[test]
fn independent_dc_encoding_and_all_live_output_combinations() {
    let mut source = String::new();
    for op in OPS {
        for mode in 0..4 {
            source += &format!(
                "func @{op}_st1_m{mode}(ptr,ptr,ptr)->i32 {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.f80 align 1 v0\n v4 = load.f80 align 1 v1\n v5,v6 = intrinsic x86_x87_{op}_st1(v3,v4)\n"
            );
            if mode & 1 != 0 {
                source += " store align 1 v5,v2\n";
            }
            source += if mode & 2 != 0 {
                " return v6\n}\n"
            } else {
                " v7 = iconst.i32 0\n return v7\n}\n"
            };
        }
    }
    for optimized in [false, true] {
        let mut m = keel_ir::parse(&source).unwrap();
        if optimized {
            keel_opt::optimize(&mut m).unwrap();
        }
        keel_ir::verify_module(&m).unwrap();
        assert!(keel_codegen::aarch64::compile(&m).is_err());
        for function in &m.functions {
            let mut count = 0;
            for (_, i) in function.layout_insts() {
                if let keel_ir::InstData::Intrinsic { id, .. } = function.inst_data(i) {
                    count += 1;
                    assert!(!id.info().pure);
                    assert!(id.is_x86_target());
                    assert_eq!(function.inst_results(i).len(), 2);
                    assert_eq!(
                        function.inst_data(i).memory_effects(),
                        keel_ir::MemoryEffects::Barrier
                    );
                }
            }
            assert_eq!(count, 1);
        }
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    let program =
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                            .unwrap();
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                    let features = Features::for_level(CpuLevel::V1);
                    assert_eq!(
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap(),
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    let lines: Vec<_> = text.lines().map(str::trim).collect();
                    for op in OPS {
                        let mut count = 0;
                        for (i, line) in lines.iter().enumerate() {
                            if line.split_whitespace().next() != Some(op) {
                                continue;
                            }
                            count += 1;
                            assert_eq!(
                                line.split_whitespace().skip(1).collect::<String>(),
                                "st(1),st(0)"
                            );
                            assert!(lines[i + 1].starts_with("fnstsw"));
                            assert_eq!(
                                lines[i + 2].split_whitespace().collect::<Vec<_>>(),
                                ["fstp", "st(0)"]
                            );
                            assert!(lines[i + 3].starts_with("fstp"));
                        }
                        assert_eq!(count, 4);
                    }
                }
            }
        }
    }
    // Native assembler syntax provides a separate byte-level reference for
    // all six DC principal opcodes and the required status/two-pop sequence.
    let assembly = ".intel_syntax noprefix\n.text\nfadd st(1),st(0)\nfmul st(1),st(0)\nfsub st(1),st(0)\nfsubr st(1),st(0)\nfdiv st(1),st(0)\nfdivr st(1),st(0)\nfnstsw ax\nfstp st(0)\nfstp st(0)\n";
    let expected = [
        0xdc, 0xc1, 0xdc, 0xc9, 0xdc, 0xe9, 0xdc, 0xe1, 0xdc, 0xf9, 0xdc, 0xf1, 0xdf, 0xe0, 0xdd,
        0xd8, 0xdd, 0xd8,
    ];
    let d = common::temp("x87-dc-production");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("dc.S"), assembly).unwrap();
    common::command(
        std::process::Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(d.join("dc.S"))
            .arg("-o")
            .arg(d.join("dc.o")),
    );
    let bytes = |data: &[u8]| {
        object::File::parse(data)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(bytes(&std::fs::read(d.join("dc.o")).unwrap()), expected);
    assert_eq!(
        bytes(&keel_asm::x86::assemble_object(assembly).unwrap()),
        expected
    );
}

#[allow(dead_code)]
#[path = "common/native_oracle.rs"]
mod native_oracle;

use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::{CodegenPipeline, SelectionTier};

const SOURCE: &str = include_str!("../../../suite/ir/llvm_import_fast_fixtures/fast.keel");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn private_fast_calls_cover_stack_args_recursion_and_spills() {
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for arm in [true, false] {
                let (text, program) = if arm {
                    (
                        keel_codegen::aarch64::compile_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                        keel_codegen::aarch64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                } else {
                    (
                        keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap(),
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                };
                let (parsed, typed) = if arm {
                    (
                        keel_asm::assemble_object(&text).unwrap(),
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                    )
                } else {
                    (
                        keel_asm::x86::assemble_object(&text).unwrap(),
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                    )
                };
                assert_eq!(parsed, typed);
            }
        }
    }
}

const LEAF: &str = "func @leaf(i64) -> i64 cc(fast) local {
block0(v0: i64):
 v1 = iconst.i64 42
 v2 = iadd v0, v1
 return v2
}";

#[test]
fn fast_leaf_omits_frame_and_retains_unwind_metadata() {
    use object::{Object, ObjectSection};
    let m = keel_ir::parse(LEAF).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let arm = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!arm.contains("stp\tx29, x30"), "{arm}");
            if selector == SelectionTier::Full {
                assert!(arm.contains("add\tx0, x0, #42"), "{arm}");
                assert!(!arm.contains("mov\tx9, x0"), "{arm}");
            }
            let program = keel_codegen::aarch64::compile_program_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let bytes = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(bytes, keel_asm::assemble_object(&arm).unwrap());
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let unwind = file
                .section_by_name("__compact_unwind")
                .unwrap()
                .data()
                .unwrap();
            assert_eq!(
                u32::from_le_bytes(unwind[12..16].try_into().unwrap()),
                0x0200_0000
            );
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!x86.contains("push\trbp"), "{x86}");
            let program = keel_codegen::x86_64::compile_program_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert_eq!(bytes, keel_asm::x86::assemble_object(&x86).unwrap());
            let file = object::File::parse(bytes.as_slice()).unwrap();
            assert!(file.section_by_name(".eh_frame").unwrap().size() > 0);
        }
    }
}

#[test]
fn frame_observers_calls_debug_spills_and_stack_arguments_retain_frames() {
    let mut modules = vec![
        keel_ir::parse(&LEAF.replace("cc(fast) local", "cc(c) local")).unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " v8 = intrinsic frame_address()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " v8 = intrinsic stack_save()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
        keel_ir::parse(&format!(
            "decl @outside()\n{}",
            LEAF.replace(
                " v1 = iconst.i64 42",
                " call @outside()\n v1 = iconst.i64 42"
            )
        ))
        .unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " asm \"nop\", \"\"()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
    ];
    let mut debug = keel_ir::parse(LEAF).unwrap();
    debug.files.push("private-leaf.c".into());
    modules.push(debug);
    let params = (0..10)
        .map(|i| format!("v{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    modules.push(
        keel_ir::parse(&format!(
            "func @stack({}) -> i64 cc(fast) local {{\nblock0({params}):\n return v9\n}}",
            ["i64"; 10].join(", ")
        ))
        .unwrap(),
    );
    for m in modules {
        keel_ir::verify_module(&m).unwrap();
        let arm = keel_codegen::compile(&m, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
        assert!(arm.contains("stp\tx29, x30"), "{arm}");
        // The x86 backend rejects stack intrinsics and inline asm independently.
        if let Ok(x86) = keel_codegen::compile(&m, keel_codegen::Target::X86_64LinuxGnu) {
            assert!(x86.contains("push\trbp"), "{x86}");
        }
    }
    let m = keel_ir::parse(LEAF).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let arm = keel_codegen::aarch64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    assert!(arm.contains("stp\tx29, x30"));
    let x86 = keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    assert!(x86.contains("push\trbp"));
}

#[test]
fn tls_getters_keep_lr_frame_and_argument_bank_reserved() {
    let m = keel_ir::parse("data @tls = thread_local align 8 { i64 5 }\nfunc @leaf(i64) -> i64 cc(fast) local {\nblock0(v0: i64):\n v1 = symbol_addr @tls\n v2 = load.i64 v1\n v3 = iadd v0, v2\n return v3\n}").unwrap();
    keel_ir::verify_module(&m).unwrap();
    let arm = keel_codegen::compile(&m, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    assert!(arm.contains("stp\tx29, x30"), "{arm}");
    assert!(arm.contains("mov\tx9, x0"), "{arm}");
    let x86 = keel_codegen::compile(&m, keel_codegen::Target::X86_64LinuxGnu).unwrap();
    assert!(x86.contains("push\trbp"), "{x86}");
}

/// `@helper` is called directly and through two escaped addresses.
const ADDRESSED: &str = "func @helper(i8 sext, i64, f64) -> i64 noinline local {
block0(v0: i8, v1: i64, v2: f64):
 v3 = sext.i64 v0
 v4 = fptosi.i64 v2
 v5 = iadd v3, v1
 v6 = iadd v5, v4
 return v6
}
func @direct(i64, f64) -> i64 {
block0(v0: i64, v1: f64):
 v2 = trunc.i8 v0
 v3 = call @helper(v2, v0, v1)
 return v3
}
func @address() -> ptr {
block0:
 v0 = symbol_addr @helper
 return v0
}
data @table = align 8 { ptr @helper }";

const ADDRESSED_ORACLE: &str = r#"
#include <stdint.h>
typedef int64_t (*F)(signed char, int64_t, double);
int64_t direct(int64_t, double);
F address(void);
extern F table;
int main(void) {
    F f = address();
    if (f != table) return 1;
    for (int64_t i = -300; i <= 300; i += 7) {
        signed char c = (signed char)(i * 37);
        double d = (double)i / 3;
        if (f(c, i, d) != (int64_t)c + i + (int64_t)d) return 2;
        if (direct(i, d) != (int64_t)(signed char)i + i + (int64_t)d) return 3;
    }
    return 0;
}
"#;

/// The instructions between `label` and `end`, without directives or comments.
fn thunk_instructions<'t>(text: &'t str, label: &str, end: &str) -> Vec<&'t str> {
    let body = text.split(label).nth(1).unwrap();
    body[..body.find(end).unwrap()]
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['.', ';', '#']) && !l.ends_with(':'))
        .collect()
}

#[test]
fn address_taken_fast_helpers_run_through_their_c_thunk() {
    let mut m = keel_ir::parse(ADDRESSED).unwrap();
    keel_opt::run_passes(&mut m, &["internal-fast-calls", "tail-calls"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(m.functions[0].sig.cc, keel_ir::CallConv::Fast);
    assert!(m.functions.iter().any(|f| f.name == "helper.c"));
    let oracle = native_oracle::PreparedOracle::new("fast-thunk", ADDRESSED_ORACLE, &["-O2"]);
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let registers = &keel_codegen::aarch64::REGISTERS;
            let text =
                keel_codegen::aarch64::compile_with_pipeline(&m, registers, &pipeline).unwrap();
            // The thunk is a single jump to the fast body.
            assert_eq!(
                thunk_instructions(&text, "_helper.c:", "Lhelper.c_end:"),
                ["b\t_helper"]
            );
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&m, registers, &pipeline)
                    .unwrap();
            let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(keel_asm::assemble_object(&text).unwrap(), object);
            oracle.run(&object, &format!("{selector:?} {allocator:?}"));
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert_eq!(
                thunk_instructions(&x86, "\"helper.c\":", ".Lhelper.c_end:"),
                ["jmp\t\"helper\""]
            );
            keel_asm::x86::assemble_object(&x86).unwrap();
        }
    }
}

#[test]
fn forwarders_jump_only_when_arguments_are_already_placed() {
    let source = |caller: &str, callee: &str| {
        format!(
            "func @g({callee}) -> i64 noinline local {{\nblock0(v0: i8):\n v1 = sext.i64 v0\n return v1\n}}\nfunc @f({caller}) -> i64 {{\nblock0(v0: i8):\n return_call @g(v0)\n}}"
        )
    };
    for (caller, callee, jumps) in [
        ("i8 sext", "i8 sext", true),
        ("i8 sext", "i8", true),
        // `@g` relies on an extension that `@f`'s callers did not apply.
        ("i8", "i8 sext", false),
        ("i8 uext", "i8 sext", false),
    ] {
        let m = keel_ir::parse(&source(caller, callee)).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let arm = keel_codegen::aarch64::compile(&m).unwrap();
        let x86 = keel_codegen::x86_64::compile(&m).unwrap();
        let arm = thunk_instructions(&arm, "_f:", "Lf_end:");
        let x86 = thunk_instructions(&x86, "\"f\":", ".Lf_end:");
        assert_eq!(arm == ["b\t_g"], jumps, "{caller} -> {callee}: {arm:?}");
        assert_eq!(
            x86 == ["jmp\t\"g\""],
            jumps,
            "{caller} -> {callee}: {x86:?}"
        );
    }
}

#[test]
fn checked_peek_matches_the_nine_instruction_leaf_shape() {
    let mut m = keel_ir::parse(include_str!("programs/checked_peek.keel")).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(
        &m,
        &keel_codegen::aarch64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let hot = text
        .split("Lchecked_peek_block0:")
        .nth(1)
        .unwrap()
        .split("\tret\n")
        .next()
        .unwrap();
    assert!(hot.contains("adds"), "{hot}");
    assert!(
        !hot.contains("cset")
            && !hot.contains("eor")
            && !hot.contains("stp")
            && !hot.contains("sub\tsp"),
        "{hot}"
    );
    let instructions: Vec<_> = hot
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';') && !line.ends_with(':'))
        .collect();
    assert_eq!(
        instructions.len(),
        8,
        "nine instructions including RET: {instructions:?}"
    );
    let program = keel_codegen::aarch64::compile_program_with_pipeline(
        &m,
        &keel_codegen::aarch64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let parsed = keel_asm::assemble_object(&text).unwrap();
    let typed = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(parsed, typed);
    let oracle = native_oracle::PreparedOracle::new(
        "checked-peek",
        include_str!("programs/checked_peek.c"),
        &["-O2"],
    );
    oracle.run(&typed, "checked peek hot and panic paths");
}

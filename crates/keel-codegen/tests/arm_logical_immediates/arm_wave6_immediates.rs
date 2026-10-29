//! ARM-specific immediate eligibility and preallocation/materialization checks.
//! Native execution and structured/text parity are separate integration gates.
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn asm(ty: &str, body: &str, result: &str) -> String {
    // Comparisons return an unsigned boolean through the narrow C ABI.
    let result_attr = if matches!(result, "i8" | "i16") {
        " uext"
    } else {
        ""
    };
    let src = format!("func @f({ty}) -> {result}{result_attr} {{\nblock0(v0: {ty}):\n{body}\n}}");
    let module = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    compile_with_options(&module, Target::Aarch64AppleDarwin, SelectionOptions::ALL).unwrap()
}

#[test]
fn arithmetic_signed_and_shifted_immediates_use_execution_width() {
    for ty in ["i32", "i64"] {
        for (k, immediate) in [
            (-4095i64, "#4095"),
            (-4096, "#1, lsl #12"),
            (4096, "#1, lsl #12"),
            (16773120, "#4095, lsl #12"),
        ] {
            for op in ["iadd", "isub"] {
                let mnemonic = if (op == "isub") ^ (k < 0) {
                    "sub"
                } else {
                    "add"
                };
                let output = asm(
                    ty,
                    &format!("v1 = iconst.{ty} {k}\nv2 = {op} v0, v1\nreturn v2"),
                    ty,
                );
                assert!(output.contains(&format!("\t{mnemonic}\t")), "{output}");
                assert!(output.contains(immediate), "{output}");
                assert!(
                    !output.contains("\tmovn\t") && !output.contains("\tmovz\t"),
                    "{output}"
                );
            }
        }
    }
    // The i32 bit pattern is +1, despite its signed i64 spelling.
    let output = asm(
        "i32",
        "v1 = iconst.i32 -4294967295\nv2 = iadd v1, v0\nreturn v2",
        "i32",
    );
    assert!(
        output.contains("\tadd\t") && output.contains("#1"),
        "{output}"
    );
}

#[test]
fn negative_compare_only_uses_z_predicates() {
    for ty in ["i32", "i64"] {
        for cc in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            let output = asm(
                ty,
                &format!("v1 = iconst.{ty} -4096\nv2 = icmp.{cc} v0, v1\nreturn v2"),
                "i8",
            );
            assert_eq!(
                output.contains("\tcmn\t"),
                matches!(cc, "eq" | "ne"),
                "{output}"
            );
        }
        for k in [0i64, 4095, 4096, 16773120] {
            let output = asm(
                ty,
                &format!("v1 = iconst.{ty} {k}\nv2 = icmp.ult v0, v1\nreturn v2"),
                "i8",
            );
            assert!(output.contains("\tcmp\t"), "{output}");
            assert!(!output.contains("\tmovz\t"), "{output}");
        }
    }
}

#[test]
fn shared_and_duplicate_constants_still_materialize_when_needed() {
    for body in [
        "v1 = iconst.i64 -1\nv2 = iadd v0, v1\nv3 = xor v2, v1\nreturn v3",
        "v1 = iconst.i64 -1\nv2 = iadd v1, v1\nreturn v2",
    ] {
        let output = asm("i64", body, "i64");
        assert!(output.contains("\tmovn\t"), "{output}");
        assert!(
            output.contains("\tadd\t") || output.contains("\tsub\t"),
            "{output}"
        );
    }
}

#[test]
fn unencodable_boundaries_keep_register_operands() {
    for (ty, k) in [
        ("i32", -2147483648i64),
        ("i64", i64::MIN),
        ("i64", 4097),
        ("i64", 16777216),
        ("i64", -16777216),
    ] {
        let output = asm(
            ty,
            &format!("v1 = iconst.{ty} {k}\nv2 = iadd v0, v1\nreturn v2"),
            ty,
        );
        let add = output
            .lines()
            .find(|line| line.contains("\tadd\t"))
            .unwrap();
        assert!(!add.contains('#'), "{output}");
    }
    for ty in ["i8", "i16"] {
        let output = asm(
            ty,
            &format!("v1 = iconst.{ty} -1\nv2 = icmp.eq v0, v1\nreturn v2"),
            "i8",
        );
        assert!(!output.contains("\tcmn\t"), "{output}");
    }
}

fn native_fixture() -> (String, String) {
    let mut ir = String::new();
    let mut declarations = String::from("#include <stdint.h>\n");
    let mut checks = String::new();
    let mut index = 0;
    for bits in [32, 64] {
        for k in [
            0i64,
            -1,
            4095,
            -4095,
            4096,
            -4096,
            4097,
            16773120,
            -16773120,
            -2147483648,
            i64::MIN,
            0x100000001,
        ] {
            for op in ["iadd", "isub", "eq", "ne", "ult", "slt"] {
                let name = format!("f{index}");
                index += 1;
                let compare = !matches!(op, "iadd" | "isub");
                let instruction = if compare {
                    format!("icmp.{op}")
                } else {
                    op.to_owned()
                };
                let result = if compare || bits == 32 {
                    "v3 = uext.i64 v2\nreturn v3"
                } else {
                    "return v2"
                };
                ir += &format!(
                    "func @{name}(i{bits}) -> i64 {{\nblock0(v0: i{bits}):\n v1 = iconst.i{bits} {k}\n v2 = {instruction} v0, v1\n {result}\n}}\n"
                );
                declarations += &format!("extern uint64_t {name}(uint{bits}_t);\n");
                let constant = format!("((uint{bits}_t)UINT64_C({}))", k as u64);
                let input = format!("((uint{bits}_t)x)");
                let expected = match op {
                    "iadd" => format!("(uint{bits}_t)({input}+{constant})"),
                    "isub" => format!("(uint{bits}_t)({input}-{constant})"),
                    "eq" => format!("({input}=={constant})"),
                    "ne" => format!("({input}!={constant})"),
                    "ult" => format!("({input}<{constant})"),
                    "slt" => format!("((int{bits}_t){input}<(int{bits}_t){constant})"),
                    _ => unreachable!(),
                };
                checks += &format!("if ({name}({input})!=(uint64_t)({expected})) return 1;\n");
            }
        }
    }
    let harness = declarations
        + "int main(void) {\n uint64_t random=0x123456789abcdef0ULL;\n const uint64_t edges[]={0,1,UINT64_MAX,UINT64_C(2147483648),UINT64_C(9223372036854775808),4095,4096};\n for(int i=0;i<1000;i++) {\n random^=random<<13;random^=random>>7;random^=random<<17;\n uint64_t x=i<7?edges[i]:random;\n"
        + &checks
        + "}\n return 0;\n}\n";
    (ir, harness)
}

fn parity(
    src: &str,
    allocation: keel_codegen::regalloc::AllocationTier,
    pools: &keel_codegen::regalloc::Pools,
) -> Vec<u8> {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &keel_codegen::SelectionTier::Full,
        allocator: &allocation,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let object =
        keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_object(&text).unwrap_or_else(|e| panic!("{e}\n{text}")),
        "{text}"
    );
    object
}

const EMPTY: keel_codegen::regalloc::Pools = keel_codegen::regalloc::Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn immediate_boundaries_have_native_text_parity_with_spills() {
    let (ir, _) = native_fixture();
    for allocation in [
        keel_codegen::regalloc::AllocationTier::Fast,
        keel_codegen::regalloc::AllocationTier::Full,
    ] {
        for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
            parity(&ir, allocation, pools);
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_immediates_match_wrapping_arithmetic_and_predicates() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../suite/reports/optimization-wave6-20261016/native-tests/immediates-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let (ir, harness) = native_fixture();
    fs::write(dir.join("test.c"), harness).unwrap();
    for allocation in [
        keel_codegen::regalloc::AllocationTier::Fast,
        keel_codegen::regalloc::AllocationTier::Full,
    ] {
        for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
            fs::write(dir.join("test.o"), parity(&ir, allocation, pools)).unwrap();
            assert!(
                Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("test.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(Command::new(dir.join("test")).status().unwrap().success());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

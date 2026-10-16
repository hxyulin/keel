//! Exact-width ARM move-wide planning, including the structured/text boundary.
//! Runtime checks deliberately require a native Apple ARM host.
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
};
use keel_ir::{EntityRef, InstData};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

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
    let text = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let object = assemble_program_object(&program, TargetFeatures::default())
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(
        object,
        assemble_object(&text).unwrap_or_else(|e| panic!("{e}\n{text}")),
        "typed/text object mismatch: {text}"
    );
    (text, object)
}

fn values() -> Vec<u64> {
    let mut values = vec![
        0,
        u64::MAX,
        0xffff_ffff,
        0x8000_0000,
        0x8000_0000_0000_0000,
        0x1234_5678_9abc_def0,
        0xffff_ffff_0000_0000,
        0x0000_0000_ffff_ffff,
        0x7ff8_0000_0000_0123,
    ];
    for lane in 0..4 {
        values.push(0xabcd << (16 * lane));
        values.push(!(0xabcd << (16 * lane)));
    }
    let mut random = 0xfeed_face_cafe_beef_u64;
    for _ in 0..32 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        values.push(random);
    }
    values
}

fn constant(bits: u64) -> String {
    format!(
        "func @constant() -> i64 {{\nblock0:\n v0 = iconst.i64 {}\n return v0\n}}\n",
        bits as i64
    )
}

fn moves(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            ["movz\t", "movn\t", "movk\t"]
                .iter()
                .any(|op| line.starts_with(op))
        })
        .collect()
}

#[test]
fn emitted_constants_have_minimal_move_wide_counts_and_exact_x_width() {
    for bits in values() {
        let (text, _) = compile(
            &constant(bits),
            SelectionTier::Baseline,
            &AllocationTier::Full,
            &keel_codegen::aarch64::REGISTERS,
        );
        let bytes = bits.to_le_bytes();
        let halves = bytes.chunks_exact(2);
        let zeros = halves.clone().filter(|h| *h == [0, 0]).count();
        let ones = halves.filter(|h| *h == [255, 255]).count();
        let instructions = moves(&text);
        assert_eq!(instructions.len(), (4 - zeros.max(ones)).max(1), "{text}");
        assert!(
            instructions.iter().all(|line| line.contains("\tx")),
            "{text}"
        );
        if bits == u64::MAX {
            assert!(instructions[0].starts_with("movn\tx"), "{text}");
            assert!(instructions[0].ends_with("#0x0"), "{text}");
        }
    }
}

fn fixture() -> (String, String) {
    let mut source = String::from("file f0 = \"arm_constants.keel\"\n");
    let mut declarations = String::from("#include <stdint.h>\n");
    let mut checks = String::new();
    for (index, bits) in values().into_iter().enumerate() {
        source += &constant(bits).replace("@constant", &format!("@c{index}"));
        declarations += &format!("extern uint64_t c{index}(void);\n");
        checks += &format!("if (c{index}() != UINT64_C({bits})) return 1;\n");
    }
    for (ty, c_ty) in [
        ("i8", "int8_t"),
        ("i16", "int16_t"),
        ("i32", "int32_t"),
        ("i64", "int64_t"),
    ] {
        let abi_extension = if matches!(ty, "i8" | "i16") {
            " sext"
        } else {
            ""
        };
        source += &format!(
            "func @signed_{ty}() -> {ty}{abi_extension} {{\nblock0:\n v0 = iconst.{ty} -1 !f0:1:1\n return v0\n}}\n"
        );
        declarations += &format!("extern {c_ty} signed_{ty}(void);\n");
        checks += &format!("if (signed_{ty}() != -1) return 2;\n");
    }
    for (index, bits) in [0xffff_abcd_ffff_ffff_u64, 0x8000_0000_1234_5678]
        .into_iter()
        .enumerate()
    {
        source += &format!(
            "func @pointer{index}() -> i64 {{\nblock0:\n v0 = iconst.i64 {}\n v1 = inttoptr.ptr v0\n v2 = ptrtoint.i64 v1\n return v2\n}}\n",
            bits as i64
        );
        declarations += &format!("extern uint64_t pointer{index}(void);\n");
        checks += &format!("if (pointer{index}() != UINT64_C({bits})) return 3;\n");
    }
    for (name, float_ty, int_ty, bits) in [
        ("nan64", "f64", "i64", 0x7ff8_0000_0000_0123_u64),
        ("signaling64", "f64", "i64", 0x7ff0_0000_0000_0001),
        ("negative_zero", "f64", "i64", 0x8000_0000_0000_0000),
        ("nan32", "f32", "i32", 0xffc0_0042),
        ("ones32", "f32", "i32", 0xffff_ffff),
    ] {
        source += &format!(
            "func @{name}() -> {int_ty} {{\nblock0:\n v0 = fconst.{float_ty} {bits:#x}\n v1 = bitcast.{int_ty} v0\n return v1\n}}\n"
        );
        let c_ty = if int_ty == "i64" {
            "uint64_t"
        } else {
            "uint32_t"
        };
        declarations += &format!("extern {c_ty} {name}(void);\n");
        checks += &format!("if ({name}() != UINT64_C({bits})) return 4;\n");
    }
    // A real loop edge swaps two parameters and transfers a constant. Empty
    // pools force stack cycles; ordinary pools exercise register copies.
    source += "func @cycle(i64, i64, i64) -> i64 {\n\
block0(v0: i64, v1: i64, v2: i64):\n\
 v3 = iconst.i64 0\n v4 = iconst.i64 1\n v5 = iconst.i64 281474976710656\n\
 jump block1(v0, v1, v3, v5)\n\
block1(v6: i64, v7: i64, v8: i64, v9: i64):\n\
 v10 = icmp.ult v8, v2\n brif v10, block2, block3\n\
block2:\n v11 = iadd v8, v4\n jump block1(v7, v6, v11, v5)\n\
block3:\n v12 = isub v6, v7\n v13 = iadd v12, v9\n return v13\n}\n";
    declarations += "extern uint64_t cycle(uint64_t, uint64_t, uint64_t);\n";
    checks += "for (uint64_t i = 0; i < 6; ++i)\n\
 if (cycle(7, 11, i) != UINT64_C(281474976710656) + (i % 2 ? 4 : (uint64_t)-4)) return 5;\n";
    (
        source,
        declarations + "int main(void) {\n" + &checks + "return 0;\n}\n",
    )
}

/// Tests exact rematerialization and cycles without changing production
/// eligibility: only existing legal, nonnegative integer recipes are assigned.
struct RecipeAllocator;
impl RegisterAllocator for RecipeAllocator {
    fn name(&self) -> &'static str {
        "test-arm-constant-recipes"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        for (_, inst) in input.func.layout_insts() {
            if let InstData::Iconst { imm, .. } = input.func.inst_data(inst) {
                if *imm >= 0 {
                    let value = input.func.inst_results(inst)[0];
                    if alloc.loc(value).is_some() {
                        alloc.locs[0][value.index()] =
                            Some(keel_codegen::regalloc::Loc::Remat(value));
                    }
                }
            }
        }
        alloc
    }
}

#[test]
fn constants_transfers_spills_debug_and_native_text_encodings_agree() {
    let (source, _) = fixture();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [
            &AllocationTier::Fast as &dyn RegisterAllocator,
            &AllocationTier::Full,
            &RecipeAllocator,
        ] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILLED] {
                let (text, _) = compile(&source, selection, allocator, pools);
                assert!(text.contains(".cfi_startproc"), "{text}");
                assert!(text.contains(".loc"), "{text}");
                assert!(
                    text.contains("movn\tw"),
                    "f32 all-ones must use W MOVN: {text}"
                );
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_constants_abi_payloads_spills_cycles_and_nzcv_execute() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-arm-constants-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let (source, harness) = fixture();
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    let run = |command: &mut Command| {
        let output = command.output().unwrap();
        assert!(output.status.success(), "{command:?}\n{output:?}");
    };
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [
            &AllocationTier::Fast as &dyn RegisterAllocator,
            &AllocationTier::Full,
            &RecipeAllocator,
        ] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILLED] {
                let (_, object) = compile(&source, selection, allocator, pools);
                let obj = dir.join("test.o");
                fs::write(&obj, object).unwrap();
                let exe = dir.join("test");
                run(Command::new("clang")
                    .args(["-O2", "-ffp-contract=off"])
                    .arg(&c)
                    .arg(&obj)
                    .arg("-o")
                    .arg(&exe));
                run(&mut Command::new(exe));
            }
        }
    }
    // Put the actual emitted move-wide sequences between CMP and MRS to
    // check NZCV directly, including all seed lanes and both requested widths.
    let mut assembly = String::from(".text\n");
    let mut upper_checks = Vec::new();
    for (index, bits) in values().into_iter().enumerate() {
        let (text, _) = compile(
            &constant(bits),
            SelectionTier::Baseline,
            &AllocationTier::Full,
            &keel_codegen::aarch64::REGISTERS,
        );
        for class in ['x', 'w'] {
            // W comes from a real f32 materialization, not a narrowed X plan.
            let w_source = format!(
                "func @constant() -> f32 {{\nblock0:\n v0 = fconst.f32 {:#x}\n return v0\n}}\n",
                bits as u32
            );
            let w_text;
            let text = if class == 'w' {
                w_text = compile(
                    &w_source,
                    SelectionTier::Baseline,
                    &AllocationTier::Full,
                    &keel_codegen::aarch64::REGISTERS,
                )
                .0;
                &w_text
            } else {
                &text
            };
            assembly +=
                &format!(".globl _flags_{class}{index}\n_flags_{class}{index}:\n cmp x0, x1\n");
            let mut sequence = String::new();
            for instruction in moves(text) {
                // Use scratch x16/w16 so the arguments stay intact.
                let (op, operands) = instruction.split_once('\t').unwrap();
                let (_, immediate) = operands.split_once(", ").unwrap();
                sequence += &format!(" {op} {class}16, {immediate}\n");
            }
            assembly += &sequence;
            assembly += " mrs x0, nzcv\n ret\n";
            if class == 'w' && !sequence.is_empty() {
                assembly += &format!(".globl _upper{index}\n_upper{index}:\n movn x16, #0\n");
                assembly += &sequence;
                assembly += " mov x0, x16\n ret\n";
                upper_checks.push((index, bits as u32));
            }
        }
    }
    let mut flags_c = String::from(
        "#include <stdint.h>\n\
uint64_t expected(uint64_t a, uint64_t b) {\n\
 uint64_t r = a - b;\n\
 return ((r >> 63) << 31) | ((uint64_t)(r == 0) << 30) |\n\
        ((uint64_t)(a >= b) << 29) | ((((a ^ b) & (a ^ r)) >> 63) << 28);\n}\n",
    );
    let mut checks = String::from(
        "uint64_t pairs[][2] = {{(uint64_t)-7, 3}, {3, (uint64_t)-7}, {3, 3},\n\
 {UINT64_C(0x8000000000000000), 1}, {UINT64_C(0x7fffffffffffffff), UINT64_MAX}, {0, 1}, {0, 0}};\n",
    );
    for index in 0..values().len() {
        for class in ['x', 'w'] {
            flags_c += &format!("extern uint64_t flags_{class}{index}(uint64_t, uint64_t);\n");
            checks += &format!(
                "for (unsigned p = 0; p < sizeof(pairs) / sizeof(pairs[0]); ++p)\n if (flags_{class}{index}(pairs[p][0], pairs[p][1]) != expected(pairs[p][0], pairs[p][1])) return 1;\n"
            );
        }
    }
    for (index, expected) in upper_checks {
        flags_c += &format!("extern uint64_t upper{index}(void);\n");
        checks += &format!("if (upper{index}() != UINT64_C({expected})) return 2;\n");
    }
    flags_c += &(String::from("int main(void) {\n") + &checks + "return 0;\n}\n");
    let flags_obj = dir.join("flags.o");
    fs::write(&flags_obj, assemble_object(&assembly).unwrap()).unwrap();
    fs::write(&c, flags_c).unwrap();
    let exe = dir.join("flags");
    run(Command::new("clang")
        .arg(&c)
        .arg(&flags_obj)
        .arg("-o")
        .arg(&exe));
    run(&mut Command::new(exe));
    fs::remove_dir_all(dir).unwrap();
}

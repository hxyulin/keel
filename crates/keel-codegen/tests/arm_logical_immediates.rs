//! Exercise the emitted instruction and allocation contract without an IR
//! optimizer, so canonicalization cannot hide either operand position or uses.
use keel_codegen::{
    CodegenPipeline, OptionsSelector, SelectionOptions,
    regalloc::{AllocationTier, Pools},
};

const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[],
};

fn options() -> impl Iterator<Item = SelectionOptions> {
    (0..8).map(|n| SelectionOptions {
        trees: n & 1 != 0,
        folded_operands: n & 2 != 0,
        register_hints: n & 4 != 0,
    })
}

fn source(bits: u32, op: &str, mask: i64, lhs: bool, tail: &str) -> String {
    let args = if lhs { "v1, v0" } else { "v0, v1" };
    let ret = if bits == 64 {
        "return v2".to_string()
    } else {
        "v3 = uext.i64 v2\n return v3".to_string()
    };
    format!(
        "func @f(i{bits}) -> i64 {{\nblock0(v0: i{bits}):\n v1 = iconst.i{bits} {mask}\n v2 = {op} {args}\n {}\n}}",
        if tail.is_empty() { &ret } else { tail }
    )
}

fn compile(
    module: &keel_ir::Module,
    option: SelectionOptions,
    allocation: AllocationTier,
) -> String {
    keel_ir::verify_module(module).unwrap();
    let selector = OptionsSelector(option);
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    // Both paths run the production allocation validator, also with spills.
    let text = keel_codegen::aarch64::compile_with_pipeline(module, &TINY, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(module, &TINY, &pipeline).unwrap();
    let native =
        keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap();
    let parsed = keel_asm::assemble_object(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(
        native, parsed,
        "native fields differ from independent text parser\n{text}"
    );
    text
}

fn logical<'a>(text: &'a str, mnemonic: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|l| l.trim_start().starts_with(&format!("{mnemonic}\t")))
        .collect()
}

#[test]
fn known_masks_use_correct_execution_width_in_all_selection_combinations() {
    // Repeated runs, rotations crossing bit zero, long runs, signed i32
    // patterns, and i32 constants with noncanonical high bits.
    for (bits, mask, normalized) in [
        (32, 0x3f, 0x3fu64),
        (32, 0x00ff00ff, 0x00ff00ff),
        (32, -2147483647, 0x80000001),
        (32, -256, 0xffffff00),
        (32, 0x10000003f, 0x3f),
        (64, 0x00ff00ff00ff00ff, 0x00ff00ff00ff00ff),
        (64, 0x8000000000000001u64 as i64, 0x8000000000000001),
        (64, -256, 0xffffffffffffff00),
        (64, 0x5555555555555555, 0x5555555555555555),
    ] {
        for (op, mnemonic) in [("and", "and"), ("or", "orr"), ("xor", "eor")] {
            for lhs in [false, true] {
                let module = keel_ir::parse(&source(bits, op, mask, lhs, "")).unwrap();
                for option in options() {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        let text = compile(&module, option, allocation);
                        let lines = logical(&text, mnemonic);
                        assert_eq!(lines.len(), 1, "{text}");
                        assert!(
                            lines[0].trim_start().starts_with(&format!(
                                "{mnemonic}\t{}",
                                if bits == 32 { "w" } else { "x" }
                            )),
                            "{text}"
                        );
                        assert!(lines[0].ends_with(&format!("#0x{normalized:x}")), "{text}");
                        assert!(
                            !text.contains("\tmovz\t") && !text.contains("\tmovk\t"),
                            "fold-only constant was materialized\n{text}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn unencodable_and_narrow_values_keep_register_forms() {
    for bits in [8, 16, 32, 64] {
        let mut masks = vec![0, -1, 0x12345678, 0x3f];
        if bits == 32 {
            masks.extend([0xffffffff, 0x100000000, 0x1ffffffff]);
        }
        for mask in masks {
            if bits >= 32 && mask == 0x3f {
                continue;
            }
            // Keep narrow constants inside the IR parser's signed range.
            let mask = if bits == 8 {
                mask as i8 as i64
            } else if bits == 16 {
                mask as i16 as i64
            } else {
                mask
            };
            for (op, mnemonic) in [("and", "and"), ("or", "orr"), ("xor", "eor")] {
                let module = keel_ir::parse(&source(bits, op, mask, false, "")).unwrap();
                for option in options() {
                    let text = compile(&module, option, AllocationTier::Full);
                    let lines = logical(&text, mnemonic);
                    assert_eq!(lines.len(), 1, "{text}");
                    assert!(!lines[0].contains('#'), "{text}");
                }
            }
        }
    }
}

#[test]
fn mixed_consumers_duplicate_occurrences_and_aliases_keep_materialization() {
    for bits in [32, 64] {
        for (op, mnemonic) in [("and", "and"), ("or", "orr"), ("xor", "eor")] {
            for mode in ["mixed", "duplicate", "alias"] {
                let tail = if bits == 32 {
                    "v5 = uext.i64 v4\n return v5"
                } else {
                    "return v4"
                };
                let mut src = source(bits, op, 63, false, &format!("v4 = imul v2, v1\n {tail}"));
                if mode == "duplicate" {
                    src = source(bits, op, 63, false, "")
                        .replace(&format!("{op} v0, v1"), &format!("{op} v1, v1"));
                }
                let mut module = keel_ir::parse(&src).unwrap();
                if mode == "alias" {
                    // Replace an extra constant with an alias to the original;
                    // the binary operand itself must resolve before folding.
                    src = src.replace(
                        &format!("v2 = {op} v0, v1"),
                        &format!("v6 = iconst.i{bits} 63\n v2 = {op} v0, v6"),
                    );
                    module = keel_ir::parse(&src).unwrap();
                    let func = &mut module.functions[0];
                    let constants: Vec<_> = func
                        .layout_insts()
                        .filter_map(|(_, i)| {
                            matches!(func.inst_data(i), keel_ir::InstData::Iconst { .. })
                                .then(|| func.inst_results(i)[0])
                        })
                        .collect();
                    func.replace_all_uses(constants[1], constants[0]);
                }
                for option in options() {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        let text = compile(&module, option, allocation);
                        assert!(logical(&text, mnemonic)[0].contains("#0x3f"), "{text}");
                        assert!(
                            text.contains("\tmovz\t"),
                            "unfolded constant occurrence lost\n{text}"
                        );
                        if mode != "duplicate" {
                            assert!(text.contains("\tmul\t"), "{text}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn folded_operand_occurrences_reach_allocator_before_allocation() {
    use keel_codegen::regalloc::{Allocation, AllocationInput, RegisterAllocator};
    struct Probe(bool);
    impl RegisterAllocator for Probe {
        fn name(&self) -> &'static str {
            "logical-fold-probe"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            for (_, inst) in input.func.layout_insts() {
                if matches!(
                    input.func.inst_data(inst),
                    keel_ir::InstData::Binary {
                        op: keel_ir::BinaryOp::And,
                        ..
                    }
                ) {
                    assert_eq!(
                        input.selection.operands(input.func, inst).len(),
                        if self.0 { 1 } else { 2 }
                    );
                }
            }
            AllocationTier::Full.allocate(input)
        }
    }
    for lhs in [false, true] {
        for duplicate in [false, true] {
            let mut src = source(64, "and", 63, lhs, "");
            if duplicate {
                src = src
                    .replace("and v0, v1", "and v1, v1")
                    .replace("and v1, v0", "and v1, v1");
            }
            let module = keel_ir::parse(&src).unwrap();
            for option in options() {
                let selector = OptionsSelector(option);
                keel_codegen::aarch64::compile_with_pipeline(
                    &module,
                    &TINY,
                    &CodegenPipeline {
                        selector: &selector,
                        allocator: &Probe(option.folded_operands),
                    },
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn i128_logical_operands_are_not_folded() {
    let module = keel_ir::parse("func @f(i128) -> i128 {\nblock0(v0: i128):\n v1 = iconst.i128 63\n v2 = and v0, v1\n return v2\n}").unwrap();
    for option in options() {
        let text = compile(&module, option, AllocationTier::Full);
        let lines = logical(&text, "and");
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines.iter().all(|line| !line.contains('#')), "{text}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_wrapping_dirty_inputs_and_spilled_multiconsumers() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("arm-logical-immediates");
    fs::create_dir_all(&dir).unwrap();
    for option in options() {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let mut ir = String::new();
            let mut driver = String::from("#include <stdint.h>\n");
            let mut checks = String::new();
            for bits in [32, 64] {
                for (mask_id, mask) in [-256, 0x8000000000000001u64 as i64, 0x00ff00ff00ff00ff]
                    .into_iter()
                    .enumerate()
                {
                    for (op, c_op) in [("and", "&"), ("or", "|"), ("xor", "^")] {
                        let name = format!("f_{op}_{bits}_{mask_id}");
                        let ty = format!("i{bits}");
                        ir += &format!(
                            "func @{name}({ty}) -> i64 {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} {mask}\n"
                        );
                        // All 12 results are live through the logical operation,
                        // forcing spills with two allocatable GPRs.
                        for i in 10..22 {
                            ir += &format!(" v{i} = iadd v0, v0\n");
                        }
                        ir += &format!(" v2 = {op} v0, v1\n v3 = imul v0, v1\n v30 = xor v2, v3\n");
                        let mut last = 30;
                        for i in 10..22 {
                            let next = 31 + i - 10;
                            ir += &format!(" v{next} = iadd v{last}, v{i}\n");
                            last = next;
                        }
                        if bits == 32 {
                            ir += &format!(" v50 = uext.i64 v{last}\n return v50\n}}\n");
                        } else {
                            ir += &format!(" return v{last}\n}}\n");
                        }
                        driver += &format!("extern uint64_t {name}(uint64_t);\n");
                        let c_ty = format!("uint{bits}_t");
                        checks += &format!(
                            "{{ {c_ty} x=({c_ty})inputs[i], k=({c_ty})0x{:x}ULL; {c_ty} want=({c_ty})((x {c_op} k) ^ ({c_ty})(x*k)); want=({c_ty})(want+({c_ty})(24*x)); if ({name}(inputs[i]) != (uint64_t)want) return 1; }}\n",
                            mask as u64
                        );
                    }
                }
            }
            let module = keel_ir::parse(&ir).unwrap();
            let text = compile(&module, option, allocation);
            assert!(text.contains("[sp"), "spill fixture did not spill\n{text}");
            driver += &format!(
                "int main(void) {{ uint64_t inputs[]={{0,1,255,256,0xffffffffULL,0x80000000ULL,0xdeadbeef00000001ULL,0xffffffffffffffffULL,0x8000000000000000ULL}}; for(unsigned i=0;i<sizeof(inputs)/sizeof(*inputs);++i) {{ {checks} }} return 0; }}\n"
            );
            let asm = dir.join("f.s");
            let c = dir.join("driver.c");
            let exe = dir.join("run");
            fs::write(&asm, text).unwrap();
            fs::write(&c, driver).unwrap();
            let out = Command::new("clang")
                .args(["-O2"])
                .arg(&asm)
                .arg(&c)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                Command::new(&exe).status().unwrap().success(),
                "{option:?} {allocation:?}"
            );
        }
    }
}

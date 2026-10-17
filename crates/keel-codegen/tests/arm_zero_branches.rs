//! Ordinary integer truth tests, deliberately separate from compare fusion.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source(bits: u32, reverse: bool, edges: bool) -> String {
    let (yes, no) = if reverse {
        ("block2", "block1")
    } else {
        ("block1", "block2")
    };
    let (setup, yes_args, no_args, param1, param2, ret1, ret2) = if edges {
        (
            "v1 = iconst.i64 37\n v2 = iconst.i64 91\n",
            "(v1)",
            "(v2)",
            "(v3: i64)",
            "(v4: i64)",
            "return v3",
            "return v4",
        )
    } else {
        (
            "", "", "", "", "",
            "v3 = iconst.i64 37\n return v3",
            "v4 = iconst.i64 91\n return v4",
        )
    };
    format!(
        "func @f(i{bits}) -> i64 {{\nblock0(v0: i{bits}):\n {setup} brif v0, {yes}{yes_args}, {no}{no_args}\nblock1{param1}:\n {ret1}\nblock2{param2}:\n {ret2}\n}}\n"
    )
}

fn compile_module(
    module: &keel_ir::Module,
    tier: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &tier,
        allocator: &allocation,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let program = keel_codegen::aarch64::compile_program_with_pipeline(module, pools, &pipeline)
        .unwrap();
    if !module.functions.iter().any(|func| {
        func.layout_insts().any(|(_, i)| {
            matches!(func.inst_data(i), keel_ir::InstData::InlineAsm { .. })
        })
    }) {
        assert_eq!(program.stats().text_bytes, 0, "no opaque text fallback");
    }
    let object = keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap(), "{text}");
    (text, object)
}

fn compile(
    src: &str,
    tier: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    compile_module(&keel_ir::parse(src).unwrap(), tier, allocation, pools)
}

#[test]
fn full_width_truth_edges_and_spills_use_register_branches_in_all_tiers() {
    for bits in [32, 64] {
        for reverse in [false, true] {
            for edges in [false, true] {
                for tier in [SelectionTier::Baseline, SelectionTier::Trees, SelectionTier::Full] {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                            let (text, _) = compile(
                                &source(bits, reverse, edges), tier, allocation, pools,
                            );
                            let width = if bits == 32 { 'w' } else { 'x' };
                            assert!(text.contains(&format!("\tcbz\t{width}")), "{text}");
                            assert!(!text.contains("\tcmp\t"), "{text}");
                            assert!(!text.contains("\tb.eq\t"), "{text}");
                            if edges {
                                assert!(text.contains("_else:"), "false-edge landing pad: {text}");
                            } else {
                                let target = if reverse { "block1" } else { "block2" };
                                assert!(
                                    text.lines().any(|l| {
                                        l.contains("\tcbz\t") && l.ends_with(target)
                                    }),
                                    "{text}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn narrow_and_wide_truth_tests_keep_mask_and_pair_semantics() {
    for (bits, instruction) in [(8, "\ttst\tw"), (16, "\ttst\tw"), (128, "\torr\tx")] {
        let (text, _) = compile(
            &source(bits, false, false),
            SelectionTier::Full,
            AllocationTier::Full,
            &SPILLED,
        );
        assert!(text.contains(instruction), "{text}");
        assert!(!text.contains("\tcbz\t"), "{text}");
        assert!(text.contains("\tb.eq\t"), "{text}");
    }
    // The IR has no i1 type, and floating truth is invalid rather than
    // bit-testing -0.0. Floating comparisons remain on their existing path.
    assert!(keel_ir::parse(&source(1, false, false)).is_err());
    let module = keel_ir::parse(&source(32, false, false).replace("i32", "f32")).unwrap();
    assert!(keel_ir::verify_module(&module).is_err());
}

#[test]
fn comparisons_and_materialized_booleans_keep_existing_flags_path() {
    for (ty, op) in [("i32", "icmp.eq"), ("i64", "icmp.slt"), ("f64", "fcmp.ne")] {
        let src = format!(
            "func @f({ty}, {ty}) -> i64 {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = {op} v0, v1\n brif v2, block1, block2\nblock1:\n v3 = iconst.i64 37\n return v3\nblock2:\n v4 = iconst.i64 91\n return v4\n}}"
        );
        let (text, _) = compile(&src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
        assert!(!text.contains("\tcbz\t"), "{text}");
        assert!(
            text.contains(if ty == "f64" { "\tfcmp\t" } else { "\tcmp\t" }),
            "{text}"
        );
    }
    // An extra use prevents compare fusion. Its i8 result is intentionally
    // left on the masked truth-test path even though CSET defines 0/1.
    let src = "func @f(i32, i32) -> i8 {\nblock0(v0: i32, v1: i32):\n v2 = icmp.eq v0, v1\n brif v2, block1, block2\nblock1:\n return v2\nblock2:\n return v2\n}";
    let (text, _) = compile(src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert!(text.contains("\tcset\t"), "{text}");
    assert!(text.contains("\ttst\t"), "{text}");
    assert!(!text.contains("\tcbz\t"), "{text}");
}

#[test]
fn successor_inline_assembly_keeps_old_flags_even_without_cc_clobber() {
    for clobber in ["", ",~{cc}"] {
        let src = source(32, false, false).replace(
            "v3 = iconst.i64 37",
            &format!("v5 = asm \"cset %w0, eq\", \"=r{clobber}\"() -> i32\n v3 = uext.i64 v5"),
        );
        let (text, _) = compile(&src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
        assert!(text.contains("\tcmp\tw"), "{text}");
        assert!(text.contains("\tb.eq\t"), "{text}");
        assert!(!text.contains("\tcbz\t"), "{text}");
    }
}

#[test]
fn aliases_locations_and_call_result_use_the_original_truth_width() {
    let src = "file f0 = \"zero.keel\"\nextern @poll() -> i32\nfunc @f() -> i64 {\nblock0:\n v0 = call @poll()\n v1 = iconst.i32 1\n brif v1, block1, block2 !f0:8:3\nblock1:\n v2 = iconst.i64 37\n return v2\nblock2:\n v3 = iconst.i64 91\n return v3\n}";
    let mut module = keel_ir::parse(src).unwrap();
    let func = &mut module.functions[0];
    let results: Vec<_> = func.layout_insts().filter_map(|(_, i)| {
        matches!(
            func.inst_data(i),
            keel_ir::InstData::Call { .. }
                | keel_ir::InstData::Iconst { ty: keel_ir::Type::I32, .. }
        ).then(|| func.inst_results(i)[0])
    }).collect();
    func.replace_all_uses(results[1], results[0]);
    let (text, _) = compile_module(&module, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert!(text.contains("\tbl\t_poll"), "{text}");
    assert!(text.contains("\tcbz\tw"), "{text}");
    assert!(text.contains(".loc\t1 8 3"), "{text}");
    assert!(text.contains(".cfi_startproc"), "{text}");
    assert!(text.find("\tbl\t_poll").unwrap() < text.find("\tcbz\tw").unwrap());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_truth_values_match_independent_clang_oracle() {
    use std::{fs, process::Command};
    let mut src = String::new();
    for bits in [8, 16, 32, 64] {
        for reverse in [false, true] {
            for edges in [false, true] {
                src += &source(bits, reverse, edges).replace(
                    "@f(", &format!("@f{bits}_{}_{}(", reverse as u8, edges as u8),
                );
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("keel-arm-zero-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("oracle.c"), include_str!("fixtures/arm_zero_branches.c")).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
            let (_, object) = compile(&src, SelectionTier::Full, allocation, pools);
            fs::write(dir.join("test.o"), object).unwrap();
            assert!(
                Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .status().unwrap().success()
            );
            assert!(Command::new(dir.join("test")).status().unwrap().success());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

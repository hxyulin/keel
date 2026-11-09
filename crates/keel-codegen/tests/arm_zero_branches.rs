//! Ordinary integer truth tests, deliberately separate from compare fusion.
#[allow(dead_code)]
#[path = "common/codegen.rs"]
mod codegen;
#[allow(dead_code)]
#[path = "common/native_oracle.rs"]
mod native_oracle;

use codegen::{Case, EMPTY as SPILLED};
use keel_codegen::{
    SelectionTier,
    regalloc::{AllocationTier, Pools},
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
            "",
            "",
            "",
            "",
            "",
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
    let mut case = Case::aarch64(module)
        .selector(&tier)
        .allocator(&allocation)
        .pools(pools);
    if !module.functions.iter().any(|func| {
        func.layout_insts()
            .any(|(_, i)| matches!(func.inst_data(i), keel_ir::InstData::InlineAsm { .. }))
    }) {
        case = case.no_text_fallback();
    }
    let out = case.compile();
    (out.text, out.object)
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
                for (tier, allocation, pools) in codegen::matrix(
                    &[
                        SelectionTier::Baseline,
                        SelectionTier::Trees,
                        SelectionTier::Full,
                    ],
                    &[&SPILLED, &keel_codegen::aarch64::REGISTERS],
                ) {
                    let (text, _) = compile(&source(bits, reverse, edges), tier, allocation, pools);
                    let width = if bits == 32 { 'w' } else { 'x' };
                    let landing = text.contains("_else:");
                    assert!(
                        text.contains(&format!(
                            "\t{}\t{width}",
                            if landing { "cbz" } else { "cbnz" }
                        )),
                        "{text}"
                    );
                    assert!(!text.contains("\tcmp\t"), "{text}");
                    assert!(!text.contains("\tb.eq\t"), "{text}");
                    if landing {
                        assert!(edges, "landing pad without edge arguments: {text}");
                    } else {
                        let target = if reverse { "block2" } else { "block1" };
                        assert!(
                            text.lines()
                                .any(|l| { l.contains("\tcbnz\t") && l.ends_with(target) }),
                            "{text}"
                        );
                        assert!(
                            !text.contains("\tb\t"),
                            "redundant fallthrough jump: {text}"
                        );
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
        assert!(text.contains("\tb.ne\t"), "{text}");
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
    let src = "func @f(i32, i32) -> i8 uext {\nblock0(v0: i32, v1: i32):\n v2 = icmp.eq v0, v1\n brif v2, block1, block2\nblock1:\n return v2\nblock2:\n return v2\n}";
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
        assert!(text.contains("\tb.ne\t"), "{text}");
        assert!(!text.contains("\tcbz\t"), "{text}");
    }
}

#[test]
fn inline_numeric_branches_keep_compound_and_always_semantics() {
    let source = "func @compound() -> i64 {\nblock0:\n v0 = asm \"mov %0, #9\\ncmp xzr, xzr\\n; keel-inline-boundary\\nb.ne 1f; mov %0, #7\\n1:\", \"={x0}\"() -> i64\n return v0\n}\nfunc @always() -> i64 {\nblock0:\n v0 = asm \"mov %0, #9\\nb.al 1f\\nmov %0, #7\\n1:\", \"={x0}\"() -> i64\n return v0\n}\n";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let target = keel_codegen::Target::Aarch64AppleDarwin;
    let text = keel_codegen::compile(&module, target).unwrap();
    let program = keel_codegen::compile_program(&module, target).unwrap();
    for branch in ["b.ne 1f; mov x0, #7", "b.al 1f"] {
        assert!(text.contains(branch), "{text}");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, keel_asm_ir::Item::Text(body) if body.contains(branch))),
            "structured output lost {branch}"
        );
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        // Keel's Mach-O assembler does not accept numeric inline labels yet.
        // Clang assembles this opaque template. Darwin treats ';' as a comment.
        let dir = keel_test_support::TempDir::new("inline-numeric-branches");
        std::fs::write(dir.join("test.s"), text).unwrap();
        let output = std::process::Command::new("clang")
            .arg("-c")
            .arg(dir.join("test.s"))
            .arg("-o")
            .arg(dir.join("test.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle = native_oracle::PreparedOracle::new(
            "inline-numeric-branches",
            "extern unsigned long compound(void), always(void); int main(void) { return compound() != 9 || always() != 9; }",
            &["-O2"],
        );
        oracle.run(
            &std::fs::read(dir.join("test.o")).unwrap(),
            "inline numeric branch semantics",
        );
    }
}

#[test]
fn aliases_locations_and_call_result_use_the_original_truth_width() {
    let src = "file f0 = \"zero.keel\"\ndecl @poll() -> i32\nfunc @f() -> i64 {\nblock0:\n v0 = call @poll()\n v1 = iconst.i32 1\n brif v1, block1, block2 !f0:8:3\nblock1:\n v2 = iconst.i64 37\n return v2\nblock2:\n v3 = iconst.i64 91\n return v3\n}";
    let mut module = keel_ir::parse(src).unwrap();
    let func = &mut module.functions[0];
    let results: Vec<_> = func
        .layout_insts()
        .filter(|&(_, i)| {
            matches!(
                func.inst_data(i),
                keel_ir::InstData::Call { .. }
                    | keel_ir::InstData::Iconst {
                        ty: keel_ir::Type::I32,
                        ..
                    }
            )
        })
        .map(|(_, i)| func.inst_results(i)[0])
        .collect();
    func.replace_all_uses(results[1], results[0]);
    let (text, _) = compile_module(&module, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert!(text.contains("\tbl\t_poll"), "{text}");
    assert!(text.contains("\tcbnz\tw"), "{text}");
    assert!(text.contains(".loc\t1 8 3"), "{text}");
    assert!(text.contains(".cfi_startproc"), "{text}");
    assert!(text.find("\tbl\t_poll").unwrap() < text.find("\tcbnz\tw").unwrap());
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
                    "@f(",
                    &format!("@f{bits}_{}_{}(", reverse as u8, edges as u8),
                );
            }
        }
    }
    let dir = keel_test_support::TempDir::new("arm-zero");
    fs::write(
        dir.join("oracle.c"),
        include_str!("fixtures/arm_zero_branches.c"),
    )
    .unwrap();
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
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(Command::new(dir.join("test")).status().unwrap().success());
        }
    }
}

#[test]
fn moves_only_on_the_false_edge_need_no_landing_pad() {
    // The then-block comes next, since it also reaches the join, and its
    // edge needs no moves.
    let src = "func @f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = iconst.i64 91\n brif v0, block1, block2(v2)\nblock1:\n v5 = iadd v1, v1\n jump block2(v5)\nblock2(v3: i64):\n v4 = iadd v3, v1\n return v4\n}";
    for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
        // Trees retains the false-edge copies. Full may coalesce both
        // edges, in which case the branch directly skips the then-block.
        for tier in [SelectionTier::Trees, SelectionTier::Full] {
            let (text, _) = compile(src, tier, AllocationTier::Full, pools);
            assert!(!text.contains("_else:"), "{text}");
            let Some(branch) = text.find("\tcbnz\t") else {
                assert!(text.contains("\tcbz\t"), "{text}");
                assert!(!text.contains("\tb\tLf_block2\n"), "{text}");
                continue;
            };
            assert!(
                text[branch..]
                    .lines()
                    .next()
                    .unwrap()
                    .ends_with("Lf_block1"),
                "{text}"
            );
            assert!(text.contains("\tb\tLf_block2\n"), "{text}");
        }
    }
}

#[test]
fn overflow_branches_preserve_value_polarity_edges_and_spill_transport() {
    let mut source = String::new();
    let mut oracle = String::from("#include <assert.h>\n#include <stdint.h>\n");
    let mut checks = String::new();
    for bits in [32, 64] {
        for (op, builtin, signed) in [
            ("uadd", "add", false),
            ("usub", "sub", false),
            ("sadd", "add", true),
            ("ssub", "sub", true),
        ] {
            for inverted in [false, true] {
                let name = format!("{op}{bits}_{}", u8::from(inverted));
                let invert = if inverted {
                    "v4=iconst.i8 1\n v5=xor v3,v4\n"
                } else {
                    ""
                };
                let (condition, yes, no) = if inverted {
                    ("v5", "block1(v2)", "block2")
                } else {
                    ("v3", "block2", "block1(v2)")
                };
                let returned = if bits == 32 {
                    "v7=uext.i64 v6\n return v7"
                } else {
                    "return v6"
                };
                source.push_str(&format!("func @{name}(i{bits},i{bits}) -> i64 {{\nblock0(v0:i{bits},v1:i{bits}):\n v2,v3={op}_overflow v0,v1\n {invert} brif {condition},{yes},{no}\nblock1(v6:i{bits}):\n {returned}\nblock2:\n v8=iconst.i64 -1\n return v8\n}}\n"));
                oracle.push_str(&format!(
                    "extern uint64_t {name}(uint{bits}_t,uint{bits}_t);\n"
                ));
                let ty = if signed {
                    format!("int{bits}_t")
                } else {
                    format!("uint{bits}_t")
                };
                checks.push_str(&format!("{{ {ty} result; int overflow=__builtin_{builtin}_overflow(({ty})a,({ty})b,&result); assert({name}((uint{bits}_t)a,(uint{bits}_t)b)==(overflow?UINT64_MAX:(uint64_t)(uint{bits}_t)result)); }}\n"));
            }
        }
    }
    oracle.push_str(&format!("int main(void) {{ uint64_t values[]={{0,1,2,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffffULL,0x8000000000000000ULL,UINT64_MAX}}; for(unsigned i=0;i<9;++i) for(unsigned j=0;j<9;++j){{ uint64_t a=values[i],b=values[j]; {checks} }} }}\n"));
    let prepared = native_oracle::PreparedOracle::new("overflow-branches", &oracle, &["-O2"]);
    for pools in [&keel_codegen::aarch64::REGISTERS, &SPILLED] {
        let (text, object) = compile(&source, SelectionTier::Full, AllocationTier::Full, pools);
        assert!(!text.contains("cset"), "{text}");
        assert!(!text.contains("eor"), "{text}");
        prepared.run(&object, "overflow branches");
    }
}

#[test]
fn overflow_flag_extra_users_and_intervening_work_keep_materialization() {
    for body in [
        "v4=uext.i64 v3\n brif v3,block1(v4),block1(v4)",
        "v4=iadd v2,v0\n brif v3,block1(v4),block1(v4)",
    ] {
        let src = format!(
            "func @f(i64,i64)->i64 {{\nblock0(v0:i64,v1:i64):\n v2,v3=uadd_overflow v0,v1\n {body}\nblock1(v5:i64):\n return v5\n}}"
        );
        let (text, _) = compile(
            &src,
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::aarch64::REGISTERS,
        );
        assert!(text.contains("cset"), "{text}");
    }
}

fn masked_source(bits: u32, mask: i64, cc: &str, branch: bool, live: bool) -> String {
    let tail = if branch {
        "brif v4, block1, block2\nblock1:\n v5 = iconst.i64 37\n return v5\nblock2:\n v6 = iconst.i64 91\n return v6".to_string()
    } else if live {
        format!("v5 = uext.i{bits} v4\n v6 = xor v5, v2\n return v6")
    } else {
        "v5 = uext.i64 v4\n return v5".to_string()
    };
    let ret_bits = if live { bits } else { 64 };
    format!(
        "func @f(i{bits}) -> i{ret_bits} {{
block0(v0: i{bits}):
 v1 = iconst.i{bits} {mask}
 v2 = and v0, v1
 v3 = iconst.i{bits} 0
 v4 = icmp.{cc} v2, v3
 {tail}
}}"
    )
}

#[test]
fn masked_equality_tests_fuse_with_spills_and_native_execution() {
    for (bits, mask) in [(32, 128), (32, i32::MIN as i64), (64, i64::MIN + 1)] {
        for cc in ["eq", "ne"] {
            for branch in [false, true] {
                let src = masked_source(bits, mask, cc, branch, false);
                let expected = if cc == "eq" { "==" } else { "!=" };
                let result = if branch { "? 37 : 91" } else { "" };
                let oracle = native_oracle::PreparedOracle::new(
                    "masked-test",
                    &format!(
                        "#include <stdint.h>\nextern uint64_t f(uint{bits}_t);\nint main(void) {{\n uint{bits}_t m = (uint{bits}_t)UINT64_C({});\n uint{bits}_t values[] = {{0, 1, 127, 128, 255, (uint{bits}_t)-1, m, ~m}};\n for (unsigned i=0;i<8;i++) {{ uint{bits}_t x=values[i];\n if (f(x) != (((x & m) {expected} 0) {result})) return 1; }}\n return 0; }}",
                        mask as u64
                    ),
                    &["-O2"],
                );
                for (tier, alloc, pools) in codegen::matrix(
                    &[SelectionTier::Covers, SelectionTier::Full],
                    &[&SPILLED, &keel_codegen::aarch64::REGISTERS],
                ) {
                    let (text, object) = compile(&src, tier, alloc, pools);
                    assert!(text.contains("\ttst\t"), "{text}");
                    assert!(!text.contains("\tand\t"), "{text}");
                    assert!(!text.contains("\tcmp\t"), "{text}");
                    oracle.run(&object, &text);
                }
            }
        }
    }
}

#[test]
fn masked_tests_preserve_live_results_signed_conditions_and_unencodable_masks() {
    for (bits, mask, cc, live) in [
        (32, 128, "eq", true),
        (64, 128, "slt", false),
        (64, 0x12345678, "ne", false),
    ] {
        let (text, _) = compile(
            &masked_source(bits, mask, cc, false, live),
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::aarch64::REGISTERS,
        );
        assert!(text.contains("\tand\t"), "{text}");
        assert!(text.contains("\tcmp\t"), "{text}");
    }
}

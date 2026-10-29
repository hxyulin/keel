//! Bounded x86 multiply-add selection; native/oracle checks require a host lease.
use keel_codegen::select::{Machine, Selection, Target as SelectTarget};
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn source(ty: &str, factor: i64, displacement: i64) -> String {
    format!(
        "func @f({ty}) -> {ty} {{
block0(v0: {ty}):
 v1 = iconst.{ty} {factor}
 v2 = imul v0, v1
 v3 = iconst.{ty} {displacement}
 v4 = iadd v2, v3
 return v4
}}"
    )
}
fn count(text: &str, opcode: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with(&format!("{opcode}\t")))
        .count()
}
fn selected(src: &str) -> bool {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let plan = Selection::compute_with_options(
        &module.functions[0],
        SelectTarget::X86,
        SelectionOptions::ALL,
    );
    plan.matches
        .values()
        .any(|m| matches!(m.machine, Machine::LeaMul3 { .. }))
}
#[test]
fn exact_factors_widths_displacements_and_commutations() {
    for ty in ["i32", "i64"] {
        for displacement in [i32::MIN as i64, -1, 0, 7, i32::MAX as i64] {
            let src = source(ty, 3, displacement);
            for src in [
                src.clone(),
                src.replace("imul v0, v1", "imul v1, v0"),
                src.replace("iadd v2, v3", "iadd v3, v2"),
            ] {
                assert!(selected(&src), "{src}");
            }
        }
    }
    for (ty, factor, displacement) in [
        ("i8", 3, 7),
        ("i16", 3, 7),
        ("i64", 2, 7),
        ("i64", 5, 7),
        ("i64", 9, 7),
        ("i64", -3, 7),
        ("i64", 3, 2147483648),
        ("i64", 3, -2147483649),
        ("i64", 4294967299, 7),
    ] {
        assert!(!selected(&source(ty, factor, displacement)));
    }
    // i32 constants are bit patterns; normalize before testing disp32/factor.
    assert!(selected(&source("i32", 4294967299, 4294967303)));
}
#[test]
fn shared_cross_block_and_separately_breakable_products_fall_back() {
    let src = source("i64", 3, 7);
    for negative in [
        src.replace("return v4", "v5 = iadd v4, v2\n return v5"),
        src.replace("v3 = iconst", "jump block1\nblock1:\n v3 = iconst"),
        format!(
            "file f0 = \"lea.c\"\n{}",
            src.replace("imul v0, v1", "imul v0, v1 !f0:2:1")
                .replace("iadd v2, v3", "iadd v2, v3 !f0:3:1")
        ),
    ] {
        assert!(!selected(&negative), "{negative}");
    }
}
#[test]
fn retained_actual_bridge_ir_selects_only_in_tree_modes_and_both_targets_assemble() {
    // Exact retained actual C bridge output at the approved base, not invented IR.
    // Fresh C->IR activation is a separate mandatory leased acceptance check.
    let module = keel_ir::parse(include_str!("fixtures/bridge_madd.ir")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in Target::NATIVE {
        for trees in [false, true] {
            for folded_operands in [false, true] {
                for register_hints in [false, true] {
                    let options = SelectionOptions {
                        trees,
                        folded_operands,
                        register_hints,
                    };
                    let text = compile_with_options(&module, target, options).unwrap();
                    if target == Target::X86_64LinuxGnu {
                        assert_eq!(count(&text, "imul"), usize::from(!trees), "{text}");
                        assert_eq!(count(&text, "lea"), usize::from(trees), "{text}");
                        if trees {
                            let lea = text
                                .lines()
                                .find(|l| l.trim_start().starts_with("lea\t"))
                                .unwrap();
                            assert!(lea.contains(" * 2") && lea.contains('7'), "{lea}");
                        }
                    }
                    let selector = keel_codegen::OptionsSelector(options);
                    let program = keel_codegen::compile_program_with_pipeline(
                        &module,
                        target,
                        &keel_codegen::CodegenPipeline {
                            selector: &selector,
                            allocator: &keel_codegen::regalloc::AllocationTier::Full,
                        },
                    )
                    .unwrap();
                    let (object, parsed) = match target {
                        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                        Target::Aarch64LinuxGnu => (
                            keel_asm::assemble_elf_program_object(&program, Default::default())
                                .unwrap(),
                            keel_asm::assemble_elf_object(&text).unwrap(),
                        ),
                        Target::Aarch64AppleDarwin => (
                            keel_asm::assemble_program_object(
                                &program,
                                keel_asm::TargetFeatures::default(),
                            )
                            .unwrap(),
                            keel_asm::assemble_object(&text).unwrap(),
                        ),
                        Target::X86_64LinuxGnu => (
                            keel_asm::x86::assemble_program_object(&program).unwrap(),
                            keel_asm::x86::assemble_object(&text).unwrap(),
                        ),
                    };
                    assert_eq!(object, parsed);
                }
            }
        }
    }
}
#[test]
fn aliases_resolve_leaves_and_never_publish_a_dead_root() {
    let module = keel_ir::parse(
        &source("i64", 3, 7)
            .replace("func @f(i64)", "func @f(i64, i64)")
            .replace("block0(v0: i64)", "block0(v0: i64, v9: i64)"),
    )
    .unwrap();
    let mut f = module.functions[0].clone();
    let block = f.blocks().next().unwrap();
    let params = f.block_params(block).to_vec();
    f.replace_all_uses(params[0], params[1]);
    let insts = f.block_insts(block).to_vec();
    let plan = Selection::compute_with_options(&f, SelectTarget::X86, SelectionOptions::ALL);
    assert_eq!(plan.operands(&f, insts[3]), vec![params[1]]);
    let result = f.inst_results(insts[3])[0];
    f.replace_all_uses(result, params[1]);
    let plan = Selection::compute_with_options(&f, SelectTarget::X86, SelectionOptions::ALL);
    assert!(!plan.matches.contains_key(&insts[3]));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_edges_match_separately_compiled_clang_oracle_with_spills() {
    use keel_codegen::regalloc::{AllocationTier, Pools};
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("x86-lea-oracle");
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::new();
    let mut oracle = String::from("#include <stdint.h>\n");
    let mut driver = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    let mut calls = String::new();
    let mut index = 0;
    for (ty, cty) in [("i32", "uint32_t"), ("i64", "uint64_t")] {
        for (factor, displacement, shared) in [
            (3, -2147483648i64, false),
            (3, -1, false),
            (3, 0, false),
            (3, 7, false),
            (3, 2147483647, false),
            (3, 2147483648, false),
            (3, -2147483649, false),
            (5, 7, false),
            (9, 7, false),
            (3, 7, true),
        ] {
            let name = format!("edge{index}");
            let mut src = source(ty, factor, displacement).replace("@f(", &format!("@{name}("));
            if shared {
                src = src.replace("return v4", "v5 = iadd v4, v2\n return v5");
            }
            // Check activation/fallback before emitting any native object.
            assert_eq!(
                selected(&src),
                factor == 3 && !shared && (ty == "i32" || i32::try_from(displacement).is_ok())
            );
            ir.push_str(&src);
            ir.push('\n');
            oracle.push_str(&format!("{cty} ref{index}({cty} x) {{ {cty} p=x*({cty}){factor}ULL; return ({cty})(p+({cty})({displacement}LL){}); }}\n", if shared { "+p" } else { "" }));
            driver.push_str(&format!(
                "extern {cty} {name}({cty}); extern {cty} ref{index}({cty});\n"
            ));
            calls.push_str(&format!("{{ uint64_t a={name}(({cty})x), b=ref{index}(({cty})x); printf(\"{name} %llu %llu %llu\\n\",(unsigned long long)x,(unsigned long long)a,(unsigned long long)b); if(a!=b) return 1; ++checks; }}\n"));
            index += 1;
        }
    }
    // An SSA compare stays live across the fused arithmetic; no flag assumption.
    ir.push_str("func @live(i64, i64) -> i64 {\nblock0(v0: i64, v6: i64):\n v7 = icmp.ult v0, v6\n v1 = iconst.i64 3\n v2 = imul v0, v1\n v3 = iconst.i64 7\n v4 = iadd v2, v3\n v5 = select v7, v4, v6\n return v5\n}\n");
    oracle.push_str("uint64_t reflive(uint64_t x,uint64_t y) { return x<y ? x*3ULL+7ULL : y; }\n");
    driver.push_str("extern uint64_t live(uint64_t,uint64_t), reflive(uint64_t,uint64_t);\n");
    driver.push_str("int main(void) { unsigned checks=0; const uint64_t edges[]={0,1,2,127,128,255,256,2147483647ULL,2147483648ULL,4294967295ULL,4294967296ULL,9223372036854775807ULL,9223372036854775808ULL,18446744073709551614ULL,18446744073709551615ULL}; for(unsigned i=0;i<79;i++){ uint64_t x=i<15?edges[i]:0x9e3779b97f4a7c15ULL*(i-14);\n");
    driver.push_str(&calls);
    driver.push_str("for(unsigned j=0;j<15;j++){uint64_t y=edges[j],a=live(x,y),b=reflive(x,y);printf(\"live %llu %llu %llu %llu\\n\",(unsigned long long)x,(unsigned long long)y,(unsigned long long)a,(unsigned long long)b);if(a!=b)return 2;++checks;} } printf(\"PASS checks=%u\\n\",checks);return 0;}\n");
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    fs::write(dir.join("driver.c"), driver).unwrap();
    fs::write(dir.join("suite.ir"), &ir).unwrap();
    let run = |command: &mut Command, stem: &str| {
        let out = command.output().unwrap();
        fs::write(dir.join(format!("{stem}.stdout")), &out.stdout).unwrap();
        fs::write(dir.join(format!("{stem}.stderr")), &out.stderr).unwrap();
        assert!(out.status.success(), "{command:?}: {out:?}");
        out
    };
    run(
        Command::new("clang")
            .args(["-O0", "-fno-lto", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o")),
        "oracle-compile",
    );
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let tiny = Pools {
        caller_saved: &[6, 7],
        callee_saved: &[3, 12],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let spilled = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool_name, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("tiny", &tiny),
            ("spilled", &spilled),
        ] {
            for bits in 0..8 {
                let options = SelectionOptions {
                    trees: bits & 1 != 0,
                    folded_operands: bits & 2 != 0,
                    register_hints: bits & 4 != 0,
                };
                let name = format!("{allocation:?}-{pool_name}-{bits}");
                let selector = keel_codegen::OptionsSelector(options);
                let pipeline = keel_codegen::CodegenPipeline {
                    selector: &selector,
                    allocator: &allocation,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let object = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
                fs::write(dir.join(format!("{name}.s")), text).unwrap();
                let obj = dir.join(format!("{name}.o"));
                fs::write(&obj, object).unwrap();
                let exe = dir.join(&name);
                run(
                    Command::new("clang")
                        .args(["-O0", "-fno-lto"])
                        .arg(dir.join("driver.c"))
                        .arg(dir.join("oracle.o"))
                        .arg(obj)
                        .arg("-o")
                        .arg(&exe),
                    &format!("{name}-link"),
                );
                let out = run(&mut Command::new(&exe), &name);
                assert!(String::from_utf8_lossy(&out.stdout).ends_with("PASS checks=2765\n"));
            }
        }
    }
}

#[test]
fn located_product_is_not_lost_at_an_unlocated_root() {
    let original = include_str!("fixtures/debug_unlocated_lea_root.ir");
    for (child, root, should_select) in [
        (" !f0:2:1", "", false),
        (" !f0:2:1", " !f0:2:4", true),
        (" !f0:2:1", " !f0:3:1", false),
        ("", " !f0:2:1", true),
        ("", "", true),
    ] {
        for intervening in [true, false] {
            let mut src = original
                .replace("imul v0, v2 !f0:2:1", &format!("imul v0, v2{child}"))
                .replace("iadd v4, v3", &format!("iadd v4, v3{root}"));
            if !intervening {
                src = src.replace("    store v0, v1 !f0:3:1\n", "");
            }
            assert_eq!(selected(&src), should_select, "{src}");
            let module = keel_ir::parse(&src).unwrap();
            let text = compile_with_options(&module, Target::X86_64LinuxGnu, SelectionOptions::ALL)
                .unwrap();
            assert_eq!(count(&text, "imul"), usize::from(!should_select), "{text}");
            if !should_select {
                // Follow actual emitted .loc state, excluding directives/comments.
                let mut line = 0;
                let mut multiply_line = None;
                for instruction in text.lines().map(str::trim) {
                    if instruction.starts_with(".loc") {
                        line = instruction
                            .split_whitespace()
                            .nth(2)
                            .unwrap()
                            .parse::<u32>()
                            .unwrap();
                    } else if instruction.starts_with("imul\t") {
                        multiply_line = Some(line);
                    }
                }
                assert_eq!(multiply_line, Some(2), "{text}");
            }
        }
    }
}

#[test]
fn sums_into_a_third_register_use_lea() {
    // Both inputs stay live past each sum, so it needs its own register.
    for (ty, rhs, wide) in [
        ("i64", "v1", true),
        ("i32", "v1", false),
        ("i64", "v6", true),
        ("i32", "v6", false),
    ] {
        let src = format!(
            "func @f({ty}, {ty}, ptr) -> {ty} {{
block0(v0: {ty}, v1: {ty}, v2: ptr):
 v6 = iconst.{ty} -7
 v3 = iadd v0, {rhs}
 store v3, v2
 v4 = imul v0, v1
 v5 = imul v4, v3
 return v5
}}"
        );
        let module = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let text =
            compile_with_options(&module, Target::X86_64LinuxGnu, SelectionOptions::ALL).unwrap();
        keel_asm::x86::assemble_object(&text).unwrap();
        assert_eq!(count(&text, "add"), 0, "{text}");
        let sum = text
            .lines()
            .find(|l| l.trim_start().starts_with("lea\t") && !l.contains("rip"))
            .unwrap_or_else(|| panic!("{text}"));
        // A 32-bit sum writes a 32-bit register; the address is 64-bit.
        let d = sum.trim_start()["lea\t".len()..].split(',').next().unwrap();
        assert_eq!(d.starts_with('r') && !d.ends_with('d'), wide, "{sum}");
    }
    // A displacement must fit 32 bits; a wider constant keeps the add.
    let src = "func @f(i64, ptr) -> i64 {
block0(v0: i64, v1: ptr):
 v2 = iconst.i64 2147483648
 v3 = iadd v0, v2
 store v3, v1
 v4 = imul v0, v3
 return v4
}";
    let module = keel_ir::parse(src).unwrap();
    let text =
        compile_with_options(&module, Target::X86_64LinuxGnu, SelectionOptions::ALL).unwrap();
    assert_eq!(count(&text, "add"), 1, "{text}");
}

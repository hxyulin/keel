//! Exercise the emitter directly so allocation choices cannot hide a cycle.
use super::*;
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};

const VREGS: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, FS2, 31];
const REMAT: u64 = 0x1234;

type CopyCase = (&'static str, Vec<(Loc, Loc)>, bool);
fn cases() -> Vec<CopyCase> {
    use Loc::{FReg as F, Reg as R, Stack as S};
    vec![
        ("two", vec![(F(0), F(1)), (F(1), F(0))], true),
        (
            "three",
            vec![(F(0), F(1)), (F(1), F(2)), (F(2), F(0))],
            true,
        ),
        (
            "four",
            vec![(F(0), F(1)), (F(1), F(2)), (F(2), F(3)), (F(3), F(0))],
            true,
        ),
        (
            "fanout",
            vec![(F(0), F(1)), (F(1), F(0)), (F(2), F(0)), (F(3), F(1))],
            true,
        ),
        (
            "disjoint",
            vec![(F(0), F(1)), (F(1), F(0)), (F(2), F(3)), (F(3), F(2))],
            true,
        ),
        // Non-FP self-copies must not disqualify an otherwise pure copy.
        (
            "self",
            vec![(F(0), F(1)), (F(1), F(0)), (R(9), R(9)), (S(0), S(0))],
            true,
        ),
        (
            "scratch_source",
            vec![(F(0), F(1)), (F(1), F(0)), (F(2), F(FS2))],
            false,
        ),
        (
            "scratch_dest",
            vec![(F(0), F(1)), (F(1), F(0)), (F(FS2), F(2))],
            false,
        ),
        (
            "scratch_self",
            vec![(F(0), F(1)), (F(1), F(0)), (F(FS2), F(FS2))],
            false,
        ),
        ("scratch_cycle", vec![(F(0), F(FS2)), (F(FS2), F(0))], false),
        (
            "mixed_gpr",
            vec![(F(0), F(1)), (F(1), F(0)), (R(9), R(10)), (R(10), R(9))],
            false,
        ),
        (
            "mixed_stack",
            vec![(F(0), F(1)), (F(1), F(0)), (S(0), S(2)), (S(2), S(0))],
            false,
        ),
        (
            "spilled_cycle",
            vec![(F(0), S(0)), (S(0), F(1)), (F(1), F(0))],
            false,
        ),
        (
            "remat",
            vec![
                (F(0), F(1)),
                (F(1), F(0)),
                (R(9), Loc::Remat(Value::new(1))),
            ],
            false,
        ),
        (
            "cross_class",
            vec![(F(0), F(1)), (F(1), F(0)), (F(2), R(9)), (R(10), F(3))],
            false,
        ),
    ]
}

fn module() -> Module {
    keel_ir::parse("func @fixture(ptr) {\nblock0(v0: ptr):\n v1 = iconst.i64 4660\n v2 = load.i32x4 v0\n store v2,v0\n return\n}\n").unwrap()
}

fn compiler(module: &Module, structured: bool) -> FunctionCompiler<'_> {
    let pipeline = crate::CodegenPipeline {
        selector: &crate::SelectionTier::Baseline,
        allocator: &regalloc::AllocationTier::Full,
    };
    // The compiler borrows the module's symbol index for its whole life.
    let symbols = Box::leak(Box::new(crate::symbols::Symbols::new(module)));
    let mut c = FunctionCompiler::new(
        &module.functions[0],
        symbols,
        &REGISTERS,
        &pipeline,
        structured,
        &leaf_calls::Summaries::default(),
    )
    .unwrap();
    assert!(
        c.copy_temp.is_some(),
        "fixture must select vector-width moves"
    );
    // A fixed test frame: two 16-byte forced spill locations and the existing
    // 16-byte cycle temporary. Production frame construction is untouched.
    c.copy_temp = Some(4);
    c.vector_spills = [(0, 16), (2, 16), (4, 16)].into_iter().collect();
    c.spill_base = 0;
    c
}

fn emit(structured: bool) -> Output {
    let module = module();
    let mut c = compiler(&module, structured);
    c.out.section("__TEXT", "__text", SectionKind::Data);
    for (name, moves, _) in cases() {
        let symbol = format!("_copy_{name}");
        c.out.directive(Directive::Global(symbol.clone()));
        c.out.label(symbol);
        c.mach("sub", [Reg::SP.into(), Reg::SP.into(), Arg::imm(64)]);
        // x0/x1 point to input/output snapshots. All used registers are
        // caller-saved, including v31, an unrelated live 128-bit sentinel.
        for (i, reg) in VREGS.into_iter().enumerate() {
            c.mach(
                "ldr",
                [
                    Reg::q(reg).into(),
                    Address::offset(Reg::x(0), i * 16).into(),
                ],
            );
        }
        for (i, slot) in [0, 2].into_iter().enumerate() {
            c.mach(
                "ldr",
                [
                    Reg::q(FS0).into(),
                    Address::offset(Reg::x(0), (9 + i) * 16).into(),
                ],
            );
            c.spill(true, true, FS0, slot);
        }
        for (i, reg) in [9, 10].into_iter().enumerate() {
            c.mach(
                "ldr",
                [
                    Reg::x(reg).into(),
                    Address::offset(Reg::x(0), (11 + i) * 16).into(),
                ],
            );
        }
        // Both equal and unequal inputs run natively. The cycle moves must
        // preserve NZCV until the pending condition is materialized.
        c.mach("cmp", [Reg::x(2).into(), Arg::imm(7)]);
        c.out.label(format!("L{name}_copy_start"));
        c.parallel_copy(&moves);
        c.out.label(format!("L{name}_copy_end"));
        c.mach("cset", [Reg::w(0).into(), Arg::from("eq")]);
        for (i, reg) in VREGS.into_iter().enumerate() {
            c.mach(
                "str",
                [
                    Reg::q(reg).into(),
                    Address::offset(Reg::x(1), i * 16).into(),
                ],
            );
        }
        for (i, slot) in [0, 2].into_iter().enumerate() {
            c.spill(false, true, FS0, slot);
            c.mach(
                "str",
                [
                    Reg::q(FS0).into(),
                    Address::offset(Reg::x(1), (9 + i) * 16).into(),
                ],
            );
        }
        for (i, reg) in [9, 10].into_iter().enumerate() {
            c.mach(
                "str",
                [
                    Reg::x(reg).into(),
                    Address::offset(Reg::x(1), (11 + i) * 16).into(),
                ],
            );
        }
        c.mach("add", [Reg::SP.into(), Reg::SP.into(), Arg::imm(64)]);
        c.native(Opcode::RET_64R_branch_reg, [(FieldId::Rn, 30)], |out| {
            out.write_str("ret")
        });
    }
    c.out
}

fn objects() -> (String, Vec<u8>, Vec<u8>) {
    let Output::Text(text) = emit(false) else {
        unreachable!()
    };
    let text = text.into_string();
    let Output::Structured(program) = emit(true) else {
        unreachable!()
    };
    assert_eq!(program.stats().text_bytes, 0);
    let typed = assemble_program_object(&program, TargetFeatures::default()).unwrap();
    let parsed = assemble_object(&text).unwrap();
    assert_eq!(typed, parsed, "copy emitter object routes disagree");
    (text, typed, parsed)
}

#[test]
fn parallel_copy_vector_cycles_select_safe_full_width_temporary() {
    let (text, _, _) = objects();
    for (name, _, scratch) in cases() {
        let body = text
            .split(&format!("L{name}_copy_start:\n"))
            .nth(1)
            .unwrap()
            .split(&format!("L{name}_copy_end:"))
            .next()
            .unwrap();
        if scratch {
            assert!(body.contains("mov\tv18.16b"), "{name}: {body}");
            assert!(!body.contains("[sp"), "{name}: {body}");
            assert!(!body.contains("fmov"), "{name}: must retain all 128 bits");
        } else {
            assert!(
                body.contains("str\tq"),
                "{name}: must keep stack cycle temporary: {body}"
            );
            assert!(body.contains("ldr\tq"), "{name}: {body}");
        }
    }
}

#[test]
fn parallel_copy_scalar_functions_keep_original_temporary() {
    let module = module();
    let mut c = compiler(&module, false);
    c.copy_temp = None;
    c.parallel_copy(&[(Loc::FReg(0), Loc::FReg(1)), (Loc::FReg(1), Loc::FReg(0))]);
    let Output::Text(text) = c.out else {
        unreachable!()
    };
    let text = text.into_string();
    assert!(text.contains("fmov\tx16, d0"), "{text}");
    assert!(text.contains("fmov\td0, d1"), "{text}");
    assert!(!text.contains("v18"));
}

#[test]
fn parallel_copy_scalar_cycles_in_vector_functions_use_x16() {
    use Loc::{Reg as R, Stack as S};
    let module = module();
    let mut c = compiler(&module, false);
    c.parallel_copy(&[(R(9), R(10)), (R(10), R(9))]);
    c.parallel_copy(&[(R(11), S(6)), (S(6), R(11))]);
    // A vector stack slot still needs the full-width stack temporary.
    c.parallel_copy(&[(R(12), S(0)), (S(0), R(12))]);
    let Output::Text(text) = c.out else {
        unreachable!()
    };
    let text = text.into_string();
    let (scalar, vector) = text.split_once("x12").unwrap();
    assert!(scalar.contains("mov\tx16, x9"), "{text}");
    assert!(scalar.contains("x16, [sp, #48]"), "{text}");
    assert!(!scalar.contains("[sp, #32]"), "{text}");
    assert!(vector.contains("[sp, #32]"), "{text}");
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn parallel_copy_native_raw_bits_live_values_and_pending_flags() {
    use std::{fs, process::Command};
    fn index(loc: Loc) -> usize {
        match loc {
            Loc::FReg(r) => VREGS.iter().position(|&v| v == r).unwrap(),
            Loc::Stack(s) => 9 + s as usize / 2,
            Loc::Reg(r) => 11 + (r - 9) as usize,
            Loc::Remat(_) => unreachable!(),
        }
    }
    // objects() asserts that both routes give the same bytes, so one run covers both.
    let (_, object, _) = objects();
    let dir = std::env::temp_dir().join(format!("keel-arm-copy-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    for (name, _, _) in cases() {
        c += &format!("extern int copy_{name}(const void*,void*,uint64_t);\n");
    }
    c += "int main(void){\n";
    // Distinct low/high halves include signed zero, infinity, signaling and
    // quiet F64 NaNs with payloads, and arbitrary non-FP vector lane bits.
    for seed in 0..8u64 {
        let bits = [
            0x7ff0_0000_0000_0001u64,
            0xfff8_dead_beef_4321,
            0x8000_0000_0000_0000,
            0x7ff0_0000_0000_0000,
            0x0123_4567_89ab_cdef,
            0xffff_ffff_0000_0001,
        ];
        let input: Vec<u128> = (0..13)
            .map(|i| {
                let low = bits[(i + seed as usize) % bits.len()] ^ (i as u64 * 131);
                let high = bits[(i + seed as usize + 1) % bits.len()] ^ (i as u64 * 919);
                u128::from(low) | if i < 11 { u128::from(high) << 64 } else { 0 }
            })
            .collect();
        let array = |values: &[u128]| {
            values
                .iter()
                .flat_map(|v| [*v as u64, (*v >> 64) as u64])
                .map(|v| format!("UINT64_C({v})"))
                .collect::<Vec<_>>()
                .join(",")
        };
        for (name, moves, scratch) in cases() {
            let mut expected = input.clone();
            for (dst, src) in moves {
                let mut value = if matches!(src, Loc::Remat(_)) {
                    u128::from(REMAT)
                } else {
                    input[index(src)]
                };
                if matches!(dst, Loc::Reg(_)) || matches!(src, Loc::Reg(_)) {
                    value &= u128::from(u64::MAX);
                }
                expected[index(dst)] = value;
            }
            // v18 is reserved and deliberately changed by eligible cycles;
            // mixed stack copies may also use it in their original helper.
            let check_scratch = !scratch && !matches!(name, "mixed_stack");
            c += &format!(
                "{{uint64_t in[26]={{{}}},want[26]={{{}}},out[26];for(unsigned flag=6;flag<=7;flag++){{memset(out,0,sizeof(out));if(copy_{name}(in,out,flag)!=(flag==7))return 1;for(unsigned i=0;i<26;i++){{if(i==23||i==25{} )continue;if(out[i]!=want[i])return 2;}}}}}}\n",
                array(&input),
                array(&expected),
                if check_scratch { "" } else { "||i==14||i==15" }
            );
        }
    }
    c += "return 0;}\n";
    fs::write(dir.join("test.c"), c).unwrap();
    let exe = dir.join("copies");
    fs::write(dir.join("copies.o"), object).unwrap();
    let result = Command::new("clang")
        .args(["-O2", "-ffp-contract=off"])
        .arg(dir.join("test.c"))
        .arg(dir.join("copies.o"))
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        Command::new(exe).status().unwrap().success(),
        "raw-bit/flags harness failed"
    );
    fs::remove_dir_all(dir).unwrap();
}

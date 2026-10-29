//! Constant-divisor division on AArch64: reciprocal multiplies and shifts
//! replace `sdiv`/`udiv`, while divisors that trap or are 1 keep the
//! hardware path and its checks.

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

const WIDTHS: [u32; 4] = [8, 16, 32, 64];
const OPS: [&str; 4] = ["sdiv", "srem", "udiv", "urem"];

fn compile(
    m: &keel_ir::Module,
    selection: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
    (text, object)
}

fn mask(width: u32) -> u64 {
    u64::MAX >> (64 - width)
}

/// The divisor as the operation sees it: wrapped to the width, then signed
/// or unsigned.
fn normalized(width: u32, op: &str, imm: i64) -> i128 {
    let bits = imm as u64 & mask(width);
    if op.starts_with('s') {
        i128::from(((bits << (64 - width)) as i64) >> (64 - width))
    } else {
        i128::from(bits)
    }
}

/// Divisor immediates: small and large, both signs, powers of two, MIN and
/// MAX, and spellings that only normalize to a divisor at the IR width.
fn divisors(width: u32) -> Vec<i64> {
    let min = i64::MIN >> (64 - width);
    let max = !min;
    let mut divisors = vec![
        0,
        1,
        -1,
        2,
        -2,
        3,
        -3,
        4,
        -4,
        5,
        -5,
        6,
        7,
        -7,
        10,
        16,
        -64,
        100,
        min,
        min + 1,
        max,
        max - 1,
        max / 3,
        min / 3,
        mask(width) as i64,
        (mask(width) - 2) as i64,
    ];
    if width >= 16 {
        divisors.extend([641, -1000, 1 << 13, 12345]);
    }
    if width >= 32 {
        divisors.extend([1_000_003, -(1 << 20), 0x7fff_0001]);
    }
    if width == 64 {
        divisors.extend([1 << 40, 0x0123_4567_89ab_cdef, -0x0123_4567_89ab_cdef]);
    }
    if width < 64 {
        // Noncanonical: 3, -3, 0 and -1 spelled above the IR width.
        divisors.extend([
            (1i64 << width) + 3,
            (1i64 << width) - 3,
            1i64 << width,
            (1i64 << width) - 1,
        ]);
    }
    divisors
}

fn ext(width: u32, op: &str) -> &'static str {
    match (width < 32, op.starts_with('s')) {
        (false, _) => "",
        (true, true) => " sext",
        (true, false) => " uext",
    }
}

fn single(name: &str, width: u32, op: &str, imm: i64) -> String {
    format!(
        "func @{name}(i{width}) -> i{width}{} cc(c) {{\nblock0(v0: i{width}):\n v1 = iconst.i{width} {imm}\n v2 = {op} v0, v1\n return v2\n}}\n",
        ext(width, op)
    )
}

/// The quotient feeds a second division while the dividend and divisor stay
/// live, so results may alias either operand.
fn chained(name: &str, width: u32, op: &str, imm: i64) -> String {
    format!(
        "func @{name}(i{width}) -> i{width}{} cc(c) {{\nblock0(v0: i{width}):\n v1 = iconst.i{width} {imm}\n v2 = {op} v0, v1\n v3 = iadd v2, v0\n v4 = {op} v3, v1\n v5 = isub v4, v2\n v6 = imul v5, v1\n return v6\n}}\n",
        ext(width, op)
    )
}

/// Reductions apply for divisors other than 0 and 1 (and -1 if signed).
fn reduced(width: u32, op: &str, imm: i64) -> bool {
    let d = normalized(width, op, imm);
    if op.starts_with('s') {
        !(-1..=1).contains(&d)
    } else {
        d > 1
    }
}

#[test]
fn constant_divisors_avoid_hardware_division_and_keep_traps() {
    for width in WIDTHS {
        for op in OPS {
            for imm in divisors(width) {
                let m = keel_ir::parse(&single("f", width, op, imm)).unwrap();
                let d = normalized(width, op, imm);
                for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                            let (text, _) = compile(&m, selection, allocation, pools);
                            let context = format!("i{width} {op} {imm} (= {d})\n{text}");
                            let hardware = text.contains("\tsdiv\t") || text.contains("\tudiv\t");
                            assert_eq!(hardware, !reduced(width, op, imm), "{context}");
                            assert_eq!(text.contains("\tcbz\t"), d == 0, "{context}");
                            let overflow = op == "sdiv" && d == -1;
                            assert_eq!(text.contains("\tccmp\t"), overflow, "{context}");
                            let multiply = ["smulh", "smull", "umulh", "umull"]
                                .iter()
                                .any(|m| text.contains(&format!("\t{m}\t")));
                            let power = d.unsigned_abs().is_power_of_two();
                            assert_eq!(multiply, reduced(width, op, imm) && !power, "{context}");
                        }
                    }
                }
            }
        }
    }
}

/// The instructions emitted for the division itself, after the incoming
/// argument is copied to its allocated register.
fn shape(width: u32, op: &str, imm: i64) -> String {
    let m = keel_ir::parse(&single("f", width, op, imm)).unwrap();
    let (text, _) = compile(
        &m,
        SelectionTier::Full,
        AllocationTier::Full,
        &keel_codegen::aarch64::REGISTERS,
    );
    text.lines()
        .skip_while(|l| !l.contains(&format!("v2 = {op} v0, v1")))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with(';'))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("; ")
}

#[test]
fn known_sequences() {
    let shapes = [
        (32, "sdiv", 3),
        (32, "srem", -5),
        (64, "sdiv", -8),
        (32, "sdiv", 2),
        (16, "srem", 7),
        (64, "udiv", 7),
        (32, "urem", 7),
    ]
    .map(|(width, op, imm)| shape(width, op, imm));
    assert_eq!(
        shapes,
        [
            // i32 / 3: the high-half and final shifts combine.
            "movz w8, #0x5556; movk w8, #0x5555, lsl #16; smull x8, w9, w8; asr x8, x8, #32; add w9, w8, w8, lsr #31",
            // i32 % -5: a negative multiplier for a negative divisor.
            "movz w8, #0x9999; movk w8, #0x9999, lsl #16; smull x8, w9, w8; asr x8, x8, #33; add w8, w8, w8, lsr #31; msub w9, w8, w10, w9",
            // Powers of two only shift, including negated ones.
            "asr x8, x9, #63; add x8, x9, x8, lsr #61; sub x9, xzr, x8, asr #3",
            "add w8, w9, w9, lsr #31; asr w9, w8, #1",
            // i16 % 7 as i32, with the dividend correction.
            "sxth w16, w9; movz w8, #0x2493; movk w8, #0x9249, lsl #16; smull x8, w16, w8; asr x8, x8, #32; add w8, w8, w16; asr w8, w8, #2; add w8, w8, w8, lsr #31; sxth w17, w10; msub w9, w8, w17, w16",
            // Unsigned averaging: overflow-free in 64 bits, a wide sum in 32.
            "movz x8, #0x2493; movk x8, #0x9249, lsl #16; movk x8, #0x4924, lsl #32; movk x8, #0x2492, lsl #48; umulh x8, x9, x8; sub x17, x9, x8; add x8, x8, x17, lsr #1; lsr x9, x8, #2",
            "movz w8, #0x4925; movk w8, #0x2492, lsl #16; umull x8, w9, w8; lsr x8, x8, #32; add x8, x8, w9, uxtw; lsr x8, x8, #3; msub w9, w8, w10, w9",
        ]
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_constant_division_matches_c_oracle() {
    use std::{fs, os::unix::process::ExitStatusExt, process::Command};
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\nstatic uint64_t seed = 0x9e3779b97f4a7c15ULL;\nstatic uint64_t next(void) { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; return seed; }\n",
    );
    let mut checks = String::new();
    let mut traps = Vec::new();
    for width in WIDTHS {
        let min = 1u64 << (width - 1);
        let max = min - 1;
        // A `TRY(x)` per dividend; `d` is the C divisor, `ud` its bits.
        let dividends = |sample_boundaries: bool| {
            if width <= 16 {
                return format!("for (uint64_t i = 0; i <= {}ULL; i++) TRY(i);", mask(width));
            }
            let mut tries = format!(
                "TRY({min}ULL); TRY({min}ULL + 1); TRY({min}ULL + 2); TRY({max}ULL); TRY({max}ULL - 1); TRY(0); TRY(1); TRY(2); TRY(-1); TRY(-2); TRY(ud); TRY(ud + 1); TRY(ud - 1); TRY(0 - ud); TRY(1 - ud); TRY(-1 - ud); for (int i = 0; i < 20000; i++) {{ uint64_t r = next(); TRY(r); TRY(r >> (r & 63)); TRY((int64_t)r >> (r & 63));"
            );
            if sample_boundaries {
                // Multiples of d nearest the top and the random value.
                tries += " TRY(top / d * d); TRY((__typeof__(d))r / d * d); TRY((uint64_t)((__typeof__(d))r / d * d) - 1);";
            }
            tries + " }"
        };
        for op in OPS {
            let signed = op.starts_with('s');
            let ct = format!("{}int{width}_t", if signed { "" } else { "u" });
            let operator = if op.ends_with("div") { "/" } else { "%" };
            for (index, imm) in divisors(width).into_iter().enumerate() {
                let dn = normalized(width, op, imm);
                for chain in [false, true] {
                    let name = format!("f{width}_{op}_{index}_{}", u8::from(chain));
                    ir += &if chain {
                        chained(&name, width, op, imm)
                    } else {
                        single(&name, width, op, imm)
                    };
                    c += &format!("extern {ct} {name}({ct});\n");
                    if dn == 0 {
                        traps.push(format!("(void){name}(({ct})1);"));
                        continue;
                    }
                    if signed && dn == -1 {
                        if op == "sdiv" {
                            traps.push(format!("(void){name}(({ct}){min}ULL);"));
                        }
                        if chain {
                            // MIN can recur as an intermediate; checked singly.
                            continue;
                        }
                    }
                    // Wrapping arithmetic through the unsigned type; `/` and
                    // `%` skip only MIN / -1, which traps in the IR.
                    let divide = |x: &str| {
                        if signed && dn == -1 {
                            if op == "sdiv" {
                                format!("({ct})(0 - (uint64_t){x})")
                            } else {
                                "0".to_string()
                            }
                        } else {
                            format!("({ct})({x} {operator} d)")
                        }
                    };
                    let expected = if chain {
                        format!(
                            "{ct} q1 = {}; {ct} s1 = ({ct})((uint64_t)q1 + (uint64_t)a); {ct} r1 = {}; {ct} e = ({ct})(((uint64_t)r1 - (uint64_t)q1) * (uint64_t)d);",
                            divide("a"),
                            divide("s1")
                        )
                    } else {
                        format!("{ct} e = {};", divide("a"))
                    };
                    let skip = if signed && op == "sdiv" && dn == -1 {
                        format!("if (a == ({ct}){min}ULL) break;")
                    } else {
                        String::new()
                    };
                    let d = if signed {
                        format!("({ct})({dn}LL)")
                    } else {
                        format!("({ct})({dn}ULL)")
                    };
                    let top = if signed { max } else { mask(width) };
                    let tries = dividends(!(signed && dn == -1));
                    checks += &format!(
                        "{{ const {ct} d = {d}, top = ({ct}){top}ULL; const uint64_t ud = (uint64_t)d; (void)top; (void)ud;\n#define TRY(x) do {{ {ct} a = ({ct})(x); {skip} {expected} {ct} got = {name}(a); if (got != e) {{ printf(\"{name} a=%lld got=%lld want=%lld\\n\", (long long)a, (long long)got, (long long)e); return 1; }} }} while (0)\n {tries}\n#undef TRY\n}}\n"
                    );
                }
            }
        }
    }
    c += "int main(int argc, char **argv) { if (argc > 1) { switch (atoi(argv[1])) {\n";
    for (i, call) in traps.iter().enumerate() {
        c += &format!("case {i}: {call} return 0;\n");
    }
    c += "} return 3; }\n";
    c += &checks;
    c += "return 0; }\n";
    let dir = std::env::temp_dir().join(format!("keel-arm-const-div-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("oracle.c"), &c).unwrap();
    // The oracle is large and invariant: compile it once, link per config.
    // At -O0 clang computes the expected values with hardware sdiv/udiv,
    // independent of LLVM's reciprocal sequences, and compiles in a second
    // instead of 16.
    let driver = Command::new("clang")
        .arg("-O0")
        .arg("-c")
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(driver.status.success(), "{driver:?}");
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(&ir).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                    let (_, object) = compile(&m, selection, allocation, pools);
                    fs::write(dir.join("test.o"), object).unwrap();
                    let link = Command::new("clang")
                        .arg(dir.join("oracle.o"))
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test"))
                        .output()
                        .unwrap();
                    assert!(link.status.success(), "{link:?}");
                    let out = Command::new(dir.join("test")).output().unwrap();
                    let context = format!("{tier:?} {selection:?} {allocation:?}");
                    assert!(out.status.success(), "{context}: {out:?}");
                    for (case, call) in traps.iter().enumerate() {
                        let out = Command::new(dir.join("test"))
                            .arg(case.to_string())
                            .output()
                            .unwrap();
                        assert_eq!(
                            out.status.signal(),
                            Some(5),
                            "case {case} ({call}) {context}: {out:?}"
                        );
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

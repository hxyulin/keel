use keel_asm::assemble;
mod common;
use common::compare_clang;

#[test]
fn integer_operations_match_clang() {
    let mut source = String::from(".text\n");
    for (bits, p) in [(32, "w"), (64, "x")] {
        for op in ["and", "ands", "orr", "eor", "bic", "bics", "orn", "eon"] {
            for shift in ["lsl", "lsr", "asr", "ror"] {
                for amount in [0, bits - 1] {
                    source.push_str(&format!("{op} {p}0, {p}30, {p}zr, {shift} #{amount}\n"));
                }
            }
        }
        for op in [
            "adc", "adcs", "sbc", "sbcs", "sdiv", "udiv", "lslv", "lsrv", "asrv", "rorv", "lsl",
            "lsr", "asr", "ror", "mul", "mneg",
        ] {
            source.push_str(&format!("{op} {p}0, {p}30, {p}zr\n"));
        }
        for op in ["clz", "cls", "rbit", "rev", "rev16", "neg", "negs", "mvn"] {
            source.push_str(&format!("{op} {p}0, {p}zr\n"));
        }
        for op in ["madd", "msub"] {
            source.push_str(&format!("{op} {p}0, {p}30, {p}zr, {p}2\n"));
        }
        for op in ["lsl", "lsr", "asr", "ror"] {
            for amount in [0, 1, bits - 1] {
                source.push_str(&format!("{op} {p}0, {p}30, #{amount}\n"));
            }
        }
        for op in ["sbfm", "ubfm", "bfm"] {
            for (r, s) in [(0, 0), (0, bits - 1), (bits - 1, 0), (bits - 1, bits - 1)] {
                source.push_str(&format!("{op} {p}0, {p}30, #{r}, #{s}\n"));
            }
        }
        for op in ["bfi", "bfxil", "ubfx", "sbfx"] {
            for (lsb, width) in [(0, bits), (1, bits - 1), (bits - 1, 1)] {
                source.push_str(&format!("{op} {p}0, {p}30, #{lsb}, #{width}\n"));
            }
        }
        source.push_str(&format!(
            "extr {p}0, {p}30, {p}zr, #{}\ntst {p}0, {p}zr, ror #1\n",
            bits - 1
        ));
        for op in ["sxtb", "sxth"] {
            source.push_str(&format!("{op} {p}0, w30\n"));
        }
        for op in ["add", "sub", "adds", "subs"] {
            for (extend, m) in [
                ("uxtb", "w2"),
                ("uxth", "w2"),
                ("uxtw", "w2"),
                ("sxtb", "w2"),
                ("sxth", "w2"),
                ("sxtw", "w2"),
            ] {
                source.push_str(&format!("{op} {p}0, {p}30, {m}, {extend} #4\n"));
            }
        }
    }
    source.push_str("rev32 x0, xzr\nsmulh x0, x1, xzr\numulh x0, x1, xzr\nsmull x0, w1, wzr\numull x0, w1, wzr\nsmaddl x0, w1, wzr, x2\numaddl x0, w1, wzr, x2\nuxtb w0, w30\nuxth w0, w30\nsxtw x0, w30\nsub sp, sp, x0\nadd sp, sp, w0, uxtw #4\nadds xzr, sp, x0, sxtx #4\nsub x0, x1, x2, uxtx #0\n");
    compare_clang(&source);
}

#[test]
fn logical_immediates_and_mov_aliases_match_clang() {
    let mut source = String::from(".text\n");
    for (bits, p) in [(32, "w"), (64, "x")] {
        let masks: Vec<u64> = if bits == 32 {
            vec![1, 0x80000000, 0x00ff00ff, 0xfffffffe]
        } else {
            vec![
                1,
                0x8000000000000000,
                0x00ff00ff00ff00ff,
                0xfffffffffffffffe,
            ]
        };
        for op in ["and", "ands", "orr", "eor"] {
            for mask in &masks {
                source.push_str(&format!("{op} {p}0, {p}zr, #0x{mask:x}\n"));
            }
            source.push_str(&format!("{op} {p}0, {p}1, #-16\n"));
        }
        for value in ["0", "-1", "-16", "65536", "0x80000000", "0xff00ff00"] {
            source.push_str(&format!("mov {p}0, #{value}\n"));
        }
        source.push_str(&format!("tst {p}0, #0x55555555\n"));
    }
    source.push_str("and sp, x0, #-16\nmov sp, #0xff00ff00ff00ff00\nmov x0, #0x8000000000000000\nmov x0, #0xffffffffffffffff\nmov x0, #-9223372036854775808\nmov xzr, #0\n");
    // 0x55555555 is not encodable in X form unless repeated across all 64 bits.
    let source = source
        .replace("tst x0, #0x55555555", "tst x0, #0x5555555555555555")
        .replace("mov x0, #0xff00ff00\n", "mov x0, #0xff00ff00ff00ff00\n");
    compare_clang(&source);
}

#[test]
fn every_logical_bitmask_matches_clang() {
    use std::collections::BTreeSet;
    let mut source = String::from(".text\n");
    for (bits, p) in [(32u32, "w"), (64, "x")] {
        let mut values = BTreeSet::new();
        for size in [2u32, 4, 8, 16, 32, 64] {
            if size > bits {
                break;
            }
            let mask = u64::MAX >> (64 - size);
            for ones in 1..size {
                let run = (1u64 << ones) - 1;
                for rotate in 0..size {
                    let element = if rotate == 0 {
                        run
                    } else {
                        ((run >> rotate) | (run << (size - rotate))) & mask
                    };
                    let mut value = 0;
                    for offset in (0..bits).step_by(size as usize) {
                        value |= element << offset;
                    }
                    values.insert(value);
                }
            }
        }
        assert_eq!(values.len(), if bits == 32 { 1302 } else { 5334 });
        for value in values {
            source.push_str(&format!("and {p}0, {p}1, #0x{value:x}\n"));
        }
    }
    compare_clang(&source);
}

#[test]
fn conditions_selects_and_branches_match_clang() {
    let mut source = String::from("Lback: nop\n");
    for cc in [
        "eq", "ne", "hs", "lo", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt",
        "le", "al", "nv",
    ] {
        source.push_str(&format!("b.{cc} Lforward\nb.{cc} Lback\n"));
        for p in ["w", "x"] {
            for op in ["csel", "csinc", "csinv", "csneg"] {
                source.push_str(&format!("{op} {p}0, {p}30, {p}zr, {cc}\n"));
            }
            for op in ["ccmp", "ccmn"] {
                source.push_str(&format!(
                    "{op} {p}0, {p}zr, #0, {cc}\n{op} {p}30, #31, #15, {cc}\n"
                ));
            }
            if !["al", "nv"].contains(&cc) {
                for op in ["cset", "csetm"] {
                    source.push_str(&format!("{op} {p}0, {cc}\n"));
                }
                for op in ["cinc", "cinv", "cneg"] {
                    source.push_str(&format!("{op} {p}0, {p}30, {cc}\n"));
                }
            }
        }
    }
    for op in ["cbz", "cbnz"] {
        for p in ["w", "x"] {
            source.push_str(&format!("{op} {p}zr, Lforward\n{op} {p}30, Lback\n"));
        }
    }
    for op in ["tbz", "tbnz"] {
        for (p, bit) in [
            ("w", 0),
            ("w", 31),
            ("x", 0),
            ("x", 31),
            ("x", 32),
            ("x", 63),
        ] {
            source.push_str(&format!(
                "{op} {p}zr, #{bit}, Lforward\n{op} {p}30, #{bit}, Lback\n"
            ));
        }
    }
    source.push_str("br x30\nblr xzr\nbrk #0\nbrk #65535\nLforward: ret\n");
    compare_clang(&source);
}

#[test]
fn conditional_branch_limits_are_checked() {
    for op in ["b.eq", "cbz x0,", "cbnz w0,"] {
        assert!(assemble(&format!("{op} #-1048576\n{op} #1048572")).is_ok());
        for offset in [-1048580, 1048576, 2] {
            assert!(
                assemble(&format!("{op} #{offset}"))
                    .unwrap_err()
                    .message
                    .contains("displacement")
            );
        }
    }
    for op in ["tbz x0, #63,", "tbnz w0, #31,"] {
        assert!(assemble(&format!("{op} #-32768\n{op} #32764")).is_ok());
        for offset in [-32772, 32768, 2] {
            assert!(
                assemble(&format!("{op} #{offset}"))
                    .unwrap_err()
                    .message
                    .contains("displacement")
            );
        }
    }
    assert_eq!(
        assemble("tbz x0, #0, #4").unwrap().text,
        assemble("tbz w0, #0, #4").unwrap().text
    );
}

#[test]
fn scalar_fp_forms_match_clang() {
    let mut source = String::from(".text\n");
    for p in ["s", "d"] {
        for op in [
            "fmov", "fabs", "fneg", "fsqrt", "frintn", "frintp", "frintm", "frintz",
        ] {
            source.push_str(&format!("{op} {p}0, {p}31\n"));
        }
        for op in ["fadd", "fsub", "fmul", "fdiv", "fmin", "fmax"] {
            source.push_str(&format!("{op} {p}0, {p}30, {p}31\n"));
        }
        for op in ["fmadd", "fmsub", "fnmadd", "fnmsub"] {
            source.push_str(&format!("{op} {p}0, {p}30, {p}31, {p}1\n"));
        }
        for op in ["fcmp", "fcmpe"] {
            source.push_str(&format!("{op} {p}31, {p}30\n{op} {p}31, #0.0\n"));
        }
        for cc in [
            "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le",
            "al", "nv",
        ] {
            source.push_str(&format!("fcsel {p}0, {p}30, {p}31, {cc}\n"));
        }
        for gp in ["w", "x"] {
            for op in ["scvtf", "ucvtf"] {
                source.push_str(&format!("{op} {p}0, {gp}zr\n"));
            }
            for op in ["fcvtzs", "fcvtzu"] {
                source.push_str(&format!("{op} {gp}zr, {p}31\n"));
            }
        }
        for value in ["1.0", "-1.0", "0.125", "-31.0", "1.5"] {
            source.push_str(&format!("fmov {p}0, #{value}\n"));
        }
    }
    source.push_str(
        "fcvt d0, s31\nfcvt s0, d31\nfmov s0, wzr\nfmov d31, xzr\nfmov w0, s31\nfmov xzr, d0\n",
    );
    compare_clang(&source);
}

#[test]
fn invalid_compute_operands_are_diagnostics() {
    for bad in [
        "and x0, x1, #0",
        "mov x0, #--1",
        "movz x0, #--1",
        "orr x0, x1, #-1",
        "and w0, w1, #0x100000000",
        "and x0, x1, #0x123456789",
        "and xzr, x1, #1",
        "ands sp, x1, #1",
        "eor x0, sp, x1",
        "bic x0, x1, #1",
        "and w0, w1, w2, ror #32",
        "madd x0, w1, x2, x3",
        "smulh w0, w1, w2",
        "smull x0, x1, w2",
        "lsl x0, x1, #64",
        "lsr w0, w1, #-1",
        "bfm w0, w1, #32, #0",
        "bfi x0, x1, #63, #2",
        "ubfx x0, x1, #0, #0",
        "sxtw w0, w1",
        "sxth x0, x1",
        "extr x0, w1, x2, #0",
        "adc x0, sp, x1",
        "rev32 w0, w1",
        "ccmp x0, #32, #0, eq",
        "ccmn x0, x1, #16, eq",
        "cset x0, al",
        "csetm x0, nv",
        "csel x0, x1, w2, eq",
        "add sp, sp, x0, lsl #5",
        "add x0, x1, x2, uxtw",
        "b.foo #4",
        "cbz sp, #4",
        "tbz w0, #32, #4",
        "tbnz x0, #64, #4",
        "br w0",
        "blr sp",
        "brk #65536",
        "b.eq Lundefined",
        "cbnz x0, _external",
        "fadd d0, d1, s2",
        "fmov d32, d0",
        "fmov x0, s0",
        "fmov s0, x0",
        "fmov d0, #0.0",
        "fmov d0, #1.1",
        "fmov d0, #inf",
        "fmov q0, q1",
        "fcmp d0, #1.0",
        "fcmp s0, d1",
        "fcvt d0, d1",
        "scvtf d0, sp",
        "fcvtzs sp, d0",
        "fcvtzu w0, h0",
    ] {
        let error = assemble(&format!("nop\n{bad}")).expect_err(bad);
        assert_eq!(error.line, 2, "{bad}: {error}");
    }
}

#[test]
fn all_scalar_fp_immediates_match_clang() {
    let mut source = String::from(".text\n");
    for p in ["s", "d"] {
        for sign in [-1.0, 1.0] {
            for exponent in -3..=4 {
                for fraction in 0..16 {
                    let value = sign * (1.0 + f64::from(fraction) / 16.0) * 2f64.powi(exponent);
                    source.push_str(&format!("fmov {p}31, #{value}\n"));
                }
            }
        }
    }
    compare_clang(&source);
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn arithmetic_and_control_flow_execute_correctly() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = "
.text
.globl _int_math, _high, _rotate, _insert, _select, _carry, _three_equal, _loop
.globl _call, _tail, _fpcalc, _fpselect, _convert, _precision, _bitcast, _popcount
_int_math: madd x2, x0, x1, x0
    sdiv x0, x2, x1
    ret
_high: umulh x0, x0, x1
    ret
_rotate: ror x0, x0, x1
    eor x0, x0, #0x5555555555555555
    ret
_insert: bfi x0, x1, #8, #16
    ret
_select: cmp x0, x1
    csel x0, x0, x1, gt
    ret
_carry: adds x2, x0, x1
    cset x0, hs
    ret
_three_equal: cmp x0, x1
    ccmp x1, x2, #0, eq
    cset x0, eq
    ret
_loop: mov x1, #0
    cbz x0, Ldone
Lsum: add x1, x1, x0
    subs x0, x0, #1
    b.ne Lsum
    tbz x1, #0, Ldone
    mov w0, #1
    ret
Ldone: mov x0, x1
    ret
_call: stp x29, x30, [sp, #-16]!
    blr x0
    ldp x29, x30, [sp], #16
    ret
_tail: br x0
_fpcalc: fmadd d0, d0, d1, d2
    frintm d0, d0
    fneg d1, d0
    fabs d0, d1
    ret
_fpselect: fcmp d0, d1
    fcsel d0, d0, d1, gt
    ret
_convert: fcvtzs x0, d0
    scvtf d0, x0
    ret
_precision: fcvt s0, d0
    fcvt d0, s0
    ret
_popcount: fmov d0, x0
    cnt v0.8b, v0.8b
    addv b0, v0.8b
    fmov w0, s0
    ret
_bitcast: fmov d0, x0
    fmov x0, d0
    ret
";
    let driver = "
#include <stdint.h>
extern long int_math(long, long), select(long, long), loop(long);
extern unsigned long high(unsigned long, unsigned long), rotate(unsigned long, unsigned long);
extern unsigned long insert(unsigned long, unsigned long), carry(unsigned long, unsigned long);
extern long three_equal(long, long, long), call(long (*)(void)), tail(long (*)(void));
extern double fpcalc(double, double, double), fpselect(double, double), convert(double), precision(double);
extern unsigned long bitcast(unsigned long), popcount(unsigned long);
static long answer(void) { return 42; }
int main(void) {
    if (int_math(-7, 3) != -9 || int_math(12, 5) != 14) return 1;
    if (high(UINT64_MAX, UINT64_MAX) != UINT64_MAX - 1) return 2;
    unsigned long x = 0x123456789abcdef0UL;
    if (rotate(x, 8) != (((x >> 8) | (x << 56)) ^ 0x5555555555555555UL)) return 3;
    if (insert(x, 0x5678) != ((x & ~0xffff00UL) | 0x567800UL)) return 4;
    if (select(-7, 2) != 2 || select(3, -4) != 3) return 5;
    if (carry(UINT64_MAX, 1) != 1 || carry(41, 1) != 0) return 6;
    if (three_equal(7, 7, 7) != 1 || three_equal(7, 8, 8) != 0 || three_equal(7, 7, 8) != 0) return 7;
    if (loop(0) != 0 || loop(4) != 10 || loop(5) != 1) return 8;
    if (call(answer) != 42 || tail(answer) != 42) return 9;
    if (fpcalc(2.0, 3.0, 4.75) != 10.0 || fpselect(-2.0, 7.0) != 7.0) return 10;
    if (convert(-2.75) != -2.0 || precision(1.23456789) != (double)(float)1.23456789) return 11;
    if (bitcast(x) != x || bitcast(UINT64_MAX) != UINT64_MAX) return 12;
    if (popcount(0) != 0 || popcount(UINT64_MAX) != 64 || popcount(x) != 32) return 13;
    return 0;
}
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("compute.o");
    let c = temp.0.join("driver.c");
    let executable = temp.0.join("compute");
    std::fs::write(&object, keel_asm::assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang")
        .args(["-std=c11", "-O2"])
        .arg(&c)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}

#[test]
fn simd_popcount_and_reductions_match_clang() {
    compare_clang(
        "cnt v0.8b, v31.8b\ncnt v31.16b, v0.16b\naddv b0, v31.8b\naddv b31, v0.16b\naddv h31, v0.4h\naddv h0, v31.8h\naddv s31, v0.4s\n",
    );
    for invalid in [
        "cnt v0.8b, v1.16b",
        "cnt v0.4h, v1.4h",
        "cnt x0, x1",
        "addv s0, v1.2s",
        "addv h0, v1.8b",
        "addv b32, v1.8b",
        "cnt v32.8b, v0.8b",
    ] {
        assert!(assemble(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn carry_negation_and_supervisor_calls_match_clang() {
    compare_clang(
        "ngc w0,w1\nngcs x30,xzr\nngc xzr,x0\nngcs wzr,wzr\nsvc #0\nsvc #0x80\nsvc #65535",
    );
    for source in ["ngc x0,w1", "ngcs sp,x1", "svc #-1", "svc #65536", "svc"] {
        assert!(assemble(source).is_err(), "{source}");
    }
}

use keel_asm::assemble;
mod common;
use common::compare_clang;

#[test]
fn exclusive_accesses_match_clang() {
    let mut source = String::from("clrex\n");
    for n in 0..16 {
        source.push_str(&format!("clrex #{n}\n"));
    }
    for (suffix, p) in [("b", "w"), ("h", "w"), ("", "w"), ("", "x")] {
        for (t, base) in [("0", "x0"), ("zr", "sp"), ("30", "x2")] {
            for root in ["ldxr", "ldaxr"] {
                source.push_str(&format!("{root}{suffix} {p}{t}, [{base}]\n"));
            }
            for root in ["stxr", "stlxr"] {
                source.push_str(&format!("{root}{suffix} w1, {p}{t}, [{base}, #0]\n"));
            }
        }
    }
    for p in ["w", "x"] {
        for (t, t2, base) in [("0", "30", "x0"), ("zr", "zr", "sp"), ("zr", "2", "x2")] {
            for root in ["ldxp", "ldaxp"] {
                source.push_str(&format!("{root} {p}{t}, {p}{t2}, [{base}]\n"));
            }
            for root in ["stxp", "stlxp"] {
                source.push_str(&format!("{root} w1, {p}{t}, {p}{t2}, [{base}]\n"));
            }
        }
        source.push_str(&format!("stxr wzr, {p}0, [sp]\n"));
    }
    compare_clang(&source);
}

#[test]
fn literal_loads_match_clang() {
    let mut source = String::from(".text\nLback: .quad 42\n");
    for dest in ["w0", "wzr", "x30", "xzr", "s0", "d31", "q0", "q31"] {
        for target in ["Lback", "Lforward", "Lforward+4", "#-1048576", "#1048572"] {
            source.push_str(&format!("ldr {dest}, {target}\n"));
        }
    }
    source.push_str("ldrsw x0, Lback\nldrsw xzr, Lforward\nLforward: .quad 42\n");
    compare_clang(&source);
}

#[test]
fn invalid_exclusive_and_literal_forms_are_diagnosed() {
    for bad in [
        "ldxrb x0, [x1]",
        "stxrh w0, x1, [x2]",
        "stxr x0, x1, [x2]",
        "stxr w0, x0, [x1]",
        "stxr w0, x1, [x0]",
        "stxr wzr, xzr, [sp]",
        "stxp w0, x1, x0, [x2]",
        "ldxp x0, x0, [x1]",
        "ldaxp w0, x1, [x2]",
        "ldxr x0, [x1, #8]",
        "ldxr x0, [x1], #0",
        "stlxr w0, x1, [x2, x3]",
        "ldxr sp, [x0]",
        "ldxp x0, x1, [xzr]",
        "clrex #16",
        "clrex sy",
        "ldr b0, #4",
        "ldr h0, #4",
        "ldrsw w0, #4",
        "ldrsw d0, #4",
        "ldr x0, #2",
        "ldr x0, #-1048580",
        "ldr x0, #1048576",
        "ldr x0, _external",
        "ldr x0, Lmissing",
        "ldr x0, Ldata\n.data\nLdata: .quad 42",
        "ldr x0, =42",
        "ldr x0, Ldata@PAGE\nLdata: .quad 42",
        ".subsections_via_symbols\n_a: ldr x0, Ldata\n_b:\nLdata: .quad 42",
    ] {
        assert!(assemble(bad).is_err(), "{bad}");
    }
    assert!(assemble(".subsections_via_symbols\n_a: ldr x0, Ldata\nret\nLdata: .quad 42").is_ok());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn exclusive_retry_loops_and_literal_loads_execute() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = "
.text
.globl _increment, _pair, _literal
_increment:
Lretry: ldaxr x1, [x0]
    add x1, x1, #1
    stlxr w2, x1, [x0]
    cbnz w2, Lretry
    ret
_pair:
Lpair: ldaxp x1, x2, [x0]
    add x1, x1, #1
    add x2, x2, #2
    stlxp w3, x1, x2, [x0]
    cbnz w3, Lpair
    ret
_literal:
    ldr x0, Lconstant
    ldrsw x1, Lnegative
    add x0, x0, x1
    ret
.p2align 3
Lconstant: .quad 43
Lnegative: .long -1
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("exclusive.o");
    let driver = temp.0.join("driver.c");
    let executable = temp.0.join("exclusive");
    std::fs::write(&object, keel_asm::assemble_object(source).unwrap()).unwrap();
    std::fs::write(
        &driver,
        "
#include <pthread.h>
#include <stdint.h>
extern void increment(uint64_t*), pair(uint64_t*);
extern long literal(void);
static _Alignas(16) uint64_t value, values[2];
static void *worker(void *unused) {
    for (int i = 0; i < 1000; ++i) { increment(&value); pair(values); }
    return 0;
}
int main(void) {
    pthread_t threads[4];
    for (int i = 0; i < 4; ++i) if (pthread_create(&threads[i], 0, worker, 0)) return 1;
    for (int i = 0; i < 4; ++i) if (pthread_join(threads[i], 0)) return 2;
    return value != 4000 || values[0] != 4000 || values[1] != 8000 || literal() != 42;
}",
    )
    .unwrap();
    run(Command::new("clang")
        .args(["-O2", "-pthread"])
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}

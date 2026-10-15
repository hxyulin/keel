use keel_asm::{assemble, assemble_object};
use std::{fmt::Write, process::Command};
mod common;
use common::{Temp, compare_clang, run};
#[test]
fn every_lse_order_width_and_store_alias_matches_clang() {
    let mut source = String::new();
    for root in [
        "ldadd", "ldclr", "ldset", "ldeor", "ldsmin", "ldsmax", "ldumin", "ldumax", "swp", "cas",
    ] {
        for order in ["", "a", "l", "al"] {
            for (width, r) in [("b", "w"), ("h", "w"), ("", "w"), ("", "x")] {
                for (s, t, n) in [
                    ("0", "1", "x2"),
                    ("30", "zr", "sp"),
                    ("zr", "30", "x0"),
                    ("1", "1", "x1"),
                ] {
                    writeln!(source, "{root}{order}{width} {r}{s},{r}{t},[{n}]").unwrap();
                }
            }
        }
    }
    for root in [
        "stadd", "stclr", "stset", "steor", "stsmin", "stsmax", "stumin", "stumax",
    ] {
        for order in ["", "l"] {
            for (width, r) in [("b", "w"), ("h", "w"), ("", "w"), ("", "x")] {
                writeln!(source, "{root}{order}{width} {r}30,[sp]").unwrap();
            }
        }
    }
    assert_eq!(
        assemble("ldadd x0,x1,[x2,#0]\nstadd w0,[sp,#0]")
            .unwrap()
            .text,
        assemble("ldadd x0,x1,[x2]\nstadd w0,[sp]").unwrap().text
    );
    compare_clang(&source);
}
#[test]
fn barriers_match_clang() {
    let mut source = String::from("isb\nisb sy\n");
    for op in ["dmb", "dsb"] {
        for option in [
            "oshld", "oshst", "osh", "nshld", "nshst", "nsh", "ishld", "ishst", "ish", "ld", "st",
            "sy",
        ] {
            writeln!(source, "{op} {option}").unwrap();
        }
    }
    for op in ["dmb", "dsb", "isb"] {
        for n in 0..16 {
            writeln!(source, "{op} #{n}").unwrap();
        }
    }
    compare_clang(&source);
}
#[test]
fn invalid_atomic_operands_are_diagnosed() {
    for source in [
        "ldadd x0,w1,[x2]",
        "casb x0,x1,[x2]",
        "swp sp,x1,[x2]",
        "ldadd x0,x1,[w2]",
        "cas x0,x1,[xzr]",
        "ldadd x0,x1,[x2,#8]",
        "ldadd x0,x1,[x2],#0",
        "swp x0,x1,[x2,#0]!",
        "swp x0,x1,[x2,x3]",
        "ldaddla x0,x1,[x2]",
        "staddal x0,[x1]",
        "cas x0,[x1]",
        "dmb",
        "dmb ishfoo",
        "dmb #16",
        "isb ish",
        "dsb #-1",
    ] {
        let error = assemble(source).unwrap_err();
        assert_eq!(error.line, 1, "{source}");
    }
}
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn generated_atomics_execute_with_threads_and_cas_locks() {
    use keel_codegen::{
        Pools,
        aarch64::{REGISTERS, compile_with},
    };
    let tiny = Pools {
        caller_saved: &[9, 10],
        callee_saved: &[19, 20],
        float_caller_saved: &[19, 20],
        float_callee_saved: &[8, 9],
    };
    let source = include_str!("../../keel-codegen/tests/programs/atomics.keel");
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let temp = Temp::new();
    let obj = temp.0.join("atomic.o");
    let exe = temp.0.join("run");
    for pools in [REGISTERS, tiny] {
        let source = compile_with(&module, &pools).unwrap();
        std::fs::write(&obj, assemble_object(&source).unwrap()).unwrap();
        run(Command::new("clang")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../keel-codegen/tests/programs/atomics.c"),
            )
            .arg(&obj)
            .arg("-o")
            .arg(&exe));
        assert!(common::execute(&exe).success());
    }
}

use keel_asm::assemble;
use std::fmt::Write;
mod common;
#[test]
fn carry_negation_and_supervisor_calls_match_clang() {
    common::compare_clang(
        "ngc w0,w1\nngcs x30,xzr\nngc xzr,x0\nngcs wzr,wzr\nsvc #0\nsvc #0x80\nsvc #65535",
    );
    for source in ["ngc x0,w1", "ngcs sp,x1", "svc #-1", "svc #65536", "svc"] {
        assert!(assemble(source).is_err(), "{source}");
    }
}
#[test]
fn every_movi_modified_immediate_matches_clang() {
    let mut source = String::new();
    for imm in 0..256u64 {
        for dest in ["v0.8b", "v31.16b"] {
            writeln!(source, "movi {dest},#{imm}").unwrap();
        }
        for dest in ["v0.4h", "v31.8h"] {
            for shift in [0, 8] {
                writeln!(source, "movi {dest},#{imm},lsl #{shift}").unwrap();
            }
        }
        for dest in ["v0.2s", "v31.4s"] {
            for shift in [0, 8, 16, 24] {
                writeln!(source, "movi {dest},#{imm},lsl #{shift}").unwrap();
            }
            for shift in [8, 16] {
                writeln!(source, "movi {dest},#{imm},msl #{shift}").unwrap();
            }
        }
        let mut mask = 0u64;
        for bit in 0..8 {
            if imm & (1 << bit) != 0 {
                mask |= 255 << (bit * 8);
            }
        }
        for dest in ["d31", "v0.2d"] {
            writeln!(source, "movi {dest},#0x{mask:x}").unwrap();
        }
    }
    common::compare_clang(&source);
    for source in [
        "movi d0,#1",
        "movi v0.1d,#0",
        "movi v0.16b,#256",
        "movi v0.8h,#1,lsl #16",
        "movi v0.4s,#1,msl #0",
        "movi v0.4s,#1,lsl #32",
        "movi v0.2d,#0,lsl #8",
        "movi v32.8b,#0",
        "movi v0.8b,#symbol",
    ] {
        assert!(assemble(source).is_err(), "{source}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn i128_and_inline_assembly_fixtures_execute() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let temp = common::Temp::new();
    let obj = temp.0.join("program.o");
    let exe = temp.0.join("run");
    for name in ["i128", "inline_asm"] {
        let source = std::fs::read_to_string(dir.join(format!("{name}.keel"))).unwrap();
        let module = keel_ir::parse(&source).unwrap();
        let source =
            keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
        std::fs::write(&obj, keel_asm::assemble_object(&source).unwrap()).unwrap();
        common::run(
            std::process::Command::new("clang")
                .arg(dir.join(format!("{name}.c")))
                .arg(&obj)
                .arg("-lm")
                .arg("-o")
                .arg(&exe),
        );
        assert!(common::execute(&exe).success(), "{name}");
    }
}

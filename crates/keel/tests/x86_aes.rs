//! AES is explicitly optional at every psABI level on the public CLI.
use std::{fs, process::Command};
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn aes_cli_level_modifiers_and_c_objects() {
    let dir = std::env::temp_dir().join(format!("keel-aes-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("aes.s");
    let input = input.to_str().unwrap();
    let object = dir.join("aes.o");
    let object = object.to_str().unwrap();
    for op in [
        "aesenc",
        "aesenclast",
        "aesdec",
        "aesdeclast",
        "aesimc",
        "aeskeygenassist",
    ] {
        fs::write(
            input,
            format!(
                "{op} xmm8,xmm15{}\n",
                if op == "aeskeygenassist" { ",255" } else { "" }
            ),
        )
        .unwrap();
        for level in ["v1", "v2", "v3"] {
            let base = [
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                level,
                input,
                "-o",
                object,
            ];
            let out = run(&base);
            assert!(!out.status.success(), "accepted {op} under {level}");
            let mut on = base.to_vec();
            on.extend(["--target-features", "+aes"]);
            let out = run(&on);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    for aes in [false, true] {
        for pclmul in [false, true] {
            let features = format!(
                "{}aes,{}pclmulqdq",
                if aes { "+" } else { "-" },
                if pclmul { "+" } else { "-" }
            );
            for (line, allowed) in [
                ("aesenc xmm0,xmm1", aes),
                ("pclmulqdq xmm0,xmm1,17", pclmul),
            ] {
                fs::write(input, line).unwrap();
                assert_eq!(
                    run(&[
                        "asm",
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "--cpu-level",
                        "v1",
                        "--target-features",
                        &features,
                        input,
                        "-o",
                        object
                    ])
                    .status
                    .success(),
                    allowed
                );
            }
        }
    }
    fs::write(input, "vaddps xmm0,xmm1,xmm2\n").unwrap();
    assert!(
        !run(&[
            "asm",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--cpu-level",
            "v1",
            "--target-features",
            "+aes",
            input,
            "-o",
            object
        ])
        .status
        .success()
    );
    let c = dir.join("aes.c");
    let c = c.to_str().unwrap();
    fs::write(c,"#include <wmmintrin.h>\n__m128i enc(__m128i a,__m128i b){return _mm_aesenc_si128(a,b);}\n__m128i kg(__m128i a){return _mm_aeskeygenassist_si128(a,27);}\n").unwrap();
    for level in ["v1", "v2", "v3"] {
        for optimized in [false, true] {
            let mut args = vec![
                "cc",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                level,
                c,
                "-o",
                object,
                "--emit-object",
            ];
            if optimized {
                args.push("-O");
            }
            assert!(!run(&args).status.success());
            args.extend(["--target-features", "+aes"]);
            let out = run(&args);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let direct = fs::read(object).unwrap();
            args.retain(|s| *s != "--emit-object");
            let out = run(&args);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let reassembled = keel_asm::x86::assemble_object_with_features(
                &fs::read_to_string(object).unwrap(),
                keel_codegen::x86_features::Features::for_level(
                    keel_codegen::x86_features::CpuLevel::parse(level).unwrap(),
                )
                .with_features("+aes")
                .unwrap(),
            )
            .unwrap();
            assert_eq!(direct, reassembled);
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

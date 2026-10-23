//! Exercise strict feature selection through the public CLI, including C input.
use std::{fs, process::Command};

fn keel() -> Command {
    Command::new(env!("CARGO_BIN_EXE_keel"))
}
fn success(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn explicit_cpu_levels_c_objects_and_assembly_obey_the_same_feature_contract() {
    let dir = std::env::temp_dir().join(format!("keel-c-v1-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("input.c");
    fs::write(&source, "int count(unsigned long x){return __builtin_popcountl(x);}\ndouble scalar(double x,double y){return x*y+x/y;}\n").unwrap();
    for level in ["v1", "v2", "v3"] {
        for optimized in [false, true] {
            let text = dir.join("output.s");
            let direct = dir.join("direct.o");
            let parsed = dir.join("parsed.o");
            for object in [false, true] {
                let mut cmd = keel();
                cmd.args([
                    "cc",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--cpu-level",
                    level,
                ]);
                if optimized {
                    cmd.arg("-O");
                }
                if object {
                    cmd.arg("--emit-object");
                }
                success(
                    cmd.arg(&source)
                        .arg("-o")
                        .arg(if object { &direct } else { &text }),
                );
            }
            let assembly = fs::read_to_string(&text).unwrap();
            assert_eq!(assembly.contains("\tpopcnt\t"), level != "v1");
            success(
                keel()
                    .args([
                        "asm",
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "--cpu-level",
                        level,
                    ])
                    .arg(&text)
                    .arg("-o")
                    .arg(&parsed),
            );
            assert_eq!(fs::read(&direct).unwrap(), fs::read(&parsed).unwrap());
        }
    }
    let assembly = dir.join("optional.s");
    for (instruction, minimum, enable) in [
        ("popcnt rax,rax", "v2", "popcnt"),
        ("roundsd xmm0,xmm0,8", "v2", "sse41"),
        ("haddpd xmm0,xmm1", "v2", "sse3"),
        ("pshufb xmm0,xmm1", "v2", "ssse3"),
        ("pcmpgtq xmm0,xmm1", "v2", "sse42"),
        ("crc32 eax,ecx", "v2", "sse42"),
        ("lahf", "v2", "lahf_sahf"),
        ("sahf", "v2", "lahf_sahf"),
        ("cmpxchg16b [rax]", "v2", "cx16"),
        ("vaddps xmm0,xmm1,xmm2", "v3", "avx"),
        ("vpaddd ymm0,ymm1,ymm2", "v3", "avx,avx2"),
        ("andn rax,rcx,rdx", "v3", "bmi1"),
        ("pext rax,rcx,rdx", "v3", "bmi2"),
        ("vfmadd132ps xmm0,xmm1,xmm2", "v3", "avx,fma"),
        ("vcvtph2ps xmm0,xmm1", "v3", "avx,f16c"),
        ("tzcnt rax,rcx", "v3", "bmi1"),
        ("lzcnt rax,rcx", "v3", "lzcnt"),
        ("movbe rax,qword ptr [rcx]", "v3", "movbe"),
        ("clflush [rax]", "optional", "clflush"),
        ("pclmulqdq xmm0,xmm1,255", "optional", "pclmulqdq"),
    ] {
        fs::write(&assembly, format!(".text\n{instruction}\n")).unwrap();
        for level in ["v1", "v2", "v3"] {
            let mut cmd = keel();
            cmd.args([
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                level,
            ])
            .arg(&assembly)
            .arg("-o")
            .arg(dir.join("instruction.o"));
            let output = cmd.output().unwrap();
            let accepted = minimum != "optional" && level >= minimum;
            assert_eq!(
                output.status.success(),
                accepted,
                "{level}: {instruction}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if !accepted {
                assert!(String::from_utf8_lossy(&output.stderr).contains("requires"));
                // Explicit modifiers change the contract, including CLFSH at every level.
                success(cmd.arg("--target-features").arg(enable));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn strict_v3_wide_ir_and_c_use_public_compile_and_object_paths() {
    let dir = std::env::temp_dir().join(format!("keel-v3-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (extension, command, source) in [
        (
            "keel",
            "compile",
            "func @wide(f32x8,f32x8)->f32x8 {\nblock0(v0:f32x8,v1:f32x8):\n v2 = vfmul v0,v1\n v3 = vfadd v2,v0\n return v3\n}\n",
        ),
        (
            "c",
            "cc",
            "#include <immintrin.h>\n__m256 wide(__m256 a,__m256 b){return _mm256_add_ps(_mm256_mul_ps(a,b),a);}\n",
        ),
    ] {
        let input = dir.join(format!("wide.{extension}"));
        fs::write(&input, source).unwrap();
        for optimized in [false, true] {
            let text = dir.join("wide.s");
            let direct = dir.join("direct.o");
            let parsed = dir.join("parsed.o");
            for object in [false, true] {
                let mut cmd = keel();
                cmd.args([
                    command,
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--cpu-level",
                    "v3",
                ]);
                if optimized {
                    cmd.arg("-O");
                }
                if object {
                    cmd.arg("--emit-object");
                }
                success(
                    cmd.arg(&input)
                        .arg("-o")
                        .arg(if object { &direct } else { &text }),
                );
            }
            let assembly = fs::read_to_string(&text).unwrap();
            assert!(assembly.contains("ymm"), "{assembly}");
            assert!(
                !assembly.contains("vfmadd"),
                "ordinary multiply/add must not contract: {assembly}"
            );
            success(
                keel()
                    .args([
                        "asm",
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "--cpu-level",
                        "v3",
                    ])
                    .arg(&text)
                    .arg("-o")
                    .arg(&parsed),
            );
            assert_eq!(fs::read(&direct).unwrap(), fs::read(&parsed).unwrap());
        }
        for level in ["v1", "v2"] {
            let output = keel()
                .args([
                    command,
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--cpu-level",
                    level,
                ])
                .arg(&input)
                .arg("-o")
                .arg(dir.join("disabled.s"))
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("AVX")
                    || String::from_utf8_lossy(&output.stderr).contains("256-bit SIMD"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let asm = dir.join("post-v3.s");
    for instruction in [
        "vaddps zmm0,zmm1,zmm2",
        "vpternlogd ymm0,ymm1,ymm2,0",
        "clflush [rax]",
    ] {
        fs::write(&asm, format!(".text\n{instruction}\n")).unwrap();
        assert!(
            !keel()
                .args([
                    "asm",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--cpu-level",
                    "v3"
                ])
                .arg(&asm)
                .arg("-o")
                .arg(dir.join("forbidden.o"))
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn optional_pclmul_ir_c_cli_modifiers_and_object_paths() {
    let dir = std::env::temp_dir().join(format!("keel-pclmul-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (command, extension, source) in [
        (
            "compile",
            "keel",
            "func @f(i64x2,i64x2)->i64x2 {\nblock0(v0:i64x2,v1:i64x2):\n v2 = iconst.i32 255\n v3 = intrinsic x86_pclmulqdq128(v0,v1,v2)\n return v3\n}\n",
        ),
        (
            "cc",
            "c",
            "#include <wmmintrin.h>\n__m128i f(__m128i a,__m128i b){return _mm_clmulepi64_si128(a,b,255);}\n",
        ),
    ] {
        let input = dir.join(format!("input.{extension}"));
        fs::write(&input, source).unwrap();
        for level in ["v1", "v2", "v3"] {
            for optimized in [false, true] {
                for object in [false, true] {
                    for (features, accepted) in [
                        (None, false),
                        (Some("+pclmulqdq"), true),
                        (Some("pclmul,-pclmulqdq"), false),
                    ] {
                        let output_path = dir.join(if object { "out.o" } else { "out.s" });
                        let mut cmd = keel();
                        cmd.args([
                            command,
                            "--target",
                            "x86_64-unknown-linux-gnu",
                            "--cpu-level",
                            level,
                        ]);
                        if optimized {
                            cmd.arg("-O");
                        }
                        if object {
                            cmd.arg("--emit-object");
                        }
                        if let Some(features) = features {
                            cmd.args(["--target-features", features]);
                        }
                        let output = cmd
                            .arg(&input)
                            .arg("-o")
                            .arg(&output_path)
                            .output()
                            .unwrap();
                        assert_eq!(
                            output.status.success(),
                            accepted,
                            "{cmd:?}: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        if !accepted {
                            assert!(String::from_utf8_lossy(&output.stderr).contains("PCLMULQDQ"));
                        }
                    }
                }
            }
        }
    }
}

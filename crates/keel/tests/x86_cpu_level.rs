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

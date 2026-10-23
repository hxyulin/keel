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
fn explicit_v1_c_objects_and_assembly_obey_the_same_feature_contract() {
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
    for (instruction, minimum) in [
        ("popcnt rax,rax", "v2"),
        ("roundsd xmm0,xmm0,8", "v2"),
        ("haddpd xmm0,xmm1", "v2"),
        ("pshufb xmm0,xmm1", "v2"),
        ("crc32 eax,ecx", "v2"),
        ("lahf", "v2"),
        ("cmpxchg16b [rax]", "v2"),
        ("vaddps ymm0,ymm1,ymm2", "v3"),
        ("tzcnt rax,rcx", "v3"),
        ("lzcnt rax,rcx", "v3"),
        ("movbe rax,qword ptr [rcx]", "v3"),
    ] {
        fs::write(&assembly, format!(".text\n{instruction}\n")).unwrap();
        let output = keel()
            .args([
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                "v1",
            ])
            .arg(&assembly)
            .arg("-o")
            .arg(dir.join("reject.o"))
            .output()
            .unwrap();
        assert!(!output.status.success(), "v1 accepted {instruction}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("requires"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        success(
            keel()
                .args([
                    "asm",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--cpu-level",
                    minimum,
                ])
                .arg(&assembly)
                .arg("-o")
                .arg(dir.join("accept.o")),
        );
    }
    // CLFSH is optional even at v3; it must never be smuggled into v1.
    fs::write(&assembly, ".text\nclflush [rax]\n").unwrap();
    assert!(
        !keel()
            .args([
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                "v1"
            ])
            .arg(&assembly)
            .arg("-o")
            .arg(dir.join("reject.o"))
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_dir_all(dir).unwrap();
}

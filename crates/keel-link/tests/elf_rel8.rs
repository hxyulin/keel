use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
fn obj(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "rel8.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
#[test]
fn external_loop_and_count_zero_branches_relocate_exactly() {
    for op in ["loop", "loope", "loopne", "jrcxz", "jecxz"] {
        for reverse in [false, true] {
            let source = obj(&format!(
                ".text\n.globl _start\n_start:\n{op} target\nret\n"
            ));
            let target = obj(".text\n.globl target\ntarget: ret\n");
            let objects = if reverse {
                vec![target, source]
            } else {
                vec![source, target]
            };
            let executable = link(&objects).unwrap();
            let file = object::File::parse(executable.bytes.as_slice()).unwrap();
            let address = |name| {
                file.symbols()
                    .find(|s| s.name().unwrap() == name)
                    .unwrap()
                    .address()
            };
            let section = file
                .sections()
                .find(|s| {
                    address("_start") >= s.address() && address("_start") < s.address() + s.size()
                })
                .unwrap();
            let code = section.data().unwrap();
            let start = (address("_start") - section.address()) as usize;
            let prefix = usize::from(op == "jecxz");
            let field = start + prefix + 1;
            let expected =
                address("target") as i128 - (section.address() + field as u64 + 1) as i128;
            assert_eq!(
                code[field] as i8 as i128, expected,
                "{op}, reverse={reverse}"
            );
        }
    }
}
#[test]
fn external_rel8_overflow_is_an_error_in_both_directions() {
    for reverse in [false, true] {
        let source = obj(".text\n.globl _start\n_start: loop target\n.space 512\n");
        let target = obj(if reverse {
            ".text\n.globl target\ntarget: ret\n.space 512\n"
        } else {
            ".text\n.space 512\n.globl target\ntarget: ret\n"
        });
        let objects = if reverse {
            vec![target, source]
        } else {
            vec![source, target]
        };
        let error = link(&objects).unwrap_err();
        assert!(error.contains("relocation 15 overflow"), "{error}");
    }
}

#[test]
fn rel8_signed_boundary_values_are_preserved_without_wrapping() {
    for (reverse, pad, expected) in [(false, 127, 127i8), (true, 126, -128i8)] {
        let source = obj(&format!(
            ".text\n.globl _start\n_start: loop target\n{}",
            if reverse {
                String::new()
            } else {
                format!(".space {pad}\n")
            }
        ));
        let target = obj(&format!(
            ".text\n.globl target\ntarget:\n{}",
            if reverse {
                format!(".space {pad}\n")
            } else {
                "ret\n".into()
            }
        ));
        let objects = if reverse {
            vec![target, source]
        } else {
            vec![source, target]
        };
        let executable = link(&objects).unwrap();
        let file = object::File::parse(executable.bytes.as_slice()).unwrap();
        let start = file
            .symbols()
            .find(|s| s.name().unwrap() == "_start")
            .unwrap()
            .address();
        let section = file
            .sections()
            .find(|s| start >= s.address() && start < s.address() + s.size())
            .unwrap();
        assert_eq!(
            section.data().unwrap()[(start - section.address()) as usize + 1] as i8,
            expected
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn external_rel8_branches_execute_across_objects_in_both_orders() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("keel-native-rel8-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for op in ["loop", "loope", "loopne", "jrcxz", "jecxz"] {
        for taken in [false, true] {
            let count: u64 = match op {
                "jecxz" if taken => 1 << 32,
                "jrcxz" if taken => 0,
                "jecxz" | "jrcxz" => 1,
                _ => {
                    if taken {
                        2
                    } else {
                        1
                    }
                }
            };
            let flags = if op == "loopne" {
                "mov eax,1\ntest eax,eax"
            } else {
                "xor eax,eax"
            };
            let source = obj(&format!(
                ".text\n.globl _start\n_start:\nmov rcx,{count}\n{flags}\n{op} target\nmov edi,7\nmov eax,60\nsyscall\n"
            ));
            let target = obj(".text\n.globl target\ntarget:\nxor edi,edi\nmov eax,60\nsyscall\n");
            for reverse in [false, true] {
                let objects = if reverse {
                    vec![target.clone(), source.clone()]
                } else {
                    vec![source.clone(), target.clone()]
                };
                let executable = link(&objects).unwrap();
                let path = dir.join(format!("{op}-{taken}-{reverse}"));
                std::fs::write(&path, executable.bytes).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
                let status = std::process::Command::new(&path).status().unwrap();
                assert_eq!(
                    status.code(),
                    Some(if taken { 0 } else { 7 }),
                    "{op}, taken={taken}, reverse={reverse}"
                );
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

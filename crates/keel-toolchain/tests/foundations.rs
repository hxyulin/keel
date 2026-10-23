use keel_ir::{InstData, Target, Type};
use keel_toolchain::{artifact, instrument};

const ACCESS: &str = "func @access(ptr) -> i32 cc(c) {\nblock0(v0: ptr):\n v1 = iconst.i32 99\n store v1, v0\n v2 = load.i32 v0\n return v2\n}\n";

#[test]
fn artifact_rejects_incompatible_or_damaged_inputs() {
    let module = keel_ir::parse(ACCESS).unwrap();
    for target in Target::ALL {
        let bytes = artifact::encode(&module, target).unwrap();
        assert_eq!(
            artifact::decode(&bytes, target).unwrap().to_string(),
            module.to_string()
        );
        let other = Target::ALL.into_iter().find(|t| *t != target).unwrap();
        assert!(artifact::decode(&bytes, other).is_err());
        assert!(artifact::decode(&bytes[..bytes.len() - 1], target).is_err());
        let mut trailing = bytes.clone();
        trailing.push(b' ');
        assert!(artifact::decode(&trailing, target).is_err());
        let mut unknown_version = bytes;
        unknown_version[13] = b'9';
        assert!(artifact::decode(&unknown_version, target).is_err());
    }
    let invalid = ACCESS.replace("return v2", "return v0");
    let invalid = keel_ir::parse(&invalid).unwrap();
    assert!(artifact::encode(&invalid, Target::Aarch64AppleDarwin).is_err());
}

#[test]
fn instrumentation_preserves_accesses_and_refuses_second_pass() {
    let mut module = keel_ir::parse(ACCESS).unwrap();
    let report = instrument::insert_memory_checks(&mut module).unwrap();
    assert_eq!(
        report,
        instrument::Report {
            callbacks: 2,
            functions: 1
        }
    );
    keel_ir::verify_module(&module).unwrap();
    let func = &module.functions[0];
    let insts = func.block_insts(func.blocks().next().unwrap());
    let accesses: Vec<_> = insts
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            matches!(
                func.inst_data(**i),
                InstData::Load { .. } | InstData::Store { .. }
            )
        })
        .collect();
    assert_eq!(accesses.len(), 2);
    for (index, _) in accesses {
        assert!(matches!(
            func.inst_data(insts[index - 1]),
            InstData::Call { .. }
        ));
    }
    // Instrumented units can travel to the eventual LTO consumer intact.
    for target in Target::ALL {
        let encoded = artifact::encode(&module, target).unwrap();
        artifact::decode(&encoded, target).unwrap();
    }
    let before = module.to_string();
    assert!(instrument::insert_memory_checks(&mut module).is_err());
    assert_eq!(before, module.to_string());
}

#[test]
fn pointer_atomic_and_bulk_memory_ranges() {
    let source = "func @ranges(ptr, ptr, i64) cc(c) {\nblock0(v0: ptr, v1: ptr, v2: i64):\n v3 = load.ptr volatile v0\n store volatile v3, v1\n v4 = atomic_load.i32 acquire v0\n atomic_store release v4, v1\n v5 = atomic_rmw.add seq_cst v0, v4\n v6 = atomic_cas seq_cst v0, v4, v5\n intrinsic memcpy(v0, v1, v2)\n intrinsic memmove(v0, v1, v2)\n v7 = iconst.i8 0\n intrinsic memset(v0, v7, v2)\n v8 = intrinsic memcmp(v0, v1, v2)\n return\n}\n";
    let mut module = keel_ir::parse(source).unwrap();
    let report = instrument::insert_memory_checks(&mut module).unwrap();
    assert_eq!(report.callbacks, 13);
    keel_ir::verify_module(&module).unwrap();
    let func = &module.functions[0];
    let mut observed = Vec::new();
    for block in func.blocks() {
        for &inst in func.block_insts(block) {
            if let InstData::Call { args, .. } = func.inst_data(inst) {
                let value = |v| match func.value_def(v) {
                    keel_ir::ValueDef::Result(i, _) => match func.inst_data(i) {
                        InstData::Iconst { imm, .. } => Some(*imm),
                        _ => None,
                    },
                    _ => None,
                };
                observed.push((value(args[1]), value(args[2]).unwrap()));
                assert_eq!(func.value_type(args[0]), Type::Ptr);
            }
        }
    }
    assert_eq!(
        &observed[..6],
        &[
            (Some(8), 9),
            (Some(8), 10),
            (Some(4), 5),
            (Some(4), 6),
            (Some(4), 7),
            (Some(4), 7)
        ]
    );
    assert_eq!(
        &observed[6..],
        &[
            (None, 2),
            (None, 1),
            (None, 2),
            (None, 1),
            (None, 2),
            (None, 1),
            (None, 1)
        ]
    );
}

#[test]
fn unsupported_access_is_transactional() {
    let source = format!(
        "{ACCESS}\nfunc @variadic(ptr, ...) cc(c) {{\nblock0(v0: ptr):\n intrinsic va_start(v0)\n return\n}}\n"
    );
    let mut module = keel_ir::parse(&source).unwrap();
    let before = module.to_string();
    assert!(instrument::insert_memory_checks(&mut module).is_err());
    assert_eq!(before, module.to_string());
}

#[test]
fn native_callback_runtime_stops_before_invalid_access() {
    use std::{fs, process::Command};
    let Some(target) = Target::host() else {
        return;
    };
    let mut module = keel_ir::parse(ACCESS).unwrap();
    instrument::insert_memory_checks(&mut module).unwrap();
    // Both backends must accept the callback ABI even on a single native host.
    let arm = keel_codegen::aarch64::compile(&module).unwrap();
    let x86 = keel_codegen::x86_64::compile(&module).unwrap();
    let asm = match target {
        Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => arm,
        Target::X86_64LinuxGnu => x86,
    };
    let dir = std::env::temp_dir().join(format!(
        "keel-instrument-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("access.s"), asm).unwrap();
    fs::write(
        dir.join("runtime.c"),
        r#"
#include <stdint.h>
#include <stdlib.h>
static int area[2];
static unsigned events;
extern int access(int *);
void __keel_check_memory_v1(void *p, uint64_t bytes, uint32_t flags) {
    if (p != area) exit(events == 2 ? 86 : 87);
    if (bytes != 4 || flags != (events == 0 ? 2u : 1u)) exit(88);
    ++events;
}
int main(void) {
    if (access(area) != 99 || area[0] != 99 || events != 2) return 89;
    access(area + 1);
    return 90;
}
"#,
    )
    .unwrap();
    let output = Command::new("clang")
        .arg(dir.join("access.s"))
        .arg(dir.join("runtime.c"))
        .arg("-o")
        .arg(dir.join("check"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = Command::new(dir.join("check")).status().unwrap();
    assert_eq!(status.code(), Some(86));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn artifact_v4_preserves_ordinary_weak_binding_and_rejects_v1() {
    let source = "decl @optional() weak\ndecl data weak @missing\ndata @d = weak align 4 { i32 0 }\nfunc @f() weak {\nblock0:\n return\n}\n";
    let m = keel_ir::parse(source).unwrap();
    for target in Target::ALL {
        let bytes = artifact::encode(&m, target).unwrap();
        assert!(bytes.starts_with(b"KEEL-IR-UNIT 4\n"));
        let decoded = artifact::decode(&bytes, target).unwrap();
        assert_eq!(decoded.to_string(), m.to_string());
        let mut v1 = bytes;
        v1[13] = b'1';
        assert!(artifact::decode(&v1, target).is_err());
    }
}

#[test]
fn artifact_v4_preserves_lifecycle_and_compiler_used_roots() {
    let m = keel_ir::parse(
        "func @callback() cc(c) constructor destructor used {\nblock0:\n return\n}\n",
    )
    .unwrap();
    for target in Target::ALL {
        let bytes = artifact::encode(&m, target).unwrap();
        let recovered = artifact::decode(&bytes, target).unwrap();
        assert_eq!(recovered.to_string(), m.to_string());
        assert!(recovered.functions[0].attrs.constructor);
        assert!(recovered.functions[0].attrs.destructor);
        assert!(recovered.functions[0].attrs.used);
        let mut old = bytes;
        old[13] = b'2';
        assert!(artifact::decode(&old, target).is_err());
    }
}

#[test]
fn artifact_v4_preserves_sections_and_retain_and_rejects_all_older_versions() {
    for target in Target::ALL {
        let (data, code) = if target == Target::Aarch64AppleDarwin {
            ("__DATA,__custom", "__TEXT,__custom")
        } else {
            (".custom_data", ".custom_code")
        };
        let text = format!(
            "data @x = retain section \"{data}\" align 4 {{ i32 7 }}\nfunc @f() retain section \"{code}\" {{\nblock0:\n return\n}}\n"
        );
        let m = keel_ir::parse(&text).unwrap();
        let bytes = artifact::encode(&m, target).unwrap();
        let recovered = artifact::decode(&bytes, target).unwrap();
        assert_eq!(recovered.to_string(), m.to_string());
        assert!(recovered.data[0].retain);
        assert!(recovered.functions[0].attrs.retain);
        for version in b'1'..=b'3' {
            let mut old = bytes.clone();
            old[13] = version;
            assert!(artifact::decode(&old, target).is_err());
        }
    }
}

#[test]
fn reserved_gnu_metadata_cannot_enter_artifacts_or_native_codegen() {
    let target = Target::X86_64LinuxGnu;
    for name in [".gnu_debuglink", ".gnu_debugaltlink", ".gnu_debugdata"] {
        let ir = format!("data @x = retain section \"{name}\" align 4 {{ i32 1 }}\n");
        let module = keel_ir::parse(&ir).unwrap();
        // A well-formed envelope from an external producer still needs the
        // target's placement checks on decode, independently of encode.
        let envelope = format!(
            "KEEL-IR-UNIT 4\ntarget {}\nbytes {}\n\n{ir}",
            target.triple(),
            ir.len()
        );
        for error in [
            artifact::encode(&module, target).err().unwrap().to_string(),
            artifact::decode(envelope.as_bytes(), target)
                .err()
                .unwrap()
                .to_string(),
            keel_codegen::compile(&module, target)
                .err()
                .unwrap()
                .to_string(),
            keel_codegen::compile_program(&module, target)
                .err()
                .unwrap()
                .to_string(),
        ] {
            assert!(error.contains("special/TLS/COMDAT"), "{error}");
        }
    }
}

#[test]
fn tls_sections_retain_artifact_roundtrip_and_target_validation() {
    for target in Target::ALL {
        let section = if target != Target::Aarch64AppleDarwin {
            ".tls_custom"
        } else {
            "__DATA,__tls_custom"
        };
        let text =
            format!("data @x = thread_local retain section \"{section}\" align 32 {{ i32 7 }}\n");
        let m = keel_ir::parse(&text).unwrap();
        let bytes = artifact::encode(&m, target).unwrap();
        let recovered = artifact::decode(&bytes, target).unwrap();
        assert_eq!(m.to_string(), recovered.to_string());
        assert!(recovered.data[0].thread_local && recovered.data[0].retain);
        let wrong = if target != Target::Aarch64AppleDarwin {
            ".data"
        } else {
            "__DATA,__thread_vars"
        };
        let ir = text.replace(section, wrong);
        let m = keel_ir::parse(&ir).unwrap();
        assert!(artifact::encode(&m, target).is_err());
        let envelope = format!(
            "KEEL-IR-UNIT 4\ntarget {}\nbytes {}\n\n{ir}",
            target.triple(),
            ir.len()
        );
        assert!(artifact::decode(envelope.as_bytes(), target).is_err());
    }
}

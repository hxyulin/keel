use super::{link, link_with, object, value, word};
use keel_link::elf;
use object::{Object, ObjectSection, ObjectSymbol};

fn fixture(kinds: &[u32], arm: bool, defined: bool) -> Vec<u8> {
    let mut bytes = object(&format!(
        ".text\n.globl _start\n_start: ret\n.type tls,@tls_object\n.globl tls\n{}\n.data\n.globl fields\nfields:\n{}",
        if defined {
            ".section .tdata,\"awT\",@progbits\n.balign 32\ntls: .quad 73\n"
        } else {
            ""
        },
        ".quad tls\n".repeat(kinds.len())
    ));
    let f = object::File::parse(bytes.as_slice()).unwrap();
    let at = f
        .section_by_name(".rela.data")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let data = f.section_by_name(".data").unwrap().file_range().unwrap().0 as usize;
    for (i, &kind) in kinds.iter().enumerate() {
        bytes[at + i * 24 + 8..at + i * 24 + 12].copy_from_slice(&kind.to_le_bytes());
        if arm {
            let instruction: u32 = match kind {
                512 | 517 | 561 => 0x10000000,
                513 | 518 | 541 | 562 => 0x90000000,
                514 | 519 | 564 | 568 => 0x91000000,
                522 | 543 | 560 => 0x58000000,
                542 | 563 | 567 => 0xf9400001,
                569 => 0xd63f0020,
                _ => panic!("fixture instruction for {kind}"),
            };
            bytes[data + i * 8..data + i * 8 + 4].copy_from_slice(&instruction.to_le_bytes());
        } else if kind == 35 {
            bytes[data + i * 8..data + i * 8 + 2].copy_from_slice(&[0xff, 0x10]);
        }
    }
    if arm {
        bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
    }
    bytes
}

fn shared(arm: bool) -> Vec<u8> {
    let mut input = object(
        ".text\n.globl _start\n_start: ret\n.globl run\n.type run,@function\nrun: .long 0xd65f03c0\n.section .tdata,\"awT\",@progbits\n.globl tls\n.type tls,@tls_object\ntls: .quad 73\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n",
    );
    if arm {
        input[18..20].copy_from_slice(&183u16.to_le_bytes());
    }
    let mut image = link_with("provider.o", &[input], options(arm))
        .unwrap()
        .bytes;
    let f = object::File::parse(image.as_slice()).unwrap();
    let dynamic = f.section_by_name(".dynamic").unwrap().index().0;
    let symbols = f.section_by_name(".symtab").unwrap().index().0;
    let strings = f.section_by_name(".strtab").unwrap().index().0;
    let shoff = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + symbols * 64 + 4..shoff + symbols * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image
}
fn options(arm: bool) -> elf::Options {
    elf::Options {
        machine: if arm {
            elf::Machine::Aarch64
        } else {
            elf::Machine::X86_64
        },
        ..Default::default()
    }
}
fn records(exe: &elf::Executable) -> Vec<(u64, u32, u32, i64)> {
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    f.section_by_name(".rela.dyn")
        .unwrap()
        .data()
        .unwrap()
        .as_chunks::<24>()
        .0
        .iter()
        .map(|r| {
            let n = |at| u64::from_le_bytes(r[at..at + 8].try_into().unwrap());
            (n(0), n(8) as u32, (n(8) >> 32) as u32, n(16) as i64)
        })
        .collect()
}

#[test]
fn imported_tls_models_have_distinct_loader_records() {
    for arm in [false, true] {
        let kinds = if arm {
            vec![541, 542, 513, 514, 562, 563, 564, 569]
        } else {
            vec![22, 19, 34, 35]
        };
        let exe = link_with(
            "tls.o",
            &[fixture(&kinds, arm, false), shared(arm)],
            options(arm),
        )
        .unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        let tls = f
            .dynamic_symbols()
            .find(|s| s.name().unwrap() == "tls")
            .unwrap();
        assert!(tls.is_undefined());
        assert_eq!(tls.kind(), object::SymbolKind::Tls);
        let records = records(&exe);
        let mut actual: Vec<_> = records.iter().map(|r| r.1).collect();
        actual.sort_unstable();
        assert_eq!(
            actual,
            if arm {
                vec![1028, 1029, 1030, 1031]
            } else {
                vec![16, 17, 18, 36]
            }
        );
        assert!(records.iter().all(|r| r.2 != 0 && r.3 == 0));
        assert_eq!(f.section_by_name(".got.tls").unwrap().size(), 40);
        assert!(records.iter().all(|r| word(&exe.bytes, r.0) == 0));
        if arm {
            let fields = value(&f, "fields");
            let got = f.section_by_name(".got.tls").unwrap().address();
            for (index, slot) in [(0u64, 0u64), (2, 8), (4, 24)] {
                let place = fields + index * 8;
                let adrp = word(&exe.bytes, place) as u32;
                let immediate = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2);
                let delta = ((immediate as i32) << 11 >> 11) as i64 * 4096;
                let page = ((place & !4095) as i64 + delta) as u64;
                let low = (word(&exe.bytes, place + 8) as u32 >> 10) & 4095;
                let scale = if index == 2 { 1 } else { 8 };
                assert_eq!(page + u64::from(low) * scale, got + slot);
            }
        }
        if !arm {
            let fields = value(&f, "fields");
            let got = f.section_by_name(".got.tls").unwrap().address();
            for (i, offset) in [(0, 0), (1, 8), (2, 24)] {
                let place = fields + i * 8;
                assert_eq!(
                    (place as i64 + word(&exe.bytes, place) as i32 as i64) as u64,
                    got + offset
                );
            }
        }
    }
}

#[test]
fn executable_definitions_use_static_tls_records() {
    for arm in [false, true] {
        let kinds = if arm {
            vec![541, 513, 518, 562]
        } else {
            vec![22, 19, 20, 34, 35]
        };
        let exe = link_with("tls.o", &[fixture(&kinds, arm, true)], options(arm)).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        let got = f.section_by_name(".got.tls").unwrap().address();
        let tp = if arm { 32 } else { -32i64 as u64 };
        assert_eq!(word(&exe.bytes, got), tp);
        assert_eq!(word(&exe.bytes, got + 8), 1);
        assert_eq!(word(&exe.bytes, got + 16), 0);
        assert_eq!(word(&exe.bytes, got + 24), 1);
        assert_eq!(word(&exe.bytes, got + 32), 0);
        assert_eq!(word(&exe.bytes, got + 48), tp);
        assert_eq!(
            word(&exe.bytes, got + 40),
            f.section_by_name(".text.tls").unwrap().address()
        );
        assert_eq!(value(&f, "tls"), 0);
    }
}

#[test]
fn rejects_tls_type_mismatches_and_imported_local_exec() {
    for kind in [17, 18, 21, 23] {
        assert!(
            link(&[fixture(&[kind], false, false), shared(false)])
                .unwrap_err()
                .contains("imported TLS")
        );
    }
    let mut input = fixture(&[35], false, true);
    let f = object::File::parse(input.as_slice()).unwrap();
    let at = f.section_by_name(".data").unwrap().file_range().unwrap().0 as usize;
    input[at] = 0x90;
    assert!(link(&[input]).unwrap_err().contains("TLSDESC_CALL"));
}

#[test]
fn script_tls_models_and_explicit_header() {
    for explicit in [false, true] {
        let bytes = fixture(&[22, 19, 20, 34, 35, 18], false, true);
        let header = if explicit {
            "PHDRS { text PT_LOAD FLAGS(5); data PT_LOAD FLAGS(6); tls PT_TLS FLAGS(4); }"
        } else {
            ""
        };
        let text = if explicit { ":text" } else { "" };
        let data = if explicit { ":data" } else { "" };
        let tls = if explicit { ":data :tls" } else { "" };
        let script = format!(
            "{header} SECTIONS {{ . = 0x400000; .text : {{ *(.text) *(.text.tls) }} {text} . = ALIGN(4096); .data : {{ *(.data) *(.got.tls) }} {data} .tdata : {{ *(.tdata) }} {tls} .tbss : {{ *(.tbss) }} {tls} .bss : {{ *(.bss) *(COMMON) }} {data} }}"
        );
        let exe = link_with(
            "tls-script.o",
            &[bytes],
            elf::Options {
                script: Some(script),
                ..Default::default()
            },
        )
        .unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(value(&f, "tls"), 0);
        let data = f.section_by_name(".data").unwrap().data().unwrap();
        assert_eq!(i64::from_le_bytes(data[40..48].try_into().unwrap()), -32);
        let count = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap());
        let headers: Vec<_> = (0..usize::from(count))
            .map(|i| &exe.bytes[64 + i * 56..64 + (i + 1) * 56])
            .filter(|h| h[..4] == 7u32.to_le_bytes())
            .collect();
        assert_eq!(headers.len(), 1);
        let n = |at| u64::from_le_bytes(headers[0][at..at + 8].try_into().unwrap());
        assert_eq!(n(32), 8);
        assert_eq!(n(40), 8);
        assert_eq!(n(48), 32);
    }
}

#[test]
fn clang_pic_tls_objects_link_on_both_targets() {
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::record_skip("keel-link", "elf_tls", keel_test_support::NO_CLANG);
        return;
    };
    let dir = keel_test_support::TempDir::new("elf-tls-clang");
    let source = dir.join("tls.c");
    std::fs::write(&source, "__thread int tls = 73; static __thread int local; int read_tls(void) { return tls + ++local; } void _start(void) {}\n").unwrap();
    for arm in [false, true] {
        let path = dir.join("tls.o");
        let output = clang
            .command()
            .args([
                "-target",
                if arm {
                    "aarch64-linux-gnu"
                } else {
                    "x86_64-linux-gnu"
                },
                "-fPIC",
                "-O2",
                "-c",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let input = std::fs::read(path).unwrap();
        let mut resolver = object(
            ".text\n.globl __tls_get_addr\n.type __tls_get_addr,@function\n__tls_get_addr: ret\n",
        );
        if arm {
            resolver[18..20].copy_from_slice(&183u16.to_le_bytes());
        }
        let exe = link_with("clang-tls.o", &[input, resolver], options(arm)).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(
            f.symbol_by_name("tls").unwrap().kind(),
            object::SymbolKind::Tls
        );
        assert!(f.section_by_name(".got.tls").unwrap().size() >= 16);
    }
}

#[test]
fn linux_loader_and_new_thread_execute_all_x86_tls_models() {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        keel_test_support::record_skip(
            "keel-link",
            "elf_tls",
            "TLS loader execution requires x86-64 Linux with glibc",
        );
        return;
    }
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::record_skip("keel-link", "elf_tls", keel_test_support::NO_CLANG);
        return;
    };
    let loader = "/lib64/ld-linux-x86-64.so.2";
    let libc = [
        "/lib/x86_64-linux-gnu/libc.so.6",
        "/lib64/libc.so.6",
        "/usr/lib64/libc.so.6",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists());
    let Some(libc) = libc.filter(|_| std::path::Path::new(loader).exists()) else {
        keel_test_support::record_skip(
            "keel-link",
            "elf_tls",
            "glibc and its x86-64 loader are required",
        );
        return;
    };
    let dir = keel_test_support::TempDir::new("elf-tls-runtime");
    let provider = dir.join("provider.c");
    std::fs::write(
        &provider,
        r#"
#include <pthread.h>
__thread int imported = 37;
typedef int (*check_fn)(void);
static void *worker(void *p) { return (void *)(long)((check_fn)p)(); }
int run(check_fn check) {
    pthread_t thread;
    void *result;
    if (check()) return 1;
    if (pthread_create(&thread, 0, worker, (void *)check)) return 2;
    if (pthread_join(thread, &result)) return 3;
    return (int)(long)result;
}
"#,
    )
    .unwrap();
    let library = dir.join("provider.so");
    let result = clang
        .command()
        .args(["-shared", "-fPIC", "-pthread", "-Wl,-soname,provider.so"])
        .arg(&provider)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = dir.join("start.s");
    std::fs::write(
        &source,
        r#"
.text
.globl _start
_start:
    lea check(%rip), %rdi
    call run@PLT
    mov %eax, %edi
    mov $60, %eax
    syscall
check:
    push %rbx
    mov %fs:zero_tls@tpoff, %eax
    test %eax, %eax
    jne fail
    movl $1, %fs:zero_tls@tpoff
    mov imported@gottpoff(%rip), %rbx
    cmpl $37, %fs:(%rbx)
    jne fail
    .byte 0x66
    lea imported@tlsgd(%rip), %rdi
    .byte 0x66, 0x66, 0x48
    call __tls_get_addr@PLT
    cmpl $37, (%rax)
    jne fail
    .byte 0x66
    lea local_tls@tlsgd(%rip), %rdi
    .byte 0x66, 0x66, 0x48
    call __tls_get_addr@PLT
    cmpl $11, (%rax)
    jne fail
    lea zero_tls@tlsld(%rip), %rdi
    call __tls_get_addr@PLT
    cmpl $1, zero_tls@dtpoff(%rax)
    jne fail
    lea imported@tlsdesc(%rip), %rax
    call *imported@tlscall(%rax)
    cmpl $37, %fs:(%rax)
    jne fail
    lea local_tls@tlsdesc(%rip), %rax
    call *local_tls@tlscall(%rax)
    cmpl $11, %fs:(%rax)
    jne fail
    xor %eax, %eax
    pop %rbx
    ret
fail:
    mov $9, %eax
    pop %rbx
    ret
.section .tdata,"awT",@progbits
.balign 32
.type local_tls,@tls_object
local_tls: .long 11
.section .tbss,"awT",@nobits
.balign 16
.type zero_tls,@tls_object
zero_tls: .zero 4
.section .note.GNU-stack,"",@progbits
"#,
    )
    .unwrap();
    let object = dir.join("start.o");
    let result = clang
        .command()
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let exe = link_with(
        "runtime.o",
        &[
            std::fs::read(object).unwrap(),
            std::fs::read(&library).unwrap(),
            std::fs::read(libc).unwrap(),
            // glibc exports __tls_get_addr from its loader.
            std::fs::read(loader).unwrap(),
        ],
        elf::Options {
            runpath: vec![dir.path().to_str().unwrap().into()],
            ..Default::default()
        },
    )
    .unwrap();
    let path = dir.join("tls-exe");
    std::fs::write(&path, exe.bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = std::process::Command::new(path).output().unwrap();
    assert!(
        result.status.success(),
        "status {:?}, stderr {}",
        result.status.code(),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn aarch64_dynamic_function_got_and_absolute_imports() {
    let mut input = object(".text\n.globl _start\n_start: .quad run\n.data\n.quad run,run\n");
    let f = object::File::parse(input.as_slice()).unwrap();
    let text = f.section_by_name(".text").unwrap().file_range().unwrap().0 as usize;
    let rela = f
        .section_by_name(".rela.text")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let data = f
        .section_by_name(".rela.data")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    input[18..20].copy_from_slice(&183u16.to_le_bytes());
    input[text..text + 4].copy_from_slice(&0x94000000u32.to_le_bytes());
    input[rela + 8..rela + 12].copy_from_slice(&283u32.to_le_bytes());
    for i in 0..2 {
        input[data + i * 24 + 8..data + i * 24 + 12].copy_from_slice(&257u32.to_le_bytes());
    }
    let exe = link_with("arm-import.o", &[input, shared(true)], options(true)).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let plt = f.section_by_name(".plt").unwrap();
    assert_eq!(plt.size(), 16);
    assert_eq!(&plt.data().unwrap()[12..16], &0xd61f0220u32.to_le_bytes());
    let mut kinds: Vec<_> = records(&exe).into_iter().map(|r| r.1).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, [257, 257, 1025]);
    assert_eq!(
        f.section_by_name(".interp").unwrap().data().unwrap(),
        b"/lib/ld-linux-aarch64.so.1\0"
    );
}

#[test]
fn scripts_preserve_tbss_and_reject_invalid_template_layouts() {
    let input = fixture(&[18], false, true);
    for middle in [
        ".data : { *(.data) }",
        ". = ALIGN(4096); .data : { *(.data) }",
    ] {
        let script = format!(
            "SECTIONS {{ . = 0x400000; .text : {{ *(.text) }} .tdata : {{ *(.tdata) }} {middle} .tbss : {{ *(.tbss) }} }}"
        );
        // With no TBSS contribution in this fixture, data follows the template.
        link_with(
            "script.o",
            std::slice::from_ref(&input),
            elf::Options {
                script: Some(script),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let input = object(
        ".text\n.globl _start\n_start: ret\n.section .tdata,\"awT\",@progbits\n.long 1\n.section .tbss,\"awT\",@nobits\n.balign 32\n.zero 40\n.data\n.long 9\n",
    );
    let script = "SECTIONS { . = 0x400000; .text : { *(.text) } . = ALIGN(32); .tdata : { *(.tdata) } .tbss : { *(.tbss) } .data : { *(.data) } }";
    let exe = link_with(
        "script.o",
        std::slice::from_ref(&input),
        elf::Options {
            script: Some(script.into()),
            ..Default::default()
        },
    )
    .unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        f.section_by_name(".tbss").unwrap().kind(),
        object::SectionKind::UninitializedTls
    );
    for script in [
        "SECTIONS { . = 0x400000; .text : { *(.text) } .tdata : { *(.tdata) } .data : { *(.data) } .tbss : { *(.tbss) } }",
        "SECTIONS { . = 0x400000; .text : { *(.text) } .mixed : { *(.data) *(.tdata) } .tbss : { *(.tbss) } }",
        "SECTIONS { . = 0x400000; .text : { *(.text) } .mixed : { *(.tdata) *(.tbss) } .data : { *(.data) } }",
    ] {
        assert!(
            link_with(
                "script.o",
                std::slice::from_ref(&input),
                elf::Options {
                    script: Some(script.into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn canonical_general_dynamic_relaxes_without_a_resolver() {
    for imported in [false, true] {
        let mut input = object(&format!(
            ".text\n.globl _start\n_start:\n.byte 0x66,0x48,0x8d,0x3d\n.long tls-4\n.byte 0x66,0x66,0x48,0xe8\n.long __tls_get_addr-4\nret\n.type tls,@tls_object\n.globl tls\n{}",
            if imported {
                ""
            } else {
                ".section .tdata,\"awT\",@progbits\ntls: .quad 73\n"
            }
        ));
        let f = object::File::parse(input.as_slice()).unwrap();
        let at = f
            .section_by_name(".rela.text")
            .unwrap()
            .file_range()
            .unwrap()
            .0 as usize;
        input[at + 8..at + 12].copy_from_slice(&19u32.to_le_bytes());
        input[at + 32..at + 36].copy_from_slice(&4u32.to_le_bytes());
        let mut malformed = input.clone();
        let f = object::File::parse(input.as_slice()).unwrap();
        let text = f.section_by_name(".text").unwrap().file_range().unwrap().0 as usize;
        malformed[text] = 0x90;
        let mut unrelaxed = vec![malformed];
        if imported {
            unrelaxed.push(shared(false));
        }
        assert!(link(&unrelaxed).unwrap_err().contains("__tls_get_addr"));
        let mut retained = vec![input.clone(), object(".data\n.quad __tls_get_addr\n")];
        if imported {
            retained.push(shared(false));
        }
        assert!(link(&retained).unwrap_err().contains("__tls_get_addr"));
        let mut inputs = vec![input];
        if imported {
            inputs.push(shared(false));
        }
        let exe = link(&inputs).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        let code = f.section_by_name(".text").unwrap().data().unwrap();
        assert_eq!(&code[..9], &[0x64, 0x48, 0x8b, 0x04, 0x25, 0, 0, 0, 0]);
        assert_eq!(
            &code[9..12],
            if imported {
                &[0x48, 0x03, 0x05]
            } else {
                &[0x48, 0x8d, 0x80]
            }
        );
        assert!(
            !f.dynamic_symbols()
                .any(|s| s.name().unwrap() == "__tls_get_addr")
        );
        if imported {
            assert_eq!(records(&exe).iter().map(|r| r.1).collect::<Vec<_>>(), [18]);
        } else {
            assert_eq!(i32::from_le_bytes(code[12..16].try_into().unwrap()), -8);
        }
    }
}

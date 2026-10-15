use keel_link::{Dylib, Input, Options};
use std::collections::BTreeSet;
fn options() -> Options {
    Options {
        dylibs: vec![Dylib {
            install_name: "/usr/lib/libSystem.B.dylib".into(),
            exports: BTreeSet::from(["_puts".into()]),
        }],
        ..Options::default()
    }
}
fn object(s: &str) -> Vec<u8> {
    keel_asm::assemble(s).unwrap().to_macho().unwrap()
}
#[test]
fn malformed_inputs_never_panic() {
    for n in 0..128 {
        let bytes = vec![0; n];
        assert!(
            keel_link::link(
                &[Input {
                    name: "bad",
                    bytes: &bytes
                }],
                options()
            )
            .is_err()
        );
    }
    let valid = object(".globl _main\n_main:\nmov w0, #0\nret\n");
    for n in 0..valid.len() {
        assert!(
            keel_link::link(
                &[Input {
                    name: "truncated",
                    bytes: &valid[..n]
                }],
                options()
            )
            .is_err()
        );
    }
}
#[test]
fn undefined_and_duplicate_are_diagnostics() {
    let bad = object(".globl _main\n_main:\nb _missing\n");
    assert!(
        keel_link::link(
            &[Input {
                name: "missing.o",
                bytes: &bad
            }],
            options()
        )
        .err()
        .unwrap()
        .contains("_missing")
    );
    let main = object(".globl _main\n_main:\nret\n");
    assert!(
        keel_link::link(
            &[
                Input {
                    name: "a",
                    bytes: &main
                },
                Input {
                    name: "b",
                    bytes: &main
                }
            ],
            options()
        )
        .err()
        .unwrap()
        .contains("duplicate")
    );
}
#[test]
fn executable_layout_and_eager_imports_are_deterministic() {
    let main = object(
        ".globl _main\n_main:\n.cfi_startproc\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa x29,16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\nadrp x0, Lmsg@PAGE\nadd x0,x0,Lmsg@PAGEOFF\nbl _puts\nmov w0,#0\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n.section __TEXT,__cstring,cstring_literals\nLmsg:\n.asciz \"hello\"\n",
    );
    let build = || {
        keel_link::link(
            &[Input {
                name: "main.o",
                bytes: &main,
            }],
            options(),
        )
        .unwrap()
    };
    let a = build();
    let b = build();
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(&a.bytes[..4], &0xfeedfacfu32.to_le_bytes());
    assert!(a.bytes.windows(6).any(|s| s == b"_puts\0"));
    assert!(a.bytes.windows(13).any(|s| s == b"__unwind_info"));
}

#[test]
fn tls_and_branch_overflow_are_explicit() {
    let tls = object(".globl _main\n_main:\nret\n.tbss _thread,8,3\n");
    assert!(
        keel_link::link(
            &[Input {
                name: "tls.o",
                bytes: &tls
            }],
            options()
        )
        .is_err()
    );
    let mut branch =
        object(".subsections_via_symbols\n.globl _main\n_main:\nb _far\n.globl _far\n_far:\nret\n");
    // Make the branch destination an absolute address far below the PIE.
    let word = |at: usize, b: &[u8]| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    while word(at, &branch) != 2 {
        at += word(at + 4, &branch);
    }
    let (offset, count, strings) = (
        word(at + 8, &branch),
        word(at + 12, &branch),
        word(at + 16, &branch),
    );
    for i in 0..count {
        let at = offset + i * 16;
        let name = strings + word(at, &branch);
        if branch[name..].starts_with(b"_far\0") {
            branch[at + 4] = 3;
            branch[at + 5] = 0;
            branch[at + 8..at + 16].fill(0);
        }
    }
    assert!(
        keel_link::link(
            &[Input {
                name: "range.o",
                bytes: &branch
            }],
            options()
        )
        .is_err()
    );
}

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut a = b"!<arch>\n".to_vec();
    for (name, bytes) in members {
        a.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                "100644",
                bytes.len()
            )
            .as_bytes(),
        );
        a.extend_from_slice(bytes);
        if !a.len().is_multiple_of(2) {
            a.push(b'\n');
        }
    }
    a
}
#[test]
fn archives_extract_transitive_members_and_skip_unused_duplicate() {
    let main = object(".globl _main\n_main:\nb _a\n");
    let a = object(".globl _a\n_a:\nb _b\n");
    let b = object(".globl _b\n_b:\nret\n");
    let unused = object(".globl _main\n_main:\nret\n");
    let ar = archive(&[("b.o", b), ("unused.o", unused), ("a.o", a)]);
    keel_link::link(
        &[
            Input {
                name: "main",
                bytes: &main,
            },
            Input {
                name: "libtest.a",
                bytes: &ar,
            },
        ],
        options(),
    )
    .unwrap();
}
#[test]
fn same_named_local_got_symbols_stay_object_local() {
    let first = object(
        ".globl _main\n_main:\nadrp x0,_local@GOTPAGE\nldr x0,[x0,_local@GOTPAGEOFF]\nret\n.data\n_local:\n.quad 1\n",
    );
    let second = object(
        ".globl _other\n_other:\nadrp x0,_local@GOTPAGE\nldr x0,[x0,_local@GOTPAGEOFF]\nret\n.data\n_local:\n.quad 2\n",
    );
    keel_link::link(
        &[
            Input {
                name: "one",
                bytes: &first,
            },
            Input {
                name: "two",
                bytes: &second,
            },
        ],
        options(),
    )
    .unwrap();
}

#[test]
fn tentative_common_alignment_and_strong_override() {
    let main = object(
        ".globl _main\n_main:\nadrp x0,_common@PAGE\nadd x0,x0,_common@PAGEOFF\nret\n.comm _common,32,4\n",
    );
    let other = object(".globl _other\n_other:\nret\n.comm _common,64,5\n");
    let strong = object(".data\n.globl _common\n.p2align 5\n_common:\n.space 128\n");
    for inputs in [
        vec![
            Input {
                name: "main",
                bytes: &main,
            },
            Input {
                name: "other",
                bytes: &other,
            },
        ],
        vec![
            Input {
                name: "main",
                bytes: &main,
            },
            Input {
                name: "strong",
                bytes: &strong,
            },
        ],
    ] {
        keel_link::link(&inputs, options()).unwrap();
    }
}

#[test]
fn multiple_bss_contributions_remain_zero_fill_without_file_copy() {
    let main = object(".globl _main\n_main:\nret\n.zerofill __DATA,__bss,_first,16,3\n");
    let other = object(".zerofill __DATA,__bss,_second,64,4\n");
    keel_link::link(
        &[
            Input {
                name: "first",
                bytes: &main,
            },
            Input {
                name: "second",
                bytes: &other,
            },
        ],
        options(),
    )
    .unwrap();
}

#[test]
fn pointer_to_got_ignores_apple_assembler_placeholder_words() {
    let base = object(
        ".globl _main\n_main:\nret\n.section __TEXT,__gcc_except_tab\nLref:\n.long _puts@GOT-Lref\n",
    );
    let mut placeholder = base.clone();
    let word = |at: usize, b: &[u8]| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    // Locate the data word through the serialized section header, independently
    // of the linker's reader. Apple uses a nonzero placeholder in this field.
    let count = word(32 + 64, &base);
    let section = (0..count)
        .map(|i| 32 + 72 + i * 80)
        .find(|&s| &base[s..s + 16] == b"__gcc_except_tab")
        .unwrap();
    let offset = word(section + 48, &base);
    placeholder[offset..offset + 4].copy_from_slice(&0xbfffffe0u32.to_le_bytes());
    let build = |bytes: &[u8]| {
        keel_link::link(
            &[Input {
                name: "got.o",
                bytes,
            }],
            options(),
        )
        .unwrap()
        .bytes
    };
    assert_eq!(build(&base), build(&placeholder));
}

#[test]
fn local_branch_target_can_share_an_imports_name() {
    let mut local = object(
        ".subsections_via_symbols\n.globl _main\n_main:\nb _puts\n.globl _puts\n_puts:\nret\n",
    );
    let word = |at: usize, b: &[u8]| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    while word(at, &local) != 2 {
        at += word(at + 4, &local);
    }
    let (offset, count, strings) = (
        word(at + 8, &local),
        word(at + 12, &local),
        word(at + 16, &local),
    );
    for i in 0..count {
        let at = offset + i * 16;
        if local[strings + word(at, &local)..].starts_with(b"_puts\0") {
            local[at + 4] = 0xe;
        }
    }
    let imported = object(".globl _other\n_other:\nb _puts\n");
    keel_link::link(
        &[
            Input {
                name: "local",
                bytes: &local,
            },
            Input {
                name: "imported",
                bytes: &imported,
            },
        ],
        options(),
    )
    .unwrap();
}

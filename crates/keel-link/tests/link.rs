use keel_link::{Dylib, Input, Options};
use std::collections::BTreeSet;
fn options() -> Options {
    Options {
        dylibs: vec![Dylib {
            install_name: "/usr/lib/libSystem.B.dylib".into(),
            exports: BTreeSet::from(["_puts".into()]),
            weak_exports: BTreeSet::new(),
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
fn tls_zero_fill_is_accepted_and_branch_overflow_is_explicit() {
    let tls = object(".globl _main\n_main:\nret\n.tbss _thread,8,3\n");
    keel_link::link(
        &[Input {
            name: "tls.o",
            bytes: &tls,
        }],
        options(),
    )
    .unwrap();
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

fn symbol_entry(bytes: &[u8], name: &str) -> usize {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    while word(at) != 2 {
        at += word(at + 4);
    }
    let (offset, count, strings) = (word(at + 8), word(at + 12), word(at + 16));
    (0..count)
        .map(|i| offset + i * 16)
        .find(|&at| {
            let start = strings + word(at);
            bytes[start..].starts_with(format!("{name}\0").as_bytes())
        })
        .unwrap()
}
fn weak_symbol(bytes: &mut [u8], name: &str, desc: u16) {
    let at = symbol_entry(bytes, name);
    bytes[at + 6..at + 8].copy_from_slice(&desc.to_le_bytes());
}
#[test]
fn strong_overrides_weak_in_both_orders_and_weak_keeps_first() {
    let main = object(".globl _main\n_main:\nb _f\n");
    let mut weak = object(".globl _f\n_f:\nmov w0,#17\nret\n");
    weak_symbol(&mut weak, "_f", 0x80);
    let strong = object(".globl _f\n_f:\nmov w0,#29\nret\n");
    for objects in [
        vec![&weak, &strong],
        vec![&strong, &weak],
        vec![&weak, &weak],
    ] {
        let mut inputs = vec![Input {
            name: "main",
            bytes: &main,
        }];
        inputs.extend(objects.iter().map(|bytes| Input {
            name: "memory",
            bytes,
        }));
        let exe = keel_link::link(&inputs, options()).unwrap();
        let at = symbol_entry(&exe.bytes, "_f");
        let address = u64::from_le_bytes(exe.bytes[at + 8..at + 16].try_into().unwrap());
        let offset = (address - 0x1_0000_0000) as usize;
        let word = u32::from_le_bytes(exe.bytes[offset..offset + 4].try_into().unwrap());
        let expected = if std::ptr::eq(objects[0], objects[1]) {
            17
        } else {
            29
        };
        assert_eq!((word >> 5) & 65535, expected);
    }
}
#[test]
fn missing_weak_import_is_permitted_but_a_strong_reference_still_fails() {
    let mut main = object(
        ".globl _main\n_main:\nadrp x0,_absent@GOTPAGE\nldr x0,[x0,_absent@GOTPAGEOFF]\nret\n",
    );
    weak_symbol(&mut main, "_absent", 0x40);
    keel_link::link(
        &[Input {
            name: "weak",
            bytes: &main,
        }],
        options(),
    )
    .unwrap();
    let strong = object(".globl _other\n_other:\nb _absent\n");
    assert!(
        keel_link::link(
            &[
                Input {
                    name: "weak",
                    bytes: &main
                },
                Input {
                    name: "strong",
                    bytes: &strong
                }
            ],
            options()
        )
        .err()
        .unwrap()
        .contains("undefined symbol `_absent`")
    );
}
#[test]
fn tlvp_requires_a_descriptor_and_debug_paths_are_explicit() {
    let main = object(
        ".globl _main\n_main:\nadrp x0,_ordinary@TLVPPAGE\nldr x0,[x0,_ordinary@TLVPPAGEOFF]\nret\n.data\n.globl _ordinary\n_ordinary:\n.quad 0\n",
    );
    assert!(
        keel_link::link(
            &[Input {
                name: "memory",
                bytes: &main
            }],
            options()
        )
        .err()
        .unwrap()
        .contains("TLS descriptor")
    );
    let mut opts = options();
    opts.debug_directory = Some("relative".into());
    assert!(
        keel_link::link(&[], opts)
            .err()
            .unwrap()
            .contains("absolute")
    );
}

#[test]
fn discarded_debug_payload_preserves_image_and_requested_archive_snapshots() {
    let source = ".text\n.globl _main\n_main:\nmov w0,#0\nret\n";
    let plain = object(source);
    let debug = object(&format!(
        "{source}.section __DWARF,__debug_str,regular,debug\n.asciz \"retained debug bytes\"\n"
    ));
    let build = |bytes: &[u8], opts| {
        keel_link::link(
            &[Input {
                name: "memory.o",
                bytes,
            }],
            opts,
        )
        .unwrap()
    };
    let expected = build(&plain, options());
    let omitted = build(&debug, options());
    assert_eq!(expected.bytes, omitted.bytes);
    assert!(omitted.debug_objects.is_empty());
    assert_eq!(omitted.warnings.len(), 1);

    let unused = object(
        ".text\n.globl _unused\n_unused:\nret\n.section __DWARF,__debug_str,regular,debug\n.asciz \"unused\"\n",
    );
    let ar = archive(&[("entry.o", debug.clone()), ("unused.o", unused)]);
    let mut opts = options();
    opts.debug_directory = Some(std::env::temp_dir().join("keel-memory-debug-snapshots"));
    let retained = build(&ar, opts);
    assert!(retained.warnings.is_empty());
    assert_eq!(retained.debug_objects.len(), 1);
    assert_eq!(retained.debug_objects[0].bytes, debug);
}

#[test]
fn discarded_debug_payload_still_requires_valid_file_bounds() {
    let mut bytes = object(
        ".text\n.globl _main\n_main:\nret\n.section __DWARF,__debug_str,regular,debug\n.asciz \"debug\"\n",
    );
    let read =
        |bytes: &[u8], at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    let mut found = false;
    for _ in 0..read(&bytes, 16) {
        if read(&bytes, at) == 0x19 {
            for i in 0..read(&bytes, at + 64) {
                let section = at + 72 + i * 80;
                if &bytes[section..section + 11] == b"__debug_str" {
                    bytes[section + 48..section + 52].copy_from_slice(&u32::MAX.to_le_bytes());
                    found = true;
                }
            }
        }
        at += read(&bytes, at + 4);
    }
    assert!(found);
    for retain in [false, true] {
        let mut opts = options();
        if retain {
            opts.debug_directory = Some(std::env::temp_dir().join("keel-invalid-debug"));
        }
        let error = keel_link::link(
            &[Input {
                name: "bad-debug.o",
                bytes: &bytes,
            }],
            opts,
        )
        .err()
        .unwrap();
        assert!(error.contains("truncated Mach-O input"), "{error}");
    }
}

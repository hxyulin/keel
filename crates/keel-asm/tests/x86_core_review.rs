//! Reproductions for the independent 2026-10-22 core review. Encoding only.
use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection};
fn bytes(source: &str) -> Vec<u8> {
    let object = assemble_object(source).unwrap();
    object::File::parse(object.as_slice())
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn core01_extensions_reject_invalid_classes_and_byte_destinations() {
    for bad in [
        "movzx al,bl",
        "movsx al,bl",
        "movzx ah,cl",
        "movsx al,byte ptr [rax]",
        "movsx eax,fs",
        "movzx eax,gs",
        "movsxd eax,fs",
        "movsxd ax,fs",
    ] {
        assert!(assemble_object(bad).is_err(), "accepted {bad}");
    }
    for (source, expected) in [
        ("movsx ax,bx", vec![0x66, 0x0f, 0xbf, 0xc3]),
        ("movzx ax,word ptr [rax]", vec![0x66, 0x0f, 0xb7, 0]),
        ("movsxd eax,ebx", vec![0x63, 0xc3]),
        ("movsxd ax,bx", vec![0x66, 0x63, 0xc3]),
    ] {
        assert_eq!(bytes(source), expected, "{source}");
    }
}
#[test]
fn core02_core_suffixes_validate_register_and_pointer_widths() {
    for (suffix, acc) in [("b", "al"), ("w", "ax"), ("l", "eax")] {
        for other in ["al", "ax", "eax"] {
            for source in [
                format!(".att_syntax\nin{suffix} $7,%{other}"),
                format!(".att_syntax\nout{suffix} %{other},$7"),
            ] {
                assert_eq!(assemble_object(&source).is_ok(), acc == other, "{source}");
            }
        }
    }
    for op in ["lfs", "lgs", "lss"] {
        for (suffix, reg) in [("w", "r9w"), ("l", "r9d"), ("q", "r9")] {
            for other in ["r9w", "r9d", "r9"] {
                let source = format!(".att_syntax\n{op}{suffix} (%rax),%{other}");
                assert_eq!(assemble_object(&source).is_ok(), reg == other, "{source}");
            }
        }
    }
    for bad in [
        "inb eax,7",
        "outw 7,eax",
        "lfsw r9,[rax]",
        "lfsl r9d,dword ptr [rax]",
    ] {
        assert!(assemble_object(bad).is_err(), "accepted {bad}");
    }
}
#[test]
fn core03_far_transfers_consume_indirect_marker_and_keep_width() {
    for (source, expected) in [
        ("lcallq *0(%rax)", vec![0x48, 0xff, 0x18]),
        ("ljmpl *0(%rip)", vec![0xff, 0x2d, 0, 0, 0, 0]),
        ("lcallw *0(%rax)", vec![0x66, 0xff, 0x18]),
        ("ljmpq *0(%rax)", vec![0x48, 0xff, 0x28]),
    ] {
        assert_eq!(
            bytes(&format!(".att_syntax\n{source}")),
            expected,
            "{source}"
        );
    }
    for bad in ["lcallq *%rax", "ljmpl *$7", "lcallq 0(%rax)", "ljmpl $7"] {
        assert!(
            assemble_object(&format!(".att_syntax\n{bad}")).is_err(),
            "accepted {bad}"
        );
    }
}
#[test]
fn core05_intel_far_aliases_preserve_suffix_and_pointer_width() {
    for (op, group) in [("lcall", 0x18), ("ljmp", 0x28)] {
        for (suffix, width, prefix) in [
            ("w", "dword", Some(0x66)),
            ("l", "fword", None),
            ("q", "tbyte", Some(0x48)),
        ] {
            let mut expected = prefix.into_iter().collect::<Vec<_>>();
            expected.extend([0xff, group]);
            let inferred_source = format!(".intel_syntax noprefix\n{op}{suffix} [rax]");
            assert_eq!(bytes(&inferred_source), expected, "{inferred_source}");
            for other in ["dword", "fword", "tbyte"] {
                let source = format!(".intel_syntax noprefix\n{op}{suffix} {other} ptr [rax]");
                if width == other {
                    assert_eq!(bytes(&source), expected, "{source}");
                } else {
                    assert!(assemble_object(&source).is_err(), "accepted {source}");
                }
            }
        }
    }
    for source in ["lcallq rax", "ljmpw 7", "lcalll word ptr [rax]"] {
        assert!(assemble_object(source).is_err(), "accepted {source}");
    }
    for (op, group) in [("callf", 0x18), ("jmpf", 0x28)] {
        for (width, prefix) in [
            ("dword", Some(0x66)),
            ("fword", None),
            ("tbyte", Some(0x48)),
        ] {
            let source = format!("{op} {width} ptr [rax]");
            let mut expected = prefix.into_iter().collect::<Vec<_>>();
            expected.extend([0xff, group]);
            assert_eq!(bytes(&source), expected, "{source}");
        }
    }
}
#[test]
fn core04_fixed_string_operands_honor_register_prefix_mode() {
    for source in [
        ".att_syntax noprefix\nmovsb (rsi),(rdi)",
        ".intel_syntax prefix\nmovsb byte ptr [%rdi],byte ptr [%rsi]",
        ".att_syntax\nmovsb (%rsi),(%rdi)",
        ".intel_syntax noprefix\nmovsb byte ptr [rdi],byte ptr [rsi]",
    ] {
        assert_eq!(bytes(source), [0xa4], "{source}");
    }
    for source in [
        ".att_syntax noprefix\nmovsb (%rsi),(%rdi)",
        ".intel_syntax prefix\nmovsb byte ptr [rdi],byte ptr [rsi]",
        ".att_syntax\nmovsb (rsi),(rdi)",
        ".intel_syntax noprefix\nmovsb byte ptr [%rdi],byte ptr [%rsi]",
    ] {
        assert!(assemble_object(source).is_err(), "accepted {source}");
    }
    for source in [
        ".att_syntax noprefix\nmovsb fs:(esi),es:(edi)",
        ".intel_syntax prefix\nmovsb byte ptr %es:[%edi],byte ptr %fs:[%esi]",
    ] {
        assert_eq!(bytes(source), [0x64, 0x67, 0xa4], "{source}");
    }
}

#[test]
fn fixed_srm_zero_uses_r8() {
    for (source, expected) in [
        ("xchg.acc r8w,ax", vec![0x66, 0x41, 0x90]),
        ("xchg.acc r8d,eax", vec![0x41, 0x90]),
        ("xchg.acc r8,rax", vec![0x49, 0x90]),
    ] {
        assert_eq!(bytes(source), expected, "{source}");
    }
}

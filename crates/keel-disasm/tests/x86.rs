use keel_disasm::x86::{Operand, decode};

#[test]
fn literal_legacy_vex_and_system_witnesses() {
    for (bytes, text) in [
        (&[0x48, 0x89, 0xd8][..], "mov rax, rbx"),
        (&[0x40, 0x88, 0xe0][..], "mov al, spl"),
        (&[0x88, 0xe0][..], "mov al, ah"),
        (&[0x66, 0x41, 0x83, 0xc1, 1][..], "add r9w, 1"),
        (
            &[0x4b, 0x8b, 0x44, 0xec, 0xf0][..],
            "mov rax, qword ptr [r12 + r13*8 - 16]",
        ),
        (
            &[0x67, 0x8b, 0x44, 0x88, 8][..],
            "mov eax, dword ptr [eax + ecx*4 + 8]",
        ),
        (
            &[0x64, 0x48, 0x8b, 0x04, 0x25, 0, 0, 0, 0][..],
            "mov rax, qword ptr fs:[0]",
        ),
        (&[0xc5, 0xf0, 0x58, 0xc2][..], "vaddps xmm0, xmm1, xmm2"),
        (&[0xc4, 0x42, 0xb0, 0xf2, 0xc2][..], "andn r8, r9, r10"),
        (&[0xc5, 0xf8, 0x77][..], "vzeroupper"),
        (&[0x66, 0x48, 0x0f, 0x38, 0xf6, 0xc1][..], "adcx rax, rcx"),
        (&[0xf3, 0x48, 0x0f, 0x38, 0xf6, 0xc1][..], "adox rax, rcx"),
        (&[0x0f, 0x38, 0xc9, 0xc1][..], "sha1msg1 xmm0, xmm1"),
        (
            &[0xc4, 0xe2, 0x71, 0xdc, 0xc2][..],
            "vaesenc xmm0, xmm1, xmm2",
        ),
        (
            &[0xc4, 0xe2, 0x75, 0xdc, 0xc2][..],
            "vaesenc ymm0, ymm1, ymm2",
        ),
        (
            &[0xc4, 0xe3, 0x71, 0x44, 0xc2, 17][..],
            "vpclmulqdq xmm0, xmm1, xmm2, 17",
        ),
        (&[0x0f, 0x01, 0x10][..], "lgdt tbyte ptr [rax]"),
        (
            &[0x43, 0x0f, 0x01, 0x5c, 0x8c, 16][..],
            "lidt tbyte ptr [r12 + r9*4 + 16]",
        ),
        (&[0x0f, 0x01, 0xf8][..], "swapgs"),
        (&[0x0f, 0x32][..], "rdmsr"),
        (&[0x0f, 0x30][..], "wrmsr"),
        (&[0x0f, 0x20, 0xc0][..], "mov rax, cr0"),
        (&[0x0f, 0x21, 0xc0][..], "mov rax, dr0"),
        (&[0x45, 0x0f, 0x20, 0xc0][..], "mov r8, cr8"),
        (&[0x45, 0x0f, 0x22, 0xc7][..], "mov cr8, r15"),
        (&[0x41, 0x0f, 0x23, 0xf9][..], "mov dr7, r9"),
        (&[0x0f, 0x21, 0xe0][..], "mov rax, dr4"),
        (&[0x0f, 0x23, 0xe8][..], "mov dr5, rax"),
        (
            &[0xf0, 0x48, 0x01, 0x18][..],
            "lock add qword ptr [rax], rbx",
        ),
        (&[0xf3, 0x90][..], "pause"),
        (&[0xf3, 0xa4][..], "rep movsb"),
        (&[0xf2, 0xa6][..], "repne cmpsb"),
        (&[0x66, 0xcb][..], "retfw"),
        (&[0x48, 0xcb][..], "retfq"),
        (&[0xff, 0x18][..], "callf fword ptr [rax]"),
        (&[0xd9, 0xe8][..], "fld1"),
        (&[0xc3][..], "ret"),
    ] {
        let instruction = decode(bytes, 0x1000).unwrap_or_else(|| panic!("{bytes:02x?}"));
        assert_eq!(instruction.bytes, bytes);
        assert_eq!(instruction.assembly(), text, "{bytes:02x?}");
        for length in 0..bytes.len() {
            assert!(
                decode(&bytes[..length], 0).is_none(),
                "truncated {bytes:02x?} at {length}"
            );
        }
    }
}

#[test]
fn branch_targets_and_following_instruction_boundaries() {
    let bytes = [0xeb, 0xfe, 0xe8, 1, 0, 0, 0, 0xc3];
    let branch = decode(&bytes, 0x1000).unwrap();
    assert_eq!(branch.operands, [Operand::Target(0x1000)]);
    assert_eq!(branch.bytes.len(), 2);
    let call = decode(&bytes[2..], 0x1002).unwrap();
    assert_eq!(call.operands, [Operand::Target(0x1008)]);
    assert_eq!(call.bytes.len(), 5);
    assert_eq!(decode(&bytes[7..], 0x1007).unwrap().assembly(), "ret");
}

#[test]
fn malformed_and_overlong_inputs_do_not_invent_lengths() {
    for bytes in [
        &[0xf0, 0x01, 0xc0][..],
        &[0xc5, 0xf0, 0x77][..],
        &[0x0f, 0x01][..],
        &[0xc4][..],
        &[0x66, 0xc5, 0xf8, 0x77][..],
    ] {
        assert!(decode(bytes, 0).is_none(), "{bytes:02x?}");
    }
    assert!(decode(&[0x66; 15], 0).is_none());
    let mut overlong = vec![0x66; 15];
    overlong.push(0x90);
    assert!(decode(&overlong, 0).is_none());
    let mut longest = vec![0x66; 14];
    longest.push(0x90);
    assert_eq!(decode(&longest, 0).unwrap().bytes.len(), 15);
}

#[test]
fn arbitrary_short_inputs_are_bounded() {
    // Enumerate every one/two-byte start, including all incomplete prefix and
    // opcode-map combinations. No successful decode can consume absent bytes.
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            let bytes = [first, second];
            if let Some(instruction) = decode(&bytes, 0) {
                assert!(!instruction.bytes.is_empty() && instruction.bytes.len() <= bytes.len());
            }
        }
    }
}

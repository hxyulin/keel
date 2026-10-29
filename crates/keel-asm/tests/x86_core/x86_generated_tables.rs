use object::{Object, ObjectSection};

fn text(source: &str) -> Vec<u8> {
    let bytes = keel_asm::x86::assemble_object(source).unwrap();
    object::File::parse(bytes.as_slice())
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn exact_aliases_preserve_byte_forms_and_explicit_width_conventions() {
    for (source, expected) in [
        ("mov.imm al,1", vec![0xc6, 0xc0, 1]),
        ("mov.imm r9b,1", vec![0x41, 0xc6, 0xc1, 1]),
        ("pushw [rax]", vec![0x66, 0xff, 0x30]),
        ("popw [rax]", vec![0x66, 0x8f, 0x00]),
        ("movq.7e rax,xmm1", vec![0x66, 0x48, 0x0f, 0x7e, 0xc8]),
        ("movq.6e mm1,rax", vec![0x48, 0x0f, 0x6e, 0xc8]),
        (
            "movabs al,fs:[123]",
            vec![0x64, 0xa0, 123, 0, 0, 0, 0, 0, 0, 0],
        ),
        ("movabs32 al,fs:[123]", vec![0x64, 0x67, 0xa0, 123, 0, 0, 0]),
    ] {
        assert_eq!(text(source), expected, "{source}");
    }
    for source in [
        "push.rm [rax]",
        "pop.rm [rax]",
        "movq.7e xmm1,xmm2",
        "movq.7e xmm1,qword ptr [r12]",
    ] {
        assert!(keel_asm::x86::assemble_object(source).is_err(), "{source}");
    }
}

use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

fn object(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn fixture(name: &str) -> Vec<u8> {
    object(&format!(
        ".text\n.p2align 4\n.globl {name}\n.type {name},@function\n{name}:\n.cfi_startproc\npush rbp\n.cfi_def_cfa_offset 16\n.cfi_offset rbp,-16\nmov rbp,rsp\n.cfi_def_cfa_register rbp\npop rbp\n.cfi_def_cfa rsp,8\nret\n.cfi_endproc\n"
    ))
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "unwind.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}
fn u64_at(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap())
}
fn put32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(data: &mut [u8], at: usize, value: u64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn locations(data: &[u8]) -> (usize, usize, usize) {
    let file = object::File::parse(data).unwrap();
    let eh = file.section_by_name(".eh_frame").unwrap();
    let rela = file.section_by_name(".rela.eh_frame").unwrap();
    (
        u64_at(data, 40) as usize + eh.index().0 * 64,
        eh.file_range().unwrap().0 as usize,
        rela.file_range().unwrap().0 as usize,
    )
}
fn symbol(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}

#[test]
fn empty_frame_contributions_keep_extended_records_aligned() {
    for fde in [false, true] {
        for padding in 0..8 {
            let records = object(&format!(
                ".text\n.globl _start\n_start: ret\n.section .eh_frame,\"a\",@progbits\n.p2align 2\n.long 12,0\n.byte 1,0,1,120,16,0,0,0\n{}\n.long 0\n.space {padding}\n",
                if fde {
                    ".long 20,20\n.quad _start,1"
                } else {
                    ""
                }
            ));
            assert!(link(std::slice::from_ref(&records)).is_ok());
            for alignment in [1, 2, 4, 64] {
                // The assembler omits empty sections, so retain the section
                // header and its offset-zero symbol while making it empty.
                let mut empty = object(
                    ".section .eh_frame,\"a\",@progbits\n.globl empty_frame_anchor\nempty_frame_anchor: .byte 0\n",
                );
                let file = object::File::parse(empty.as_slice()).unwrap();
                let section = file.section_by_name(".eh_frame").unwrap();
                let header = u64_at(&empty, 40) as usize + section.index().0 * 64;
                put64(&mut empty, header + 32, 0);
                put64(&mut empty, header + 48, alignment);
                for inputs in [
                    [records.clone(), empty.clone()],
                    [empty.clone(), records.clone()],
                ] {
                    let exe = link(&inputs).unwrap_or_else(|error| {
                        panic!("fde={fde} padding={padding} alignment={alignment}: {error}")
                    });
                    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
                    let section = file.section_by_name(".eh_frame").unwrap();
                    assert_eq!(symbol(&file, "empty_frame_anchor") % alignment, 0);
                    let data = section.data().unwrap();
                    let mut at = 0;
                    while at < data.len() && u32_at(data, at) != 0 {
                        let length = u32_at(data, at) as usize;
                        assert_eq!(length % 4, 0);
                        at += 4 + length;
                    }
                    assert!(data[at..].iter().all(|&byte| byte == 0));
                }
            }
        }
    }
}

#[test]
fn merges_frame_contributions_and_indexes_relocated_records() {
    let a = fixture("_start");
    let mut b = fixture("second");
    let (header, _, _) = locations(&b);
    put32(&mut b, header + 4, object::elf::SHT_PROGBITS);
    put64(&mut b, header + 48, 64);
    for inputs in [[a.clone(), b.clone()], [b, a]] {
        let exe = link(&inputs).unwrap();
        assert_eq!(exe.bytes, link(&inputs).unwrap().bytes);
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let frames: Vec<_> = file
            .sections()
            .filter(|s| s.name().unwrap() == ".eh_frame")
            .collect();
        assert_eq!(frames.len(), 1);
        let shoff = u64_at(&exe.bytes, 40) as usize;
        let section = &frames[0];
        assert_eq!(section.align(), 64);
        assert_eq!(section.address() % section.align(), 0);
        let mut base = 0usize;
        for (i, input) in inputs.iter().enumerate() {
            let (header, _, _) = locations(input);
            let out_header = shoff + section.index().0 * 64;
            assert_eq!(
                u32_at(&exe.bytes, out_header + 4),
                object::elf::SHT_PROGBITS
            );
            let alignment = u64_at(input, header + 48) as usize;
            base = (base + alignment - 1) & !(alignment - 1);
            assert_eq!((section.address() + base as u64) % alignment as u64, 0);
            assert_eq!(u64_at(&exe.bytes, out_header + 8), 2);
            let f = object::File::parse(input.as_slice()).unwrap();
            let original = f.section_by_name(".eh_frame").unwrap();
            let mut expected = original.data().unwrap().to_vec();
            // Only the relocated FDE PC field changes; all CFI and padding survive.
            let rel = original.relocations().next().unwrap().0 as usize;
            let data = section.data().unwrap();
            expected[rel..rel + 4].copy_from_slice(&data[base + rel..base + rel + 4]);
            if i + 1 < inputs.len() {
                let next = object::File::parse(inputs[i + 1].as_slice()).unwrap();
                let alignment = next.section_by_name(".eh_frame").unwrap().align() as usize;
                let next_base = (base + expected.len() + alignment - 1) & !(alignment - 1);
                let mut record = 0;
                let mut last = 0;
                while record < expected.len() && u32_at(&expected, record) != 0 {
                    last = record;
                    record += 4 + u32_at(&expected, record) as usize;
                }
                put32(&mut expected, last, (next_base - base - last - 4) as u32);
            }
            assert_eq!(expected, &data[base..base + expected.len()]);
            base += expected.len();
        }
        let hdr = file.section_by_name(".eh_frame_hdr").unwrap();
        let data = hdr.data().unwrap();
        assert_eq!(&data[..4], &[1, 0x1b, 3, 0x3b]);
        assert_eq!(u32_at(data, 8), 2);
        let relative =
            |at: usize| (hdr.address() as i64 + i64::from(u32_at(data, at) as i32)) as u64;
        let mut functions = [symbol(&file, "_start"), symbol(&file, "second")];
        functions.sort();
        for (i, pc) in functions.into_iter().enumerate() {
            assert_eq!(relative(12 + i * 8), pc);
            let fde = relative(16 + i * 8);
            let section = frames
                .iter()
                .find(|s| fde >= s.address() && fde < s.address() + s.size())
                .unwrap();
            let at = (fde - section.address()) as usize;
            let field = fde + 8;
            assert_eq!(
                (field as i64 + i64::from(u32_at(section.data().unwrap(), at + 8) as i32)) as u64,
                pc
            );
        }
        let phoff = u64_at(&exe.bytes, 32) as usize;
        let phnum = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
        let headers: Vec<_> = (0..phnum).map(|i| phoff + i * 56).collect();
        let eh = *headers
            .iter()
            .find(|&&p| u32_at(&exe.bytes, p) == object::elf::PT_GNU_EH_FRAME)
            .unwrap();
        assert_eq!(u32_at(&exe.bytes, eh + 4), 4);
        assert_eq!(u64_at(&exe.bytes, eh + 16), hdr.address());
        assert_eq!(u64_at(&exe.bytes, eh + 32), hdr.size());
        for section in frames.iter().chain(std::iter::once(&hdr)) {
            assert!(headers.iter().any(|&p| u32_at(&exe.bytes, p) == 1
                && u32_at(&exe.bytes, p + 4) == 4
                && section.address() >= u64_at(&exe.bytes, p + 16)
                && section.address() + section.size()
                    <= u64_at(&exe.bytes, p + 16) + u64_at(&exe.bytes, p + 32)));
        }
    }
}

#[test]
fn malformed_unwind_sections_records_and_relocations_are_errors() {
    let valid = fixture("_start");
    let (header, data, rela) = locations(&valid);
    let fde = data + 4 + u32_at(&valid, data) as usize;
    let pc = u64_at(&valid, rela) as usize;
    let mut cases = vec![
        (header + 8, 0u64.to_le_bytes().to_vec(), "SHF_ALLOC"),
        (header + 8, 3u64.to_le_bytes().to_vec(), "SHF_ALLOC"),
        (header + 8, 6u64.to_le_bytes().to_vec(), "SHF_ALLOC"),
        (header + 8, 0x12u64.to_le_bytes().to_vec(), "SHF_ALLOC"),
        (header + 8, 0x402u64.to_le_bytes().to_vec(), "SHF_ALLOC"),
        (header + 48, 2u64.to_le_bytes().to_vec(), "alignment"),
        (header + 48, 3u64.to_le_bytes().to_vec(), "alignment"),
        (header + 56, 8u64.to_le_bytes().to_vec(), "entry size"),
        (header + 4, 8u32.to_le_bytes().to_vec(), "PROGBITS"),
        (data, 0xffff_ffffu32.to_le_bytes().to_vec(), "DWARF64"),
        (data, 0x7fff_fffcu32.to_le_bytes().to_vec(), "truncated"),
        (data, 5u32.to_le_bytes().to_vec(), "unaligned"),
        (data, 0u32.to_le_bytes().to_vec(), "terminator"),
        (data + 8, vec![2], "version"),
        (data + 10, vec![b'X'], "augmentation"),
        (data + 16, vec![0x9b], "encoded pointer"),
        (data + 16, vec![0x11], "encoded pointer"),
        (data + 16, vec![0x3b], "encoded pointer"),
        (data + 15, vec![0], "truncated"),
        (data + 12, vec![0], "code alignment"),
        (fde + 4, 1u32.to_le_bytes().to_vec(), "CIE reference"),
        (fde + 16, vec![127], "truncated"),
        (rela, 0u64.to_le_bytes().to_vec(), "pointer field"),
        (rela + 8, 1u32.to_le_bytes().to_vec(), "disagrees"),
        (rela, u64::MAX.to_le_bytes().to_vec(), "outside"),
        (rela + 12, 0xffffu32.to_le_bytes().to_vec(), "symbol index"),
    ];
    // The PC relocation remains valid, but a range extending outside .text fails.
    cases.push((data + pc + 4, u32::MAX.to_le_bytes().to_vec(), "unwind"));
    for (at, replacement, message) in cases {
        let mut bad = valid.clone();
        bad[at..at + replacement.len()].copy_from_slice(&replacement);
        let Err(err) = link(&[bad]) else {
            panic!("accepted malformed input at {at}; expected {message}")
        };
        assert!(err.contains(message), "at {at}: {err}; expected {message}");
    }
    let mut wrong_name = valid;
    let file = object::File::parse(wrong_name.as_slice()).unwrap();
    let shstr = file
        .section_by_name(".shstrtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let name = shstr + u32_at(&wrong_name, header) as usize;
    wrong_name[name + 1] = b'x';
    assert!(
        link(&[wrong_name])
            .unwrap_err()
            .contains(".eh_frame requires")
    );
}

#[test]
fn merge_absorbs_terminators_and_padding_and_rebases_local_section_targets() {
    let empty = object(".section .eh_frame,\"a\",@progbits\n.p2align 2\n.long 0\n");
    let frame = |name| {
        object(&format!(
            ".text\n.globl {name}\n{name}: ret\n.section .eh_frame,\"a\",@progbits\n.p2align 6\n.long 12\n.long 0\n.byte 1,0,1,120,16,0,0,0\nlocal_fde: .long 20\n.long 20\n.quad {name}\n.quad 1\n.long 0,0\n.data\n.globl {name}_pointer\n{name}_pointer: .quad local_fde+8\n"
        ))
    };
    for inputs in [
        vec![
            empty.clone(),
            frame("_start"),
            empty.clone(),
            frame("second"),
            empty.clone(),
        ],
        vec![frame("_start"), empty, frame("second")],
    ] {
        let exe = link(&inputs).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let sections: Vec<_> = file
            .sections()
            .filter(|s| s.name().unwrap() == ".eh_frame")
            .collect();
        assert_eq!(sections.len(), 1);
        let section = &sections[0];
        let hdr = file.section_by_name(".eh_frame_hdr").unwrap();
        assert_eq!(u32_at(hdr.data().unwrap(), 8), 2);
        let mut at = 0;
        let mut fdes = 0;
        while at < section.size() as usize {
            let data = section.data().unwrap();
            let len = u32_at(data, at) as usize;
            if len == 0 {
                break;
            }
            if u32_at(data, at + 4) != 0 {
                let pc = u64_at(data, at + 8);
                let name = if pc == symbol(&file, "_start") {
                    "_start"
                } else {
                    "second"
                };
                let pointer = symbol(&file, &format!("{name}_pointer"));
                assert_eq!(
                    u64_at(&exe.bytes, (pointer - 0x400000) as usize),
                    section.address() + at as u64 + 8
                );
                fdes += 1;
            }
            at += 4 + len;
        }
        assert_eq!(fdes, 2);
        let locals: Vec<_> = file
            .symbols()
            .filter(|s| s.name().unwrap() == "local_fde")
            .collect();
        assert_eq!(locals.len(), 2);
        assert!(
            locals
                .iter()
                .all(|s| s.section_index() == Some(section.index()))
        );
    }
}

#[test]
fn no_unwind_header_for_objects_without_frames_and_no_input_header_passthrough() {
    let exe = link(&[object(".text\n.globl _start\n_start:\nret\n")]).unwrap();
    assert!(
        object::File::parse(exe.bytes.as_slice())
            .unwrap()
            .section_by_name(".eh_frame_hdr")
            .is_none()
    );
    let bad = object(
        ".text\n.globl _start\n_start:\nret\n.section .eh_frame_hdr,\"a\",@progbits\n.long 0\n",
    );
    assert!(link(&[bad]).unwrap_err().contains("linker generates"));
}

#[test]
fn retains_personality_lsda_and_multiple_cies() {
    let input = object(
        ".text\n.globl _start\n_start:\n.cfi_startproc\n.cfi_personality 155,personality_ref\n.cfi_lsda 27,lsda\nret\n.cfi_endproc\nother:\n.cfi_startproc\nret\n.cfi_endproc\npersonality:\nret\n.data\n.p2align 3\npersonality_ref:\n.quad personality\n.section .gcc_except_table,\"a\",@progbits\nlsda:\n.byte 0\n",
    );
    let exe = link(std::slice::from_ref(&input)).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        u32_at(
            file.section_by_name(".eh_frame_hdr")
                .unwrap()
                .data()
                .unwrap(),
            8
        ),
        2
    );
    assert!(file.section_by_name(".gcc_except_table").is_some());
    let original = object::File::parse(input.as_slice()).unwrap();
    let section = original.section_by_name(".eh_frame").unwrap();
    let out = file.section_by_name(".eh_frame").unwrap();
    let mut expected = section.data().unwrap().to_vec();
    for (offset, relocation) in section.relocations() {
        let object::RelocationTarget::Symbol(index) = relocation.target() else {
            panic!()
        };
        let target = original.symbol_by_index(index).unwrap();
        let target_section = original
            .section_by_index(target.section_index().unwrap())
            .unwrap();
        let output_section = file
            .section_by_name(target_section.name().unwrap())
            .unwrap();
        let expected_target = (output_section.address() as i64
            + target.address() as i64
            + relocation.addend()) as u64;
        let relocated = (out.address() as i64
            + offset as i64
            + i64::from(u32_at(out.data().unwrap(), offset as usize) as i32))
            as u64;
        assert_eq!(relocated, expected_target);
        let at = offset as usize;
        expected[at..at + 4].copy_from_slice(&out.data().unwrap()[at..at + 4]);
    }
    assert_eq!(expected, out.data().unwrap());
    let (_, _, rela) = locations(&input);
    let mut duplicate = input.clone();
    let offset = u64_at(&input, rela);
    put64(&mut duplicate, rela + 24, offset);
    assert!(
        link(&[duplicate])
            .unwrap_err()
            .contains("duplicate .eh_frame")
    );
    let code_relocations: Vec<_> = section
        .relocations()
        .enumerate()
        .filter_map(|(i, (_, r))| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                return None;
            };
            let sym = original.symbol_by_index(index).unwrap();
            let target = original
                .section_by_index(sym.section_index().unwrap())
                .unwrap();
            (target.name().unwrap() == ".text").then_some(rela + i * 24)
        })
        .collect();
    assert_eq!(code_relocations.len(), 2);
    let mut same_pc = input.clone();
    let target = same_pc[code_relocations[0] + 8..code_relocations[0] + 24].to_vec();
    same_pc[code_relocations[1] + 8..code_relocations[1] + 24].copy_from_slice(&target);
    let err = link(&[same_pc]).unwrap_err();
    assert!(err.contains("duplicate unwind FDE"), "{err}");
}

#[test]
fn absolute_pointers_cie_versions_and_terminators_are_supported() {
    for version in [1, 3, 4] {
        // Empty augmentation: default absolute 64-bit FDE address and range.
        let address_size = if version == 4 { ".byte 8,0\n" } else { "" };
        let input = object(&format!(
            ".text\n.globl _start\n_start:\nret\n.section .eh_frame,\"a\"\n.p2align 3\n.long 12\n.long 0\n.byte {version},0\n{address_size}.byte 1,120,16\n.balign 16\n.long 20\n.long 20\n.quad _start\n.quad 1\n.long 0\n.long 0\n"
        ));
        let exe = link(std::slice::from_ref(&input)).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let data = file.section_by_name(".eh_frame").unwrap().data().unwrap();
        assert_eq!(u64_at(data, 24), symbol(&file, "_start"));
        assert_eq!(&data[40..], &[0; 8]);
        let (_, _, rela) = locations(&input);
        let mut overflow = input;
        put64(&mut overflow, rela + 16, u64::MAX - 0x400000);
        assert!(
            link(&[overflow])
                .unwrap_err()
                .contains("outside executable")
        );
    }
}

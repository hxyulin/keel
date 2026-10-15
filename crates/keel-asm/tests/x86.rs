use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::{Item, Program, x86::Instruction};
use object::{Object, ObjectSection, ObjectSymbol};

fn text(source: &str) -> Vec<u8> {
    let bytes = assemble_object(source).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    file.section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn rex_sib_displacements_and_immediates() {
    assert_eq!(
        text(
            "mov r8, r9; mov spl, al; mov ah, bl; mov rax, -1; movabs r11, 0x123456789abcdef0\nmov rax, [r12 + 8*r13 - 16]"
        ),
        [
            0x4d, 0x89, 0xc8, 0x40, 0x88, 0xc4, 0x88, 0xdc, 0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff,
            0xff, 0x49, 0xbb, 0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12, 0x4b, 0x8b, 0x44,
            0xec, 0xf0
        ]
    );
}

#[test]
fn near_branches_and_symbol_differences() {
    let bytes = text("start: jmp done\nje start\n.long done-start\ndone: ret");
    assert_eq!(
        bytes,
        [
            0xe9, 10, 0, 0, 0, 0x0f, 0x84, 0xf5, 0xff, 0xff, 0xff, 15, 0, 0, 0, 0xc3
        ]
    );
}

#[test]
fn elf_sections_symbols_and_relocations() {
    let bytes = assemble_object(
        r#"
.intel_syntax noprefix
.text
.globl function
.type function,@function
function:
call external@PLT
mov r8, [rip + external@GOTPCREL]
lea rax, [rip + data + 7]
mov rax, qword ptr fs:0
lea rax, [rax + tls@tpoff]
.size function, .-function
.section .data,"aw",@progbits
.type data,@object
data: .quad external+9
.size data,8
.section .tbss,"awT",@nobits
.type tls,@object
tls: .zero 16
.section .note.GNU-stack,"",@progbits
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(file.architecture(), object::Architecture::X86_64);
    let f = file.symbols().find(|s| s.name() == Ok("function")).unwrap();
    assert!(f.is_global());
    assert_eq!(f.kind(), object::SymbolKind::Text);
    assert_eq!(f.size(), 35);
    let sec = file.section_by_name(".text").unwrap();
    let rel: Vec<_> = sec
        .relocations()
        .map(|(o, r)| (o, r.flags(), r.addend()))
        .collect();
    assert_eq!(
        rel,
        vec![
            (1, object::RelocationFlags::Elf { r_type: 4 }, -4),
            (8, object::RelocationFlags::Elf { r_type: 9 }, -4),
            (15, object::RelocationFlags::Elf { r_type: 2 }, 3),
            (31, object::RelocationFlags::Elf { r_type: 23 }, 0)
        ]
    );
    let tls = file.symbols().find(|s| s.name() == Ok("tls")).unwrap();
    assert_eq!(tls.kind(), object::SymbolKind::Tls);
    assert_eq!(file.section_by_name(".tbss").unwrap().size(), 16);
    assert_eq!(
        file.section_by_name(".data")
            .unwrap()
            .relocations()
            .next()
            .unwrap()
            .1
            .addend(),
        9
    );
}

#[test]
fn rejects_unsupported_and_mixed_architectures() {
    for src in [
        "vaddps ymm0,ymm1,ymm2",
        "mov rax, ah",
        "mov ah, r8b",
        "lock add eax,ecx",
        "mov rax, [rsp + rsp*2]",
        ".unknown 42",
        ".cfi_startproc",
        ".section .text,\"axG\",@progbits",
        "mov [rax], 3",
        "shl rax, dl",
    ] {
        assert!(assemble_object(src).is_err(), "{src}");
    }
    let mut p = Program::new();
    p.x86_instruction(Instruction::parse_intel("ret").unwrap());
    assert!(keel_asm::assemble_program_object(&p, Default::default()).is_err());
    p.items.push(Item::Branch {
        link: false,
        target: "arm".into(),
    });
    assert!(assemble_program_object(&p).is_err());
}

#[test]
fn cfi_and_line_tables_are_present() {
    let bytes=assemble_object(".file 1 \"test.c\"\n.text\nf:\n.cfi_startproc\n.loc 1 3 1\npush rbp\n.cfi_def_cfa_offset 16\n.cfi_offset rbp,-16\nmov rbp,rsp\n.cfi_def_cfa_register rbp\n.loc 1 4 2 prologue_end\nret\n.cfi_endproc").unwrap();
    let f = object::File::parse(&*bytes).unwrap();
    let eh = f.section_by_name(".eh_frame").unwrap();
    assert_eq!(eh.relocations().count(), 1);
    let line = f.section_by_name(".debug_line").unwrap();
    assert_eq!(line.relocations().count(), 1);
    assert!(line.size() > 40);
}

#[test]
fn instruction_bytes_match_reference_assemblers() {
    use std::process::Command;
    let temp = std::env::temp_dir().join(format!("keel-x86-reference-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    // Branches and accumulator immediates have valid alternative encodings;
    // this matrix chooses forms with a shared canonical encoding.
    let cases = [
        "mov r8,r9",
        "mov spl,al",
        "mov ah,bl",
        "mov ax,0xabcd",
        "mov rax,-1",
        "movabs r11,0x123456789abcdef0",
        "mov r9,[r12+8*r13-16]",
        "mov [rbp],r8b",
        "mov rdx,[r13]",
        "mov eax,[r12]",
        "mov byte ptr [rsp+8],-1",
        "mov rax,qword ptr fs:0",
        "lea r8,[r9+r10*4+128]",
        "movzx r9d,byte ptr [r12]",
        "movsx rax,cx",
        "movsxd r11,r8d",
        "push r12",
        "pop r15",
        "add r9d,ecx",
        "sub r8,127",
        "adc r9,r11",
        "sbb r8w,ax",
        "test dil,sil",
        "cmp r9,0x12345678",
        "neg r9",
        "not byte ptr [r12]",
        "mul r8b",
        "imul r9,rcx",
        "imul r9,r10,123",
        "imul r9d,r10d,2654404609",
        "idiv r8",
        "cqo",
        "cdq",
        "shl r9,cl",
        "shr byte ptr [r12],3",
        "sar r9,1",
        "rol r9w,8",
        "shld r9,r10,cl",
        "shrd r9,r10,4",
        "bsr r9,r10",
        "bsf r9d,r10d",
        "popcnt r9,r10",
        "bswap r9",
        "btc r9,63",
        "btr r9,r10",
        "setne r9b",
        "cmovae r9,r10",
        "xchg [r12],r9",
        "lock xadd qword ptr [r12],r9",
        "lock cmpxchg byte ptr [r12],r9b",
        "mfence",
        "pause",
        "ud2",
        "ret",
        "movsd xmm9,[r12]",
        "movss [r12],xmm9",
        "movaps [r12],xmm9",
        "movaps xmm9,xmm10",
        "movd xmm9,r10d",
        "movq r10,xmm9",
        "movq xmm9,r10",
        "movq xmm9,xmm10",
        "addsd xmm9,xmm10",
        "subss xmm9,xmm10",
        "mulsd xmm9,xmm10",
        "divss xmm9,xmm10",
        "sqrtsd xmm9,xmm10",
        "minsd xmm9,xmm10",
        "maxss xmm9,xmm10",
        "ucomisd xmm9,xmm10",
        "xorps xmm9,xmm10",
        "andpd xmm9,xmm10",
        "orps xmm9,xmm10",
        "roundsd xmm9,xmm10,9",
        "roundss xmm9,xmm10,11",
        "cvtsi2sd xmm9,r10",
        "cvtsi2ss xmm9,r10d",
        "cvttsd2si r9,xmm10",
        "cvttss2si r9d,xmm10",
        "cvtss2sd xmm9,xmm10",
        "cvtsd2ss xmm9,xmm10",
    ];
    for (n, case) in cases.iter().enumerate() {
        let source = format!(".intel_syntax noprefix\n.text\n{case}\n");
        let input = temp.join("input.s");
        let output = temp.join("clang.o");
        std::fs::write(&input, &source).unwrap();
        let result = Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .expect("Clang is required for differential tests");
        assert!(
            result.status.success(),
            "case {n}: {case}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reference = std::fs::read(&output).unwrap();
        let reference = object::File::parse(&*reference).unwrap();
        assert_eq!(
            text(&source),
            reference.section_by_name(".text").unwrap().data().unwrap(),
            "case {n}: {case}"
        );
        #[cfg(target_os = "linux")]
        {
            let gnu = temp.join("gnu.o");
            let result = Command::new("as")
                .args(["--64", "-o"])
                .arg(&gnu)
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "GNU as {case}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let bytes = std::fs::read(gnu).unwrap();
            let file = object::File::parse(&*bytes).unwrap();
            assert_eq!(
                text(&source),
                file.section_by_name(".text").unwrap().data().unwrap(),
                "GNU as case {n}: {case}"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn relocation_semantics_match_reference_assemblers() {
    use std::process::Command;
    let source = r#".intel_syntax noprefix
.set absolute,0x1234
.text
lea rdx,[rip+absolute]
mov rcx,[rip+absolute@GOTPCREL]
lea rsi,[rip+forward_absolute]
jmp forward_absolute
.set forward_absolute,0x5678
call external@PLT
jmp external@PLT
mov r9,[rip+external@GOTPCREL+8]
mov r10,[rip+tls@gottpoff]
lea rax,[rax+tls@tpoff]
lea r11,[rip+data+3]
mov dword ptr [rip+data],42
.data
data: .quad external+7
.long external+3
.short external+2
.byte external+1
.quad tls@dtpoff
.section .tbss,"awT",@nobits
tls: .zero 8
"#;
    let temp = std::env::temp_dir().join(format!("keel-x86-relocations-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    let output = temp.join("clang.o");
    std::fs::write(&input, source).unwrap();
    let result = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-Wa,-mrelax-relocations=no",
            "-c",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fn relocations(
        file: &object::File<'_>,
        section: &str,
    ) -> Vec<(u64, object::RelocationFlags, String, i64)> {
        let mut entries: Vec<_> = file
            .section_by_name(section)
            .unwrap()
            .relocations()
            .map(|(at, r)| {
                let id = match r.target() {
                    object::RelocationTarget::Symbol(id) => id,
                    object::RelocationTarget::Absolute => {
                        return (at, r.flags(), "*ABS*".into(), r.addend());
                    }
                    target => panic!("unexpected relocation target: {target:?}"),
                };
                let symbol = file.symbol_by_index(id).unwrap();
                let (target, addend) = if let Some(sec) = symbol.section_index() {
                    (
                        file.section_by_index(sec)
                            .unwrap()
                            .name()
                            .unwrap()
                            .to_string(),
                        r.addend() + symbol.address() as i64,
                    )
                } else if symbol.section() == object::SymbolSection::Absolute {
                    ("*ABS*".into(), r.addend() + symbol.address() as i64)
                } else {
                    (symbol.name().unwrap().to_string(), r.addend())
                };
                (at, r.flags(), target, addend)
            })
            .collect();
        // There is no PLT entry for an absolute symbol: GNU as uses PC32
        // where Clang uses PLT32. Both mean S + A - P for this target.
        for (_, flags, target, _) in &mut entries {
            if target == "*ABS*" && *flags == (object::RelocationFlags::Elf { r_type: 4 }) {
                *flags = object::RelocationFlags::Elf { r_type: 2 };
            }
        }
        entries.sort_by_key(|entry| entry.0);
        entries
    }
    let ours = assemble_object(source).unwrap();
    let ours = object::File::parse(&*ours).unwrap();
    let reference = std::fs::read(output).unwrap();
    let reference = object::File::parse(&*reference).unwrap();
    for section in [".text", ".data"] {
        assert_eq!(
            ours.section_by_name(section).unwrap().data().unwrap(),
            reference.section_by_name(section).unwrap().data().unwrap(),
            "{section} bytes"
        );
        assert_eq!(
            relocations(&ours, section),
            relocations(&reference, section),
            "{section} relocations"
        );
    }
    #[cfg(target_os = "linux")]
    {
        // GNU as rejects absolute symbols with @GOTPCREL; Clang accepts this
        // form and is the reference for it above. Compare GNU's shared syntax.
        let gnu_source = source.replace("mov rcx,[rip+absolute@GOTPCREL]\n", "");
        std::fs::write(&input, &gnu_source).unwrap();
        let ours_bytes = assemble_object(&gnu_source).unwrap();
        let ours = object::File::parse(&*ours_bytes).unwrap();
        let gnu = temp.join("gnu.o");
        let result = Command::new("as")
            .args(["--64", "-mrelax-relocations=no", "-o"])
            .arg(&gnu)
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "GNU as: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = std::fs::read(gnu).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        for section in [".text", ".data"] {
            assert_eq!(
                ours.section_by_name(section).unwrap().data().unwrap(),
                file.section_by_name(section).unwrap().data().unwrap(),
                "GNU as {section} bytes"
            );
            assert_eq!(
                relocations(&ours, section),
                relocations(&file, section),
                "GNU as {section} relocations"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn executable_alignment_uses_nops_and_explicit_zero_fill_is_preserved() {
    let bytes = assemble_object(
        ".text\nnop\n.p2align 4\nret\n.p2align 5,0\n.data\n.byte 1\n.p2align 4\n.byte 2\n",
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let code = file.section_by_name(".text").unwrap().data().unwrap();
    assert_eq!(
        &code[..16],
        &[0x90; 16],
        "fall-through padding must be executable NOPs"
    );
    assert_eq!(code[16], 0xc3);
    assert!(
        code[17..].iter().all(|b| *b == 0),
        "explicit fill remains literal"
    );
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert_eq!(data[0], 1);
    assert!(data[1..16].iter().all(|b| *b == 0));
    assert_eq!(data[16], 2);
    let skipped = assemble_object(".text\n.byte 1\n.p2align 4,,7\n").unwrap();
    let skipped = object::File::parse(&*skipped).unwrap();
    assert_eq!(skipped.section_by_name(".text").unwrap().align(), 16);
    assert_eq!(skipped.section_by_name(".text").unwrap().size(), 1);
}

#[test]
fn absolute_symbols_fold_rip_displacements_but_keep_got_relocations() {
    let bytes = assemble_object(".set absolute,0x1234\n.text\nlea rax,[rip+absolute]\nmov rax,[rip+absolute@GOTPCREL]\n.quad absolute\n").unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let absolute = file.symbols().find(|s| s.name() == Ok("absolute")).unwrap();
    assert_eq!(absolute.section(), object::SymbolSection::Absolute);
    let code = file.section_by_name(".text").unwrap();
    let rel: Vec<_> = code.relocations().collect();
    assert_eq!(rel.len(), 1, "GOT references must reach the linker");
    assert_eq!(rel[0].0, 10);
    assert_eq!(
        rel[0].1.target(),
        object::RelocationTarget::Symbol(absolute.index())
    );
    assert_eq!(rel[0].1.addend(), -4);
    assert_eq!(&code.data().unwrap()[3..7], &0x1234u32.to_le_bytes());
    assert_eq!(&code.data().unwrap()[14..], &0x1234u64.to_le_bytes());
    for op in ["jmp", "call", "je"] {
        assert!(assemble_object(&format!(".set absolute,42\n{op} absolute")).is_err());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn alignment_and_absolute_operands_execute_like_clang() {
    use std::process::Command;
    let temp = std::env::temp_dir().join(format!(
        "keel-x86-executable-alignment-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let source = r#".intel_syntax noprefix
.set displacement,16
.text
.globl main
.type main,@function
main:
lea rax,[rip+displacement]
.Lnext:
lea rcx,[rip+.Lnext+16]
cmp rax,rcx
setne al
mov rdx,[rip+displacement@GOTPCREL]
cmp rdx,16
setne dl
or al,dl
movzx eax,al
.p2align 6
ret
.section .note.GNU-stack,"",@progbits
"#;
    let asm = temp.join("input.s");
    std::fs::write(&asm, source).unwrap();
    let ours = temp.join("ours.o");
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let reference = temp.join("reference.o");
    let output = Command::new("clang")
        .args(["-Wa,-mrelax-relocations=no", "-c"])
        .arg(&asm)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (name, object) in [("ours", ours), ("reference", reference)] {
        let exe = temp.join(name);
        // GNU ld's GOT relaxation rejects absolute targets in PIE output;
        // retain the GOT load in both objects for this execution regression.
        let result = Command::new("clang")
            .arg("-Wl,--no-relax")
            .arg(object)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            Command::new(&exe).status().unwrap().code(),
            Some(0),
            "{name}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

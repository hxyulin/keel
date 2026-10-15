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
fn short_branches_and_symbol_differences() {
    let bytes = text("start: jmp done\nje start\n.long done-start\ndone: ret");
    assert_eq!(bytes, [0xeb, 6, 0x74, 0xfc, 8, 0, 0, 0, 0xc3]);
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
        "add al,17",
        "or ax,128",
        "adc eax,128",
        "sbb rax,128",
        "and eax,65535",
        "sub rax,128",
        "xor al,255",
        "cmp eax,1000",
        "test al,1",
        "test ax,1",
        "test eax,1",
        "test rax,1",
        "add eax,127",
        "sub rax,-128",
        "cmp ax,127",
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
test rax,offset external
call external@PLT
jmp external@PLT
mov r9,[rip+external@GOTPCREL+8]
mov r10,[rip+tls@gottpoff]
lea rax,[rax+tls@tpoff]
lea r11,[rip+.Ldata+3]
mov dword ptr [rip+.Ldata],42
.data
.set .Labs_before,42
.long .Labs_before-.
.long .Labs_forward-.
.set .Labs_forward,43
.set .Labs_alias,.Labs_later
.long .Labs_alias-.
.set .Labs_later,44
.Ldata: .quad external+7
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

#[test]
fn rel8_boundaries_and_binding_sensitive_targets() {
    for op in ["jmp", "je", "jne", "jg"] {
        for gap in [126, 127, 128, 129] {
            let code = text(&format!("{op} target\n.zero {gap}\ntarget: ret"));
            assert_eq!(
                code[0] == 0xeb || (0x70..=0x7f).contains(&code[0]),
                gap <= 127,
                "{op} forward {gap}"
            );
            if gap <= 127 {
                assert_eq!(code[1], gap as u8);
            }
        }
        for gap in [125, 126, 127, 128] {
            let code = text(&format!("target: .zero {gap}\n{op} target"));
            let is_short = code[gap] == 0xeb || (0x70..=0x7f).contains(&code[gap]);
            assert_eq!(is_short, gap <= 126, "{op} backward {gap}");
            if is_short {
                assert_eq!(code[gap + 1] as i8 as i32, -(gap as i32) - 2);
            }
        }
    }
    let bytes = assemble_object("jmp external\nje data\njmp public\nje weak\njmp alias\n.globl public\n.weak weak\npublic: ret\nweak: ret\n.set alias,public\n.data\ndata: .byte 0").unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let code = file.section_by_name(".text").unwrap();
    assert_eq!(code.relocations().count(), 5);
    assert_eq!(&code.data().unwrap()[..2], &[0xe9, 0]);
    // Resolved same-section targets retain signed rel32 range diagnostics.
    assert!(
        assemble_object("jmp far\n.zero 0\n.set far,.Lstart+0x80000010\n.Lstart: ret").is_err()
    );
}

#[test]
fn cascading_widening_rechecks_alignment_and_all_offsets() {
    // First branch widens, pushing the backward conditional beyond -128.
    let code = text("start: jmp far\n.zero 124\nje start\n.zero 128\nfar: ret");
    assert_eq!(code[0], 0xe9);
    assert_eq!(&code[129..131], &[0x0f, 0x84]);
    // Widening the far branch disables a max-skip alignment, taking the
    // initially short target branch out of range on the following pass.
    let bytes = assemble_object(".globl f\n.type f,@function\nf: jmp target\njmp far\n.zero 123\n.p2align 7,0xcc,4\ntarget: .long target-f\n.zero 128\nfar: ret\n.size f,.-f\n.set alias,target\n.data\n.quad alias\n.long far-f").unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let code = file.section_by_name(".text").unwrap().data().unwrap();
    assert_eq!(code[0], 0xe9);
    assert_eq!(code[5], 0xe9);
    let target = file
        .symbols()
        .find(|s| s.name() == Ok("target"))
        .unwrap()
        .address() as usize;
    assert_eq!(
        i32::from_le_bytes(code[target..target + 4].try_into().unwrap()) as usize,
        target
    );
    let f = file.symbols().find(|s| s.name() == Ok("f")).unwrap();
    assert_eq!(f.size() as usize, code.len());
    let alias = file.symbols().find(|s| s.name() == Ok("alias")).unwrap();
    assert_eq!(alias.address() as usize, target);
}

#[test]
fn temporary_symbols_use_section_relocations_but_preserve_identity() {
    let bytes = assemble_object(
        r#"
.text
.set .Lconstant,42
.quad .Lconstant
lea rax,[rip+.Lforward_absolute]
.set .Lforward_absolute,123
.zero 3
.Lcode: ret
.set .Lalias,.Lcode+1
.globl .Lglobal
.Lglobal: ret
.weak .Lweak
.Lweak: ret
mov rax,[rip+.Lgot@GOTPCREL]
mov rax,[rip+.Ltls@gottpoff]
mov rax,[rip+.Lgot_alias@GOTPCREL]
mov rax,[rip+.Ltls_alias@gottpoff]
.set .Lgot_alias,.Lgot
.set .Ltls_alias,.Ltls
lea rax,[rax+.Ltls@tpoff]
.data
.zero 7
.Ldata: .quad .Lcode+9
.quad .Lalias+2
.long .Ldata-.
.Lgot: .quad 0
.section .tbss,"awT",@nobits
.Ltls: .zero 8
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    for name in [".Lcode", ".Lalias", ".Ldata", ".Lconstant"] {
        assert!(!file.symbols().any(|s| s.name() == Ok(name)), "{name}");
    }
    for name in [".Lglobal", ".Lweak", ".Lgot", ".Ltls", ".Lforward_absolute"] {
        assert!(file.symbols().any(|s| s.name() == Ok(name)), "{name}");
    }
    let code = file.section_by_name(".text").unwrap();
    let targets: Vec<_> = code
        .relocations()
        .map(|(_, r)| {
            let object::RelocationTarget::Symbol(id) = r.target() else {
                panic!()
            };
            file.symbol_by_index(id)
                .unwrap()
                .name()
                .unwrap()
                .to_string()
        })
        .collect();
    for name in [
        ".Lgot",
        ".Ltls",
        ".Lgot_alias",
        ".Ltls_alias",
        ".Lforward_absolute",
    ] {
        assert!(
            targets.iter().any(|target| target == name),
            "{name}: {targets:?}"
        );
    }
    let data = file.section_by_name(".data").unwrap();
    let rel: Vec<_> = data.relocations().collect();
    assert_eq!(rel.len(), 2);
    for ((offset, r), addend) in rel.iter().zip([27, 21]) {
        assert!([7, 15].contains(offset));
        assert_eq!(r.addend(), addend);
        let object::RelocationTarget::Symbol(id) = r.target() else {
            panic!()
        };
        let symbol = file.symbol_by_index(id).unwrap();
        assert_eq!(symbol.kind(), object::SymbolKind::Section);
        assert_eq!(
            symbol.section_index(),
            file.section_by_name(".text").unwrap().index().into()
        );
    }
    assert_eq!(&data.data().unwrap()[23..27], &(-16i32).to_le_bytes());
}

#[test]
fn branch_layout_matches_clang_and_gnu_with_aliases_and_alignment() {
    use std::process::Command;
    let temp = std::env::temp_dir().join(format!("keel-x86-relax-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let mut cases = Vec::new();
    for gap in [0, 125, 126, 127, 128, 129, 254] {
        for op in ["jmp", "je", "jg"] {
            cases.push(format!("start: {op} alias\n.zero {gap}\ntarget: {op} start\n.set alias,target\n.long target-start\n"));
        }
    }
    for max in [3, 7, 15, 127] {
        // LLVM and GAS may choose different valid fixed points for max-skip;
        // these cases have unambiguous branch distances.
        cases.push(format!("start: jmp target\n.zero 32\n.p2align 7,0xcc,{max}\ntarget: jne start\n.long target-start\n"));
    }
    for (index, case) in cases.iter().enumerate() {
        let source = format!(".intel_syntax noprefix\n.text\n{case}");
        let input = temp.join("input.s");
        std::fs::write(&input, &source).unwrap();
        let mut commands = vec![("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"])];
        if cfg!(target_os = "linux") {
            commands.push(("as", vec!["--64"]));
        }
        for (tool, args) in commands {
            let output = temp.join(format!("{tool}.o"));
            let result = Command::new(tool)
                .args(args)
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{tool}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let reference = std::fs::read(&output).unwrap();
            let reference = object::File::parse(&*reference).unwrap();
            assert_eq!(
                text(&source),
                reference.section_by_name(".text").unwrap().data().unwrap(),
                "{tool} case {index}: {source}"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn layout_dependent_absolute_operands_are_reencoded() {
    // The first branch widening changes an already-defined absolute alias from
    // 127 to 130: PUSH must switch from imm8 to imm32 on the next pass.
    let code = text(
        "start: jmp far\n.zero 125\nend: .set distance,end-start\npush distance\n.zero 128\nfar: ret",
    );
    assert_eq!(code[0], 0xe9);
    assert_eq!(&code[130..135], &[0x68, 130, 0, 0, 0]);
    // A later alias remains a symbolic immediate, even when its final value
    // fits an imm8. This is the before-definition versus forward-alias rule.
    let code = text("push forward\n.set forward,1\n");
    assert_eq!(code, [0x68, 1, 0, 0, 0]);
}

#[test]
fn got_and_tls_aliases_link_and_execute_like_clang() {
    use std::process::Command;
    let source = r#".intel_syntax noprefix
.text
.globl main
.type main,@function
main:
mov rax,[rip+.Lgot_alias@GOTPCREL]
cmp qword ptr [rax],9
jne .Lbad
mov eax,dword ptr fs:.Ltls_alias@tpoff
cmp eax,42
jne .Lbad
xor eax,eax
ret
.Lbad: mov eax,1
ret
.data
.Lvalue: .quad 9
.set .Lgot_alias,.Lvalue
.section .tdata,"awT",@progbits
.Ltls: .long 42
.set .Ltls_alias,.Ltls
.section .note.GNU-stack,"",@progbits
"#;
    let temp = std::env::temp_dir().join(format!("keel-x86-tls-alias-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    let ours = temp.join("ours.o");
    let reference = temp.join("clang.o");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let output = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in [&ours, &reference] {
        let bytes = std::fs::read(path).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        let code = file.section_by_name(".text").unwrap();
        let types: Vec<_> = code.relocations().map(|(_, r)| r.flags()).collect();
        assert!(types.contains(&object::RelocationFlags::Elf { r_type: 23 }));
        assert!(
            types
                .iter()
                .any(|r| matches!(r, object::RelocationFlags::Elf { r_type: 9 | 42 }))
        );
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for (name, obj) in [("ours", &ours), ("clang", &reference)] {
        let exe = temp.join(name);
        let output = Command::new("clang")
            .arg(obj)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            Command::new(&exe).status().unwrap().code(),
            Some(0),
            "{name}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn temporary_absolute_symbols_survive_converted_pc_relative_differences() {
    let bytes = assemble_object(
        ".data\n.set .Labs_before,42\n.long .Labs_before-.\n.long .Labs_forward-.\n.set .Labs_forward,43\n.set .Labs_alias,.Labs_later\n.long .Labs_alias-.\n.set .Labs_later,44\n.set .Lresolved_only,73\n.set .Lresolved_minus,20\n.long .Lresolved_only-.Lresolved_minus\n",
    ).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let data = file.section_by_name(".data").unwrap();
    assert_eq!(&data.data().unwrap()[12..], &53u32.to_le_bytes());
    let relocations: Vec<_> = data.relocations().collect();
    assert_eq!(relocations.len(), 3);
    for ((offset, relocation), (name, value)) in relocations.iter().zip([
        (".Labs_before", 42),
        (".Labs_forward", 43),
        (".Labs_alias", 44),
    ]) {
        assert!([0, 4, 8].contains(offset));
        assert_eq!(
            relocation.flags(),
            object::RelocationFlags::Elf { r_type: 2 }
        );
        assert_eq!(relocation.addend(), 0);
        let object::RelocationTarget::Symbol(id) = relocation.target() else {
            panic!("absolute symbol identity missing")
        };
        let symbol = file.symbol_by_index(id).unwrap();
        assert_eq!(symbol.name().unwrap(), name);
        assert_eq!(symbol.section(), object::SymbolSection::Absolute);
        assert_eq!(symbol.address(), value);
    }
    for name in [".Lresolved_only", ".Lresolved_minus"] {
        assert!(!file.symbols().any(|symbol| symbol.name() == Ok(name)));
    }
}

#[test]
fn parser_fast_paths_keep_case_quotes_and_spilled_lists() {
    assert_eq!(
        text("ADD RAX, RCX\nMOVAPS XMM15, XMM14"),
        text("add rax, rcx\nmovaps xmm15, xmm14")
    );
    let source = ".data\n.byte 1,2,3,4,5,6,7\n.asciz \"a,b;:#//\",\"escaped\\\";quote\"\n";
    let bytes = assemble_object(source).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".data").unwrap().data().unwrap(),
        b"\x01\x02\x03\x04\x05\x06\x07a,b;:#//\0escaped\";quote\0"
    );
    use keel_asm_ir::x86::Register;
    assert_eq!(Register::parse("r0008D"), Register::parse("r8d"));
    assert_eq!(Register::parse("XMM0015"), Register::parse("xmm15"));
    assert!(Register::parse("rax ").is_none());
}

#[test]
fn elf_output_is_deterministic_across_symbol_hash_seeds() {
    let source = ".text\n.globl f\nf: jne .Lend\nmov eax,1\n.p2align 4\n.Lend: ret\n.size f,.-f\n.data\n.quad f\n.set alias,f\n.quad alias\n.comm common,16,8\n";
    let expected = assemble_object(source).unwrap();
    for _ in 0..8 {
        assert_eq!(assemble_object(source).unwrap(), expected);
    }
}

use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::Program;
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::process::Command;

fn snapshot(bytes: &[u8]) -> Vec<(String, u64, u64, Vec<u8>, Vec<(u64, u32, String, i64)>)> {
    let file = object::File::parse(bytes).unwrap();
    let mut result = Vec::new();
    for section in file.sections() {
        let name = section.name().unwrap();
        if ![
            ".text",
            ".data",
            ".bss",
            "__ex_table",
            ".altinstructions",
            ".altinstr_replacement",
        ]
        .contains(&name)
        {
            continue;
        }
        let relocations = section
            .relocations()
            .map(|(offset, r)| {
                let object::RelocationFlags::Elf { r_type } = r.flags() else {
                    panic!()
                };
                let RelocationTarget::Symbol(index) = r.target() else {
                    panic!()
                };
                let sym = file.symbol_by_index(index).unwrap();
                let (target, addend) = if let Some(section) = sym.section_index() {
                    (
                        file.section_by_index(section)
                            .unwrap()
                            .name()
                            .unwrap()
                            .to_owned(),
                        r.addend() + sym.address() as i64,
                    )
                } else {
                    (sym.name().unwrap().to_owned(), r.addend())
                };
                (offset, r_type, target, addend)
            })
            .collect();
        let object::SectionFlags::Elf { sh_flags } = section.flags() else {
            panic!()
        };
        result.push((
            name.to_owned(),
            section.size(),
            sh_flags,
            section.data().unwrap().to_vec(),
            relocations,
        ));
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
fn compare(source: &str) {
    compare_tools(source, true);
}
fn compare_tools(source: &str, clang: bool) {
    let source = format!(".att_syntax prefix\n{source}\n.data\n.byte 7\n.bss\n.skip 1\n");
    let ours = assemble_object(&source).unwrap_or_else(|e| panic!("{source}\n{e}"));
    let temp = std::env::temp_dir().join(format!(
        "keel-expressions-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    std::fs::write(&input, &source).unwrap();
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    for (tool, args) in [
        ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
        (gnu.as_str(), vec!["--64"]),
    ] {
        if tool == "clang" && !clang {
            continue;
        }
        let output = temp.join("reference.o");
        let result = Command::new(tool)
            .args(args)
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{tool}: {}\n{source}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            snapshot(&ours),
            snapshot(&std::fs::read(output).unwrap()),
            "{tool}\n{source}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn expanded_statements_resolve_nested_macros_numeric_labels_and_dollar_immediates() {
    compare(
        r#"
.text
.macro leaf n:req
addq $\n,%rax
.endm
.macro wrapper old:req
\old
.endm
wrapper "999: leaf 1; jmp 999b"
.irp n,2,3
wrapper "888: leaf \n; jmp 888b"
.endr
.data
.macro fields
.value 0x1234
.2byte external+1
.4byte external+3
.8byte external+5
.endm
fields
"#,
    );
}

#[test]
fn grouped_operands_and_unary_chains_keep_registers_separate_from_symbols() {
    compare(
        r#"
.text
movq $((1)<<6),%rax
leaq (external+7)(%rip),%rax
leaq external--8(%rip),%r12
leaq external- -8(%rip),%r13
leaq external+-8(%rip),%r14
movq $--8,%r15
movq (external),%rax
movq 4+(-8)(%rax),%rdx
movq (,%rax,(1<<2)),%rdx
.intel_syntax noprefix
mov rax,[rbx-(-8)]
mov rax,[rbx+(1<<2)*3]
mov rax,[rbx+rcx*4]
mov rax,[rbx+"rcx"]
.att_syntax prefix
"#,
    );
    // GNU accepts adjacent unary minus here; LLVM's Intel lexer rejects it.
    compare_tools(
        ".intel_syntax noprefix\nmov rax,[rbx--8]\nmov rax,[rbx+rcx*((1)<<2)]\n",
        false,
    );
    for source in [
        "mov rax,[rbx+(rcx)]",
        "mov rax,[rbx+(rcx-8)]",
        "mov rax,[--rbx]",
        ".att_syntax noprefix\nmovq (rax+8),rdx",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}

#[test]
fn optional_keyword_whitespace_and_iteration() {
    compare(
        r#"
.macro instruction body="nop"
\body
.endm
instruction
instruction body="nop"
.macro emit a=1 b=2 c
.byte \a, \b
.ifnc \c,
.byte \c
.endif
.endm
emit
emit 3 4 5
emit 9,10 11
emit b=7,a=6
emit ,8,
.macro select reg:req type=3
.set .Lfound,0
.set .Lregnr,0
.irp rs,rax,rcx,rdx,rbx,rsp,rbp,rsi,rdi,r8,r9,r10,r11,r12,r13,r14,r15
.ifc \reg,%\rs
.set .Lfound,.Lfound+1
.long \type + (.Lregnr << 8)
.endif
.set .Lregnr,.Lregnr+1
.endr
.if (.Lfound != 1)
.error "bad register"
.endif
.endm
select type=7,reg=%r9
select %rax
.purgem select
.macro select
.irpc c,"123"
.byte \c
.endr
.endm
select
.irp x,4,5
.irpc y,67
.byte \x + \y
.endr
.endr
"#,
    );
}

#[test]
fn empty_iteration_follows_gnu() {
    // LLVM omits the empty iteration; GNU runs it once with an empty value.
    compare_tools(".irp x,\n.ifc \\x,\n.byte 23\n.endif\n.endr\n", false);
}

#[test]
fn unwind_defaults_register_tests_and_mutable_symbol_arguments() {
    compare(
        r#"
.macro hint type:req sp_reg=0 sp_offset=0 signal=0
.short \sp_offset
.byte \sp_reg,\type,\signal
.endm
.macro regs base=%rsp offset=0 indirect=0 partial=0 signal=1
.if \base == %rsp
.if \indirect
.set sp_reg,9
.else
.set sp_reg,5
.endif
.elseif \base == %rbp
.set sp_reg,4
.elseif \base == %r10
.set sp_reg,6
.else
.error "bad base"
.endif
.set sp_offset,\offset
.if \partial
.set type,4
.set sp_offset,\offset+(16*8)
.else
.set type,3
.endif
hint sp_reg=sp_reg sp_offset=sp_offset type=type signal=\signal
.endm
regs
regs base=%rbp offset=16 partial=1
regs base=%r10 signal=0
.set i,0
.rept 32
.if ((0x20027d00 >> i) & 1) == 0
.byte 0
.else
.byte 1
.endif
pushq $i
i=i+1
.endr
"#,
    );
}

#[test]
fn gnu_constant_operators_and_parenthesized_relocations() {
    compare(
        r#"
.byte 2+3*4, (1<<3)|2, (19/3)%4, ~0 & 0xff
.byte (1<2), (2==2), (3!=3), (1<=1), (1>=2), (2<>3)
.byte !0, !4, (1&&2), (0||3), (1 | 2 & 4), (1+2<<3)
.set xff,7
.byte 0xff
.set imm,17
movq $imm,%rax
.set imm,19
movq $imm,%rax
.set cnt,2
.int cnt
cnt=3
.int cnt
.set n,(3*32+21)
.rept (n & 3)
.byte n
.endr
1: nop
.pushsection __ex_table,"a"
.long (1b)-., (external+7)-.
.popsection
.quad (1b)+8, (1b)-(1b)
"#,
    );
}

#[test]
fn alternatives_skip_forward_section_differences_and_branches() {
    for old in [1, 7] {
        compare(&format!(
            r#"
740: .rept {old}
nop
.endr
741:
.skip -(((744f-743f)-(741b-740b)) > 0) * ((744f-743f)-(741b-740b)),0x90
742:
jmp 745f
.pushsection .altinstructions,"a"
.long (740b)-., (743f)-.
.byte 742b-740b,744f-743f
.popsection
.pushsection .altinstr_replacement,"ax"
743: .byte 1,2,3,4
744:
.popsection
.org 128
745: ret
"#
        ));
    }
    compare("jmp 2f\n1: nop\n.skip 3f-2f,0x90\n2: .rept 130\nnop\n.endr\n3: ret\n");
}

#[test]
fn failures_are_explicit_and_bounded() {
    for source in [
        ".if unknown\nnop\n.endif",
        ".set a,b\n.set b,a\n.if a\n.endif",
        ".skip unknown",
        ".text\na:nop\n.data\nb:.byte 0\n.text\n.skip b-a",
        ".text\na:nop\n.skip a",
        ".skip 1/0",
        ".skip 1<<64",
        ".skip 1<<63",
        ".skip 67108865",
        ".set a,b\n.set b,a\n.skip a",
        ".skip 0x10000000000000000",
        ".skip (1<<63)*(1<<63)",
        ".skip finish-start\nstart:nop\nfinish:\n.skip -1",
        ".skip 67108865+finish-start\nstart:nop\nfinish:",
        "start:\n.skip 1-(finish-start)\nfinish:",
        ".macro m a:req\n.endm\nm",
        ".macro m a\n.endm\nm b=1",
        ".macro m a\n.endm\nm a=1,a=2",
        ".macro m a,b\n.endm\nm a=1,2",
        ".irp x,1\n.irp y,2\n.endm\n.endr",
        ".irpc x,abc\n.irpc y,def\n.endr",
        ".code16\nnop",
        ".code32\nnop",
    ] {
        assert!(
            assemble_object(source).is_err(),
            "unexpectedly accepted {source}"
        );
    }
    let deep = format!(".byte {}1{}", "(".repeat(66), ")".repeat(66));
    assert!(assemble_object(&deep).is_err());
}

#[test]
fn diagnostic_reasons_cover_resolution_cycles_and_bounds() {
    for (source, reason) in [
        (
            ".if missing\n.endif",
            "undefined or layout-dependent symbol",
        ),
        (".skip missing", "undefined symbol"),
        (
            ".text\na:nop\n.data\nb:.byte 0\n.text\n.skip b-a",
            "same-section difference",
        ),
        (
            ".set a,b\n.set b,a\n.skip a",
            "cyclic or excessively nested",
        ),
        (
            "start:\n.skip 1-(finish-start)\nfinish:",
            "cyclic variable-length directive layout",
        ),
        (".skip 1<<64", "shift must be in 0..64"),
        (".skip 67108865", "64 MiB"),
        (".skip -1", "nonnegative"),
        (".irpc x,\n.endr", "requires a character string"),
    ] {
        let error = assemble_object(source).unwrap_err();
        assert!(
            error.message.contains(reason),
            "{source}: {error}; expected {reason}"
        );
    }
}

#[test]
fn unknown_symbols_cross_section_differences_and_alias_cycles_fail_oracles() {
    let temp = std::env::temp_dir().join(format!("keel-expression-errors-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("invalid.s");
    let output = temp.join("invalid.o");
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    for source in [
        ".if missing\n.endif",
        ".skip missing",
        ".text\na:nop\n.data\nb:.byte 0\n.text\n.skip b-a",
        ".set a,b\n.set b,a\n.skip a",
    ] {
        assert!(assemble_object(source).is_err());
        std::fs::write(&input, source).unwrap();
        for (tool, args) in [
            ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
            (gnu.as_str(), vec!["--64"]),
        ] {
            let result = Command::new(tool)
                .args(args)
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                !result.status.success(),
                "{tool} unexpectedly accepted {source}"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn absolute_assignment_snapshots_survive_text_fragments() {
    let mut program = Program::new();
    program.text("counter=1\n");
    program.label("first");
    program.text(".byte counter\n");
    program.label("second");
    program.text("counter=counter+1\n");
    program.label("third");
    program.text(".byte counter\n");
    let bytes = assemble_program_object(&program).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[1, 2]
    );
}

#[test]
fn parenthesized_macro_invocations_preserve_assembler_arguments() {
    compare(
        r#"
.text
.macro record symbol:req
.pushsection .data
.long \symbol - .
.popsection
.endm
first: nop
record(first)
record (first)
second: ret
.macro wrapper target:req
record(\target)
.endm
wrapper second
.macro spell arg:req
.ascii "<\arg>"
.endm
spell(foo)
"#,
    );
}

#[test]
fn full_width_symbolic_addends_match_elf64_word_semantics() {
    compare(
        r#"
.text
addq $(external - 0xffffffff80000000), %rcx
movq $(external + 0xffffffffffffffff), %rax
leaq external-0xffffffffffffffff(%rip), %rcx
.data
.quad external-0xffffffffffffffff
.quad external+0xffffffffffffffff
.quad external-0x8000000000000001
.long external-0xffffffff80000000
.short external+0xffffffffffffffff
.byte external-0xffffffffffffffff
"#,
    );
    compare_tools("addq $(external - 0xffffffff80000000), %rax", false);
    for source in [
        ".quad external+0xffffffffffffffff+1",
        ".quad external-0xffffffffffffffff-1",
    ] {
        let error = assemble_object(source).unwrap_err().to_string();
        assert!(error.contains("exceeds 64 bits"), "{error}");
    }
}

#[test]
fn octa_data_preserves_all_128_bits() {
    compare(
        r#"
.data
.octa 0, 0x6b20657479622d323320646e61707865
.octa 340282366920938463463374607431768211455
.octa 0xffffffffffffffffffffffffffffffff
.octa 0b10101, 0777
.text
ret
"#,
    );
    compare_tools(
        ".data\n.octa -1, -0xffffffffffffffffffffffffffffffff, 1+2\n",
        false,
    );
    for source in [
        ".octa 340282366920938463463374607431768211456",
        ".octa 0x100000000000000000000000000000000",
        ".octa 0xg",
        ".octa external",
        ".octa",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}

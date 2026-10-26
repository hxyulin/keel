//! Syntax and byte/relocation oracles against Clang's integrated assembler.
use keel_asm::x86::{
    Features, Syntax, assemble_object, assemble_object_with_syntax_and_features,
    assemble_program_object,
};
use keel_asm_ir::{
    Program,
    x86::{Instruction, SourceParser},
};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

const FEATURES: Features = Features {
    xsave: true,
    osxsave: true,
    avx: true,
    bmi1: true,
    bmi2: true,
    ..Features {
        popcnt: true,
        sse41: true,
        ..Features::NONE
    }
};
fn assemble(source: &str) -> Vec<u8> {
    assemble_object_with_syntax_and_features(source, Syntax::Att, FEATURES)
        .unwrap_or_else(|e| panic!("{source}\n{e}"))
}
fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "keel-att-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
// Match the existing encoder's nonrelaxable GOTPCREL relocation contract.
// Clang's default GOTPCRELX/REX_GOTPCRELX forms are a separate model gap.
fn clang(source: &str) -> Vec<u8> {
    let dir = Scratch::new();
    let input = dir.0.join("input.s");
    let output = dir.0.join("reference.o");
    std::fs::write(&input, source).unwrap();
    let result = Command::new("clang")
        .args([
            "-c",
            "-target",
            "x86_64-unknown-linux-gnu",
            "-Wa,-mrelax-relocations=no",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("Clang is required for the AT&T differential tests");
    assert!(
        result.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(output).unwrap()
}
fn assert_clang(source: &str) {
    let reference = clang(source);
    let ours = assemble(source);
    assert_eq!(text(&ours), text(&reference), "{source}");
}

#[test]
fn clang_integer_width_address_and_extension_matrix() {
    let mut cases = Vec::new();
    for (suffix, r, other) in [
        ("b", "%r9b", "%r10b"),
        ("w", "%r9w", "%r10w"),
        ("l", "%r9d", "%r10d"),
        ("q", "%r9", "%r10"),
    ] {
        for op in [
            "mov", "add", "sub", "and", "or", "xor", "cmp", "test", "adc", "sbb",
        ] {
            for src in [
                other.to_string(),
                "-16(%r12,%r13,8)".into(),
                "4096(,%r14,4)".into(),
            ] {
                cases.push(format!("{op}{suffix} {src}, {r}"));
            }
            cases.push(format!("{op}{suffix} $7, (%r15)"));
        }
        for op in ["inc", "dec", "neg", "not", "mul", "imul", "div", "idiv"] {
            cases.push(format!("{op}{suffix} (%r13)"));
        }
        for op in ["shl", "shr", "sar", "rol", "ror", "sal"] {
            cases.push(format!("{op}{suffix} {r}"));
            cases.push(format!("{op}{suffix} %cl, 128(%r13)"));
            cases.push(format!("{op}{suffix} $3, {r}"));
        }
    }
    for (name, src, dst) in [
        ("movsbw", "%al", "%r9w"),
        ("movsbl", "%al", "%r9d"),
        ("movsbq", "%al", "%r9"),
        ("movswl", "%ax", "%r9d"),
        ("movswq", "%ax", "%r9"),
        ("movslq", "%eax", "%r9"),
        ("movzbw", "%al", "%r9w"),
        ("movzbl", "%al", "%r9d"),
        ("movzbq", "%al", "%r9"),
        ("movzwl", "%ax", "%r9d"),
        ("movzwq", "%ax", "%r9"),
    ] {
        cases.push(format!("{name} {src},{dst}"));
        cases.push(format!("{name} 16(%r12),{dst}"));
    }
    cases.extend(
        [
            "pushq %r15",
            "popq %r15",
            "pushw %r9w",
            "popw %r9w",
            "pushq $-7",
            "pushq (%r13)",
            "popq (%r13)",
            "retq $16",
            "leaveq",
            "movabsq $0x123456789abcdef0,%r11",
            "cltd",
            "cltq",
            "cqto",
            "cbtw",
            "cwtl",
            "leal (%rax,%rcx,4),%r9d",
            "movq %fs:0,%rcx",
            "movq %gs:16(%rax),%rcx",
            "imulq $7,8(%r13),%r9",
            "imulq $7,%r9",
            "shldq $3,%r9,%r10",
            "shrdq %cl,%r9,(%r10)",
            "shldq %r9,%r10",
            "shrdq %r9,(%r10)",
            "cmpq $7,8(%rip)",
        ]
        .map(str::to_string),
    );
    // Separate cases make any unexpected byte mismatch identify one instruction.
    for case in &cases {
        assert_clang(&format!(".text\n{case}\n"));
    }
    eprintln!(
        "{} integer/address/extension Clang byte oracles",
        cases.len()
    );
}

#[test]
fn clang_simd_bmi_and_condition_operand_orders() {
    let mut cases = Vec::new();
    for op in [
        "addss", "subsd", "mulps", "divpd", "movaps", "movups", "movapd", "movupd", "paddb",
        "psubq", "pmullw", "ucomiss", "ucomisd", "xorps", "xorpd", "movq", "movhlps",
    ] {
        cases.push(format!("{op} %xmm13,%xmm14"));
    }
    for cc in [
        "a", "ae", "b", "be", "e", "ne", "g", "ge", "l", "le", "o", "no", "s", "ns", "p", "np",
    ] {
        cases.push(format!("cmov{cc}q %r9,%r10"));
        cases.push(format!("set{cc} %r9b"));
    }
    for op in ["andn", "pext", "pdep", "mulx"] {
        cases.push(format!("{op}q 16(%r12,%r13,8),%r9,%r10"));
    }
    for op in ["bextr", "bzhi", "shlx", "shrx", "sarx"] {
        cases.push(format!("{op}q %r9,16(%r12,%r13,8),%r10"));
    }
    for op in ["blsi", "blsr", "blsmsk", "tzcnt"] {
        cases.push(format!("{op}q 16(%r12,%r13,8),%r10"));
    }
    cases.extend(
        [
            "rorxq $7,(%r12),%r10",
            "roundss $3,%xmm13,%xmm14",
            "roundsd $3,(%r13),%xmm14",
            "vsubps %xmm12,%xmm13,%xmm14",
            "vdivsd (%r13),%xmm13,%xmm14",
            "vroundss $3,%xmm12,%xmm13,%xmm14",
            "vmovss %xmm12,%xmm13,%xmm14",
            "cvtsi2sdq (%r13),%xmm14",
            "cvtsi2ssl (%r13),%xmm14",
            "cvtsi2sd %r9,%xmm14",
            "cvttsd2siq %xmm14,%r9",
            "cvttss2sil (%r13),%r9d",
            "movd %r9d,%xmm14",
            "movq %r9,%xmm14",
            "movq %xmm14,%r9",
            "lock\t\txaddq %r9,(%r10)",
            "setneb %r9b",
            "seteb (%r10)",
            "setbb %r10b",
            "setnbb (%r10)",
        ]
        .map(str::to_string),
    );
    for case in &cases {
        assert_clang(&format!(".text\n{case}\n"));
    }
    eprintln!("{} SIMD/BMI/condition Clang byte oracles", cases.len());
}

#[test]
fn added_sse2_model_opcodes_keep_att_operand_order() {
    // Requires the separately reviewed x86-features prerequisite patch.
    let mut cases = Vec::new();
    for op in [
        "movdqa",
        "movdqu",
        "pxor",
        "pand",
        "pandn",
        "por",
        "pcmpeqb",
        "pcmpeqw",
        "pcmpeqd",
        "pcmpgtb",
        "pcmpgtw",
        "pcmpgtd",
        "punpcklbw",
        "punpcklwd",
        "punpckldq",
        "punpcklqdq",
        "punpckhbw",
        "punpckhwd",
        "punpckhdq",
        "punpckhqdq",
        "unpcklps",
        "unpckhps",
        "unpcklpd",
        "unpckhpd",
        "andnps",
        "andnpd",
        "minps",
        "minpd",
        "maxps",
        "maxpd",
        "cmpltss",
        "cmpnltps",
        "pmaddwd",
        "pmulhuw",
        "pmulhw",
        "pmuludq",
        "cvtdq2ps",
        "cvtps2dq",
        "cvttps2dq",
        "cvtps2pd",
        "cvtpd2ps",
        "cvtdq2pd",
        "cvtpd2dq",
        "cvttpd2dq",
    ] {
        cases.push(format!("{op} %xmm13,%xmm14"));
        cases.push(format!("{op} 16(%r12,%r13,8),%xmm14"));
    }
    for op in ["movdqa", "movdqu"] {
        cases.push(format!("{op} %xmm14,16(%r12,%r13,8)"));
    }
    for op in [
        "pshufd", "pshuflw", "pshufhw", "shufps", "shufpd", "cmpps", "cmppd", "cmpss", "cmpsd",
    ] {
        cases.push(format!("{op} $3,%xmm13,%xmm14"));
        cases.push(format!("{op} $3,16(%r12,%r13,8),%xmm14"));
    }
    for op in [
        "psllw", "pslld", "psllq", "psrlw", "psrld", "psrlq", "psraw", "psrad",
    ] {
        cases.push(format!("{op} $7,%xmm14"));
        cases.push(format!("{op} %xmm13,%xmm14"));
        cases.push(format!("{op} 16(%r12,%r13,8),%xmm14"));
    }
    for op in ["pslldq", "psrldq"] {
        cases.push(format!("{op} $7,%xmm14"));
    }
    for op in ["pmovmskb", "movmskps", "movmskpd"] {
        cases.push(format!("{op} %xmm13,%r10d"));
        cases.push(format!("{op} %xmm13,%r10"));
    }
    for case in &cases {
        assert_clang(&format!(".text\n{case}\n"));
    }
    eprintln!("{} added SSE2 AT&T Clang byte oracles", cases.len());
}

#[test]
fn actual_clang_and_rustc_instruction_body_oracles() {
    // Verbatim body from rust-extern_c-x86att-O2, lines 31-39 (rustc 1.99).
    // File-level .file/.prefalign support is outside this syntax change.
    assert_clang(
        ".text\n.cfi_startproc\n\tpushq\t%rax\n\t.cfi_def_cfa_offset 16\n\tcallq\t*%rdi\n\t#APP\n\t#NO_APP\n\tpopq\t%rax\n\t.cfi_def_cfa_offset 8\n\tretq\n.cfi_endproc\n",
    );
    // Verbatim basic block from cpp-virtual-x86att-O2, lines 45-52.
    assert_clang(
        ".text\n\tmovq\t%r13, %rbp\n\tmovq\t(%rbx,%r12,8), %rdi\n\tmovq\t(%rdi), %rax\n\tcallq\t*16(%rax)\n\tleaq\t(%rax,%r13,8), %r13\n\tsubq\t%rbp, %r13\n\tmovq\t(%rbx,%r12,8), %rbp\n\ttestq\t%rbp, %rbp\n",
    );
}

#[test]
fn clang_also_rejects_invalid_widths_and_addresses() {
    let dir = Scratch::new();
    for bad in [
        "movq %eax,%rax",
        "movb %xmm1,%al",
        "movsbq %ax,%rax",
        "movzwl %al,%eax",
        "cvtsi2sdq %eax,%xmm0",
        "cvttsd2sil %xmm0,%rax",
        "movq (%rax,%rsp,2),%rdx",
        "movq (%rip,%rcx,4),%rax",
        "movq (%rax,%rcx,3),%rdx",
        "movq (),%rax",
        "movq (%rax,%rcx,4,%rdx),%rax",
        "movb %ah,%r8b",
    ] {
        let source = format!(".text\n{bad}\n");
        assert!(
            assemble_object_with_syntax_and_features(&source, Syntax::Att, FEATURES).is_err(),
            "{bad}"
        );
        let input = dir.0.join("bad.s");
        std::fs::write(&input, &source).unwrap();
        let result = Command::new("clang")
            .args(["-c", "-target", "x86_64-unknown-linux-gnu"])
            .arg(&input)
            .arg("-o")
            .arg(dir.0.join("bad.o"))
            .output()
            .unwrap();
        assert!(!result.status.success(), "Clang accepted {bad}");
    }
}

#[test]
fn clang_symbols_relocations_and_indirect_transfers() {
    let source = r#".text
.globl entry
.type entry,@function
entry:
callq external@PLT
jmp external@PLT
movq external@GOTPCREL(%rip),%r9
leaq external+7(%rip),%r10
movl $external,%r11d
movq $external+11,%r11
movabsq $external+9,%r12
movl external,%r13d
movq %fs:tls@TPOFF,%r14
leaq tls@TPOFF(%r14),%r15
movq tls@GOTTPOFF(%rip),%r9
callq *%r12
callq *16(%r13,%r14,8)
jmpq *external@GOTPCREL(%rip)
movl $7,external(%rip)
retq
.size entry,.-entry
"#;
    fn relocations(bytes: &[u8]) -> Vec<(u64, object::RelocationFlags, i64, String)> {
        let f = object::File::parse(bytes).unwrap();
        f.section_by_name(".text")
            .unwrap()
            .relocations()
            .map(|(offset, r)| {
                let name = match r.target() {
                    RelocationTarget::Symbol(index) => f
                        .symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_string(),
                    target => panic!("{target:?}"),
                };
                (offset, r.flags(), r.addend(), name)
            })
            .collect()
    }
    let ours = assemble(source);
    let reference = clang(source);
    assert_eq!(text(&ours), text(&reference));
    assert_eq!(relocations(&ours), relocations(&reference));
}

#[test]
fn dialect_switches_preserve_symbols_strings_and_typed_programs() {
    let source = r#".text
.intel_syntax noprefix
mov r9,r10
.att_syntax prefix
movq %r10,%r9
.att_syntax noprefix
movq r10,r9
.intel_syntax prefix
mov %r9,%r10
mov %r9,qword ptr [%r12+8*%r13-16]
.att_syntax
movq -16(%r12,%r13,8),%r9
.intel_syntax
mov r9,[r12+8*r13-16]
.ascii "%rax,$name,#,;"
"#;
    // Clang 21 rejects .att_syntax noprefix and .intel_syntax prefix, so
    // verify these lexical modes against the equivalent default Intel stream.
    let expected = assemble_object("mov r9,r10;mov r9,r10;mov r9,r10;mov r9,r10;mov r9,[r12+8*r13-16];mov r9,[r12+8*r13-16];mov r9,[r12+8*r13-16]\n.ascii \"%rax,$name,#,;\"" ).unwrap();
    let ours = assemble_object(source).unwrap();
    assert_eq!(text(&ours), text(&expected));
    assert_clang(
        ".intel_syntax noprefix\nmov r9,r10\n.att_syntax prefix\nmovq %r10,%r9\n.intel_syntax\nmov r9,r10\n.att_syntax\nmovq %r10,%r9",
    );
    let p = keel_asm_ir::x86::parse_program(source).unwrap();
    assert_eq!(text(&assemble_program_object(&p).unwrap()), text(&ours));
    let mut p = Program::new();
    p.text(".att_syntax\n");
    p.x86_instruction(Instruction::parse_intel("mov r9,r10").unwrap());
    p.text("movq %r10,%r9\n");
    assert_eq!(
        text(&assemble_program_object(&p).unwrap()),
        text(&assemble("movq %r10,%r9\nmovq %r10,%r9"))
    );
    let mut parser = SourceParser::default();
    parser.parse(".att_syntax noprefix").unwrap();
    assert!(parser.parse("movq r10,r9").is_ok());
    assert!(parser.parse("movq %r10,%r9").is_err());
    // Embedded '$' belongs to the symbol, rather than the immediate prefix.
    let a = Instruction::parse_att("leaq lambda$_0(%rip),%rax").unwrap();
    assert!(a.to_string().contains("lambda$_0"));
}

#[test]
fn malformed_or_unrepresentable_forms_fail_on_the_source_line() {
    for bad in [
        "movq %eax,%rax",
        "movb %xmm1,%al",
        "movsbq %ax,%rax",
        "movzwl %al,%eax",
        "cvtsi2sdq %eax,%xmm0",
        "cvttsd2sil %xmm0,%rax",
        "movq 0(%eax,%rcx,2),%rax",
        "movq (%rax,%rsp,2),%rdx",
        "movq (%rip,%rcx,4),%rax",
        "movq (%rax,%rcx,3),%rdx",
        "movq (,%rcx,),%rdx",
        "movq (),%rax",
        "movq (%rax,%rcx,4,%rdx),%rax",
        "movq (%rax,%rcx,4,%rdx,%rsi),%rax",
        "movq (%rax,%rcx,4),",
        "movq (%rax,%rcx,4,%rax",
        "callq %rax",
        "callq *$12",
        "je *%rax",
        "movq *%rax,%rbx",
        "mov (%rax),$3",
        "lock addq %rax,%rbx",
        "movb %ah,%r8b",
        ".att_syntax bogus",
        ".intel_syntax prefix junk",
        "movq %st(0),%rax",
        "fsub %st(2),%st(3)",
        "leaq external- -8(%rip),%rax",
        "leaq external--8(%rip),%rax",
        "leaq external+-8(%rip),%rax",
        "movq $--8,%rax",
        "setel (%rax)",
        "pushl $7",
        "nopw",
    ] {
        let source = format!(".att_syntax\n\n{bad}\n");
        let err = assemble_object(&source).expect_err(bad).to_string();
        assert!(err.contains("line 3:"), "{bad}: {err}");
    }
    assert!(
        assemble_object("movq %r10,%r9").is_err(),
        "Intel default must not guess AT&T"
    );
    assert!(assemble_object(".att_syntax noprefix\nmovq %r10,%r9").is_err());
    assert!(assemble_object(".intel_syntax prefix\nmov rax,rbx").is_err());
    // Feature directives and unsupported operations remain loud errors.
    assert!(assemble_object(".att_syntax\n.arch generic64\nretq").is_err());
    assert!(
        assemble_object(".att_syntax\nandnq %rax,%rbx,%rcx")
            .unwrap_err()
            .to_string()
            .contains("bmi1")
    );
}

#[test]
fn local_x86_runtime_abi_and_operand_order() {
    let native = cfg!(all(target_os = "linux", target_arch = "x86_64"));
    let darwin = cfg!(target_os = "macos")
        && Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .is_ok_and(|s| s.success());
    if !native && !darwin {
        eprintln!("SKIP runtime: no local x86-64 Linux or Darwin runner");
        return;
    }
    let kernels = [
        (
            "indexed",
            "movq (%rdi,%rsi,8),%rax\nsubq %rdx,%rax\nimulq $7,%rax\nretq",
        ),
        ("extend", "movsbl (%rdi,%rsi),%eax\nimull $7,%eax\nretq"),
        ("compare", "cmpq %rsi,%rdi\nsetg %al\nmovzbl %al,%eax\nretq"),
        ("scalar", "subsd %xmm1,%xmm0\nretq"),
        (
            "indirect",
            "movq %rdi,%rax\nmovq %rsi,%rdi\nsubq $8,%rsp\ncallq *%rax\naddq $8,%rsp\nretq",
        ),
        ("atomic", "lock xaddq %rsi,(%rdi)\nmovq %rsi,%rax\nretq"),
    ];
    let dir = Scratch::new();
    let mut wrappers = String::from(".text\n");
    for (name, body) in kernels {
        let source = format!(".text\n{body}\n");
        let ours = assemble(&source);
        let reference = clang(&source);
        assert_eq!(text(&ours), text(&reference), "{name}");
        for (suffix, object) in [("keel", ours), ("clang", reference)] {
            let symbol = format!("{}{name}_{suffix}", if darwin { "_" } else { "" });
            wrappers += &format!(
                ".globl {symbol}\n{symbol}:\n.byte {}\n",
                text(&object)
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    if !darwin {
        wrappers += ".section .note.GNU-stack,\"\",@progbits\n";
    }
    // Mach-O wrappers embed unchanged, relocation-free ELF text for execution
    // under Darwin/Rosetta. They do not test ELF loading or ELF relocations.
    std::fs::write(dir.0.join("wrappers.s"), wrappers).unwrap();
    std::fs::write(dir.0.join("check.c"),r#"
#include <stdint.h>
#include <stdio.h>
#define DECL(tag) \
extern long indexed_##tag(const long*,long,long); \
extern int extend_##tag(const signed char*,long); \
extern int compare_##tag(long,long); \
extern double scalar_##tag(double,double); \
extern long indirect_##tag(long(*)(long),long); \
extern long atomic_##tag(long*,long);
DECL(keel)
DECL(clang)
static long callback(long x) { return x*3-7; }
int main(void) {
    const long values[]={-1024,-3,0,1,20,41,100000};
    const signed char bytes[]={-128,-1,0,1,127};
    for(int i=0;i<7;i++) for(int j=0;j<7;j++) {
        long a=values[i],b=values[j];
        long expected=(a-b)*7;
        if(indexed_keel(values,i,b)!=expected || indexed_clang(values,i,b)!=expected) return 1;
        if(compare_keel(a,b)!=(a>b) || compare_clang(a,b)!=(a>b)) return 2;
        if(scalar_keel((double)a,(double)b)!=(double)a-(double)b || scalar_clang((double)a,(double)b)!=(double)a-(double)b) return 3;
        if(indirect_keel(callback,a)!=callback(a) || indirect_clang(callback,a)!=callback(a)) return 4;
        long x=a,y=a;
        if(atomic_keel(&x,b)!=a || atomic_clang(&y,b)!=a || x!=a+b || y!=a+b) return 5;
    }
    for(int i=0;i<5;i++) if(extend_keel(bytes,i)!=(int)bytes[i]*7 || extend_clang(bytes,i)!=(int)bytes[i]*7) return 6;
    puts("6 AT&T kernels: indexed memory, sign extension, signed condition, scalar SIMD, indirect-call ABI and LOCK passed against Clang and C");
    return 0;
}
"#).unwrap();
    let mut compiler = Command::new("clang");
    if darwin {
        compiler.args(["-target", "x86_64-apple-darwin"]);
    }
    let result = compiler
        .args(["-O2", "-msse2", "-mno-avx", "-Wall", "-Wextra", "-Werror"])
        .arg(dir.0.join("check.c"))
        .arg(dir.0.join("wrappers.s"))
        .arg("-o")
        .arg(dir.0.join("check"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = if darwin {
        Command::new("arch")
            .arg("-x86_64")
            .arg(dir.0.join("check"))
            .output()
    } else {
        Command::new(dir.0.join("check")).output()
    }
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&result.stdout));
    assert!(
        result.status.success(),
        "status={}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn evidenced_x87_memory_loads_and_scalar_less_than_match_clang() {
    let att = ".text\nflds (%rax)\nfldl -128(%r13,%r12,4)\nfldt external(%rip)\n\
               flds %fs:4096(,%r14,8)\nfldl 128(%r15)\nfldt (%r8)\n\
               cmpltsd %xmm15, %xmm8\ncmpltsd external(%rip), %xmm12\n\
               cmpltsd %gs:8(%r13,%r12,2), %xmm0\n";
    let intel = ".intel_syntax noprefix\n.text\nfld dword ptr [rax]\n\
                 fld qword ptr [r13+r12*4-128]\nfld tbyte ptr [rip+external]\n\
                 fld dword ptr fs:[r14*8+4096]\nfld qword ptr [r15+128]\n\
                 fld tbyte ptr [r8]\ncmpltsd xmm8, xmm15\n\
                 cmpltsd xmm12, qword ptr [rip+external]\n\
                 cmpltsd xmm0, qword ptr gs:[r13+r12*2+8]\n";
    let expected = clang(att);
    let att_object = assemble(att);
    let intel_object = assemble_object(intel).unwrap();
    assert_eq!(att_object, intel_object);
    assert_eq!(text(&att_object), text(&expected));
    // RIP fixups before an implicit compare predicate must account for imm8.
    let relocations = |bytes: &[u8]| {
        let file = object::File::parse(bytes).unwrap();
        file.section_by_name(".text")
            .unwrap()
            .relocations()
            .map(|(offset, r)| (offset, r.addend(), r.flags()))
            .collect::<Vec<_>>()
    };
    assert_eq!(relocations(&att_object), relocations(&expected));
    let program = keel_asm_ir::x86::parse_program(intel).unwrap();
    assert_eq!(assemble_program_object(&program).unwrap(), intel_object);
    for instruction in program.items.iter().filter_map(|item| match item {
        keel_asm_ir::Item::X86Instruction(i) => Some(i),
        _ => None,
    }) {
        assert_eq!(
            Instruction::parse_intel(&instruction.to_string()).unwrap(),
            *instruction
        );
    }
    for bad in [
        "fld [rax]",
        "fld word ptr [rax]",
        "fld xmm0",
        "fld rax",
        "fld 1",
        "fld qword ptr [rax], xmm0",
        "cmpltsd xmm0, dword ptr [rax]",
        "cmpltsd xmm0, xmm1, 1",
        "lock fld qword ptr [rax]",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
    for bad in ["flds %eax", "fldt $1", "fldl (%rax), (%rbx)"] {
        assert!(Instruction::parse_att(bad).is_err(), "{bad}");
    }
}

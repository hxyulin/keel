//! Executes the exact text bytes assembled as Linux ELF. Darwin x86-64 uses
//! the same register ABI for these leaf kernels; only their object wrapper is
//! changed there. No cross-target ELF linking or Linux runtime is implied.
use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection};
use std::process::Command;

fn cases() -> Vec<(String, String)> {
    let mut cases = Vec::new();
    let mut add = |instruction: String, expression: String| cases.push((instruction, expression));
    for (op, intrinsic) in [
        ("pxor", "xor_si128"),
        ("pand", "and_si128"),
        ("pandn", "andnot_si128"),
        ("por", "or_si128"),
        ("pcmpeqb", "cmpeq_epi8"),
        ("pcmpeqw", "cmpeq_epi16"),
        ("pcmpeqd", "cmpeq_epi32"),
        ("pcmpgtb", "cmpgt_epi8"),
        ("pcmpgtw", "cmpgt_epi16"),
        ("pcmpgtd", "cmpgt_epi32"),
        ("punpcklbw", "unpacklo_epi8"),
        ("punpcklwd", "unpacklo_epi16"),
        ("punpckldq", "unpacklo_epi32"),
        ("punpcklqdq", "unpacklo_epi64"),
        ("punpckhbw", "unpackhi_epi8"),
        ("punpckhwd", "unpackhi_epi16"),
        ("punpckhdq", "unpackhi_epi32"),
        ("punpckhqdq", "unpackhi_epi64"),
        ("pmaddwd", "madd_epi16"),
        ("pmulhuw", "mulhi_epu16"),
        ("pmulhw", "mulhi_epi16"),
        ("pmuludq", "mul_epu32"),
    ] {
        for source in ["xmm15", "xmmword ptr [rsi]"] {
            add(
                format!("{op} xmm14,{source}"),
                format!("_mm_{intrinsic}(a,b)"),
            );
        }
    }
    for (op, intrinsic, suffix) in [
        ("unpcklps", "unpacklo", "ps"),
        ("unpckhps", "unpackhi", "ps"),
        ("unpcklpd", "unpacklo", "pd"),
        ("unpckhpd", "unpackhi", "pd"),
        ("andnps", "andnot", "ps"),
        ("andnpd", "andnot", "pd"),
        ("minps", "min", "ps"),
        ("maxps", "max", "ps"),
        ("minpd", "min", "pd"),
        ("maxpd", "max", "pd"),
        ("cmpltss", "cmplt", "ss"),
        ("cmpnltps", "cmpnlt", "ps"),
    ] {
        let class = if suffix.ends_with('s') { "ps" } else { "pd" };
        for source in [
            "xmm15",
            if op == "cmpltss" {
                "dword ptr [rsi]"
            } else {
                "xmmword ptr [rsi]"
            },
        ] {
            add(
                format!("{op} xmm14,{source}"),
                format!(
                    "_mm_cast{class}_si128(_mm_{intrinsic}_{suffix}(_mm_castsi128_{class}(a),_mm_castsi128_{class}(b)))"
                ),
            );
        }
    }
    for (op, expression, size) in [
        (
            "cvtdq2ps",
            "_mm_castps_si128(_mm_cvtepi32_ps(b))",
            "xmmword",
        ),
        (
            "cvtps2dq",
            "_mm_cvtps_epi32(_mm_castsi128_ps(b))",
            "xmmword",
        ),
        (
            "cvttps2dq",
            "_mm_cvttps_epi32(_mm_castsi128_ps(b))",
            "xmmword",
        ),
        (
            "cvtps2pd",
            "_mm_castpd_si128(_mm_cvtps_pd(_mm_castsi128_ps(b)))",
            "qword",
        ),
        (
            "cvtpd2ps",
            "_mm_castps_si128(_mm_cvtpd_ps(_mm_castsi128_pd(b)))",
            "xmmword",
        ),
        ("cvtdq2pd", "_mm_castpd_si128(_mm_cvtepi32_pd(b))", "qword"),
        (
            "cvtpd2dq",
            "_mm_cvtpd_epi32(_mm_castsi128_pd(b))",
            "xmmword",
        ),
        (
            "cvttpd2dq",
            "_mm_cvttpd_epi32(_mm_castsi128_pd(b))",
            "xmmword",
        ),
    ] {
        for source in ["xmm15".into(), format!("{size} ptr [rsi]")] {
            add(format!("{op} xmm14,{source}"), expression.into());
        }
    }
    for (op, expression, size) in [
        (
            "shufps",
            "_mm_castps_si128(_mm_shuffle_ps(_mm_castsi128_ps(a),_mm_castsi128_ps(b),27))",
            "xmmword",
        ),
        (
            "shufpd",
            "_mm_castpd_si128(_mm_shuffle_pd(_mm_castsi128_pd(a),_mm_castsi128_pd(b),1))",
            "xmmword",
        ),
        ("pshufd", "_mm_shuffle_epi32(b,27)", "xmmword"),
        ("pshuflw", "_mm_shufflelo_epi16(b,27)", "xmmword"),
        ("pshufhw", "_mm_shufflehi_epi16(b,27)", "xmmword"),
        (
            "cmpps",
            "_mm_castps_si128(_mm_cmplt_ps(_mm_castsi128_ps(a),_mm_castsi128_ps(b)))",
            "xmmword",
        ),
        (
            "cmppd",
            "_mm_castpd_si128(_mm_cmplt_pd(_mm_castsi128_pd(a),_mm_castsi128_pd(b)))",
            "xmmword",
        ),
        (
            "cmpss",
            "_mm_castps_si128(_mm_cmplt_ss(_mm_castsi128_ps(a),_mm_castsi128_ps(b)))",
            "dword",
        ),
        (
            "cmpsd",
            "_mm_castpd_si128(_mm_cmplt_sd(_mm_castsi128_pd(a),_mm_castsi128_pd(b)))",
            "qword",
        ),
    ] {
        for source in ["xmm15".into(), format!("{size} ptr [rsi]")] {
            let imm = if op == "shufpd" || op.starts_with("cmp") {
                1
            } else {
                27
            };
            add(format!("{op} xmm14,{source},{imm}"), expression.into());
        }
    }
    for (op, intrinsic, immediate_only) in [
        ("psllw", "sll_epi16", false),
        ("pslld", "sll_epi32", false),
        ("psllq", "sll_epi64", false),
        ("psrlw", "srl_epi16", false),
        ("psrld", "srl_epi32", false),
        ("psrlq", "srl_epi64", false),
        ("psraw", "sra_epi16", false),
        ("psrad", "sra_epi32", false),
        ("pslldq", "slli_si128", true),
        ("psrldq", "srli_si128", true),
    ] {
        for imm in [0, 1, 15, 16, 31, 32, 63, 64, 255] {
            let intrinsic = if immediate_only {
                intrinsic.into()
            } else {
                intrinsic.replacen('_', "i_", 1)
            };
            add(
                format!("{op} xmm14,{imm}"),
                format!("_mm_{intrinsic}(a,{imm})"),
            );
        }
        if !immediate_only {
            for source in ["xmm15", "xmmword ptr [rsi]"] {
                add(
                    format!("{op} xmm14,{source}"),
                    format!("_mm_{intrinsic}(a,b)"),
                );
            }
        }
    }
    for (op, expression) in [
        ("pmovmskb", "_mm_movemask_epi8(b)"),
        ("movmskps", "_mm_movemask_ps(_mm_castsi128_ps(b))"),
        ("movmskpd", "_mm_movemask_pd(_mm_castsi128_pd(b))"),
    ] {
        for dst in ["r10d", "r10"] {
            add(
                format!("movabs r10,-1\n{op} {dst},xmm15\nmovq xmm14,r10"),
                format!("_mm_cvtsi64_si128((uint64_t){expression})"),
            );
        }
    }
    for op in ["movdqa", "movdqu"] {
        add(format!("{op} xmm14,xmm15"), "b".into());
        add(format!("{op} xmm14,xmmword ptr [rsi]"), "b".into());
        add(
            format!("{op} xmmword ptr [rdx],xmm14\nmovdqu xmm14,[rdx]"),
            "a".into(),
        );
    }
    cases
}

#[test]
fn baseline_vector_kernels_execute_against_intrinsics_when_x86_runner_is_available() {
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
    let root = std::env::var_os("KEEL_X86_FEATURE_ARTIFACTS").map(std::path::PathBuf::from);
    let dir = root.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("keel-x86-features-runtime-{}", std::process::id())),
        |p| p.join("runtime"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let cases = cases();
    let mut c = String::from(
        "#include <emmintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut wrappers = String::from(".text\n");
    let mut checks = String::new();
    let mut elf_source = String::from(".intel_syntax noprefix\n.text\n");
    for (i, (instruction, expression)) in cases.iter().enumerate() {
        let source = format!(
            "movdqu xmm14,[rdi]\nmovdqu xmm15,[rsi]\n{instruction}\nmovdqu [rdx],xmm14\nret\n"
        );
        let elf = assemble_object(&source).unwrap();
        let file = object::File::parse(&*elf).unwrap();
        let section = file.section_by_name(".text").unwrap();
        assert_eq!(section.relocations().count(), 0);
        let name = format!("{}f{i}", if darwin { "_" } else { "" });
        wrappers += &format!(
            ".globl {name}\n{name}:\n.byte {}\n",
            section
                .data()
                .unwrap()
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        elf_source += &format!(".globl f{i}\nf{i}:\n{source}");
        c += &format!(
            "extern void f{i}(const void*,const void*,void*);\n__attribute__((noinline)) static __m128i ref{i}(__m128i a,__m128i b) {{ (void)a;(void)b;return {expression}; }}\n"
        );
        checks += &format!(
            "{{ __m128i expected=ref{i}(a,b); f{i}(values[x],values[y],out); if(memcmp(out,&expected,16)) {{ fprintf(stderr,\"case {i} x=%u y=%u failed\\n\",x,y);return 1; }} }}\n"
        );
    }
    let mut scalar_checks = String::new();
    let mut scalar_count = 0;
    for (bits, size, r) in [
        (8, "byte", "r9b"),
        (16, "word", "r9w"),
        (32, "dword", "r9d"),
        (64, "qword", "r9"),
    ] {
        let mask = if bits == 64 {
            "UINT64_MAX".to_string()
        } else {
            format!("((1ULL<<{bits})-1)")
        };
        for op in [
            "add", "adc", "sub", "sbb", "and", "or", "xor", "xadd", "xchg", "cmpxchg", "inc",
            "dec", "not", "neg", "shl", "shr", "sar",
        ] {
            let initial = "old";
            let lo = format!("(old&{mask})");
            let value = match op {
                "add" | "adc" | "xadd" => "old+v".into(),
                "sub" | "sbb" => "old-v".into(),
                "and" => "old&v".into(),
                "or" => "old|v".into(),
                "xor" => "old^v".into(),
                "xchg" => "v".into(),
                "cmpxchg" => format!("({lo}==5?v:old)"),
                "inc" => "old+1".into(),
                "dec" => "old-1".into(),
                "not" => "~old".into(),
                "neg" => "0-old".into(),
                "shl" => "old<<1".into(),
                "shr" => format!("{lo}>>1"),
                "sar" => format!("({lo}>>1)|({lo}&(1ULL<<{}))", bits - 1),
                _ => unreachable!(),
            };
            let expected_mem = format!("({initial}&~{mask})|(({value})&{mask})");
            let expected_return = match op {
                "xadd" | "xchg" if bits < 32 => format!("(v&~{mask})|{lo}"),
                "xadd" | "xchg" => lo.clone(),
                "cmpxchg" => lo.clone(),
                _ => "expected_mem".into(),
            };
            let instruction = match op {
                "inc" | "dec" | "not" | "neg" => format!("lock\t{op} {size} ptr [rdi]"),
                "shl" | "shr" | "sar" => format!("{op} {size} ptr [rdi]"),
                _ => format!("lock\t{op} {size} ptr [rdi],{r}"),
            };
            let tail = match op {
                "xadd" | "xchg" => "mov rax,r9",
                "cmpxchg" => "",
                _ => "mov rax,[rdi]",
            };
            // XOR clears carry for ADC/SBB; CMPXCHG compares the low bits of 5.
            let source =
                format!("xor eax,eax\nmov r9,rsi\nmov eax,5\n{instruction}\n{tail}\nret\n");
            let elf = assemble_object(&source).unwrap();
            let file = object::File::parse(&*elf).unwrap();
            let section = file.section_by_name(".text").unwrap();
            assert_eq!(section.relocations().count(), 0);
            let name = format!("{}s{scalar_count}", if darwin { "_" } else { "" });
            wrappers += &format!(
                ".globl {name}\n{name}:\n.byte {}\n",
                section
                    .data()
                    .unwrap()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            elf_source += &format!(".globl s{scalar_count}\ns{scalar_count}:\n{source}");
            c += &format!("extern uint64_t s{scalar_count}(uint64_t*,uint64_t);\n");
            scalar_checks += &format!(
                "{{ uint64_t mem=old;uint64_t expected_mem={expected_mem};uint64_t expected_return={expected_return};uint64_t actual=s{scalar_count}(&mem,v);if(mem!=expected_mem||actual!=expected_return) {{ fprintf(stderr,\"scalar {op}{bits} failed old=%llx v=%llx\\n\",(unsigned long long)old,(unsigned long long)v);return 1; }} }}\n"
            );
            scalar_count += 1;
        }
    }
    c += "static int scalar_checks(void) { uint64_t values[]={0,1,5,7,15,16,31,32,63,64,255,256,0x80000000,0x8000000000000000ULL,~0ULL,0x76543210fedcba98ULL}; for(unsigned x=0;x<16;x++) { for(unsigned y=0;y<16;y++) { uint64_t old=values[x],v=values[y];\n";
    c += &scalar_checks;
    c += "} } return 0; }\n";
    c += r#"
int main(void) {
    if(scalar_checks()) return 1;
    _Alignas(16) uint32_t values[][4]={
        {0,0,0,0},{1,2,3,4},{0xffffffff,0xffffffff,0xffffffff,0xffffffff},
        {0x80000000,0x7fffffff,0x80008000,0x7fff7fff},
        {0x01234567,0x89abcdef,0xfedcba98,0x76543210},
        {0x3f800000,0xc0200000,0x7f800000,0xff800000},
        {0x7fc00000,0x80000000,0x00000001,0x7f7fffff},
        {0,0x3ff00000,0,0xc0040000}, {0,0x7ff80000,0,0x7ff00000},
        {1,0,0,0},{15,0,0,0},{16,0,0,0},{31,0,0,0},{32,0,0,0},
        {63,0,0,0},{64,0,0,0},{255,0,0,0},{0,1,0,0}
    };
    _Alignas(16) unsigned char out[16];
    for(unsigned x=0;x<sizeof(values)/sizeof(values[0]);x++) {
    for(unsigned y=0;y<sizeof(values)/sizeof(values[0]);y++) {
        __m128i a=_mm_load_si128((const __m128i*)values[x]);
        __m128i b=_mm_load_si128((const __m128i*)values[y]);
"#;
    c += &checks;
    c += &format!(
        "}} }}\nputs(\"{} vector kernels x 324 inputs + {scalar_count} scalar kernels x 256 inputs passed\"); return 0; }}\n",
        cases.len()
    );
    std::fs::write(dir.join("test.c"), c).unwrap();
    std::fs::write(dir.join("wrappers.s"), wrappers).unwrap();
    let linux_elf = assemble_object(&elf_source).unwrap();
    std::fs::write(dir.join("linux-kernels.s"), &elf_source).unwrap();
    std::fs::write(dir.join("linux-kernels.keel.o"), &linux_elf).unwrap();
    let reference = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("linux-kernels.s"))
        .arg("-o")
        .arg(dir.join("linux-kernels.clang.o"))
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let expected = std::fs::read(dir.join("linux-kernels.clang.o")).unwrap();
    let a = object::File::parse(&*linux_elf).unwrap();
    let b = object::File::parse(&*expected).unwrap();
    assert!(
        a.section_by_name(".text").unwrap().data().unwrap()
            == b.section_by_name(".text").unwrap().data().unwrap(),
        "runtime Linux ELF text differs from Clang"
    );
    let mut clang = Command::new("clang");
    if darwin {
        clang.args(["-target", "x86_64-apple-darwin"]);
    }
    let result = clang
        .args(["-O2", "-msse2", "-mno-avx", "-Wall", "-Wextra", "-Werror"])
        .arg(dir.join("test.c"))
        .arg(dir.join("wrappers.s"))
        .arg("-o")
        .arg(dir.join("test"))
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
            .arg(dir.join("test"))
            .output()
    } else {
        Command::new(dir.join("test")).output()
    }
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&result.stdout));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    if root.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

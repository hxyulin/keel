//! Runtime cases are emitted and syntax checked everywhere; executed only on
//! Linux/x86-64 with the actual independent BMI feature bits present.
use keel_asm::x86::assemble_object_with_features;
use keel_asm_ir::x86::Features;
use std::process::Command;
const BMI: Features = Features {
    avx: false,
    bmi1: true,
    bmi2: true,
    ..Features::legacy()
};

fn fixtures(bmi2: bool) -> (String, String) {
    let ops = if bmi2 {
        &[
            "shlx", "shrx", "sarx", "rorx", "bzhi", "pext", "pdep", "mulhi", "mullo",
        ][..]
    } else {
        &["andn", "bextr", "blsi", "blsmsk", "blsr", "tzcnt"][..]
    };
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    let mut c = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    let mut checks = String::new();
    for bits in [32, 64] {
        let reg = |n| {
            if bits == 64 {
                ["rax", "r10", "r11"][n]
            } else {
                ["eax", "r10d", "r11d"][n]
            }
        };
        for op in ops {
            for mode in ["reg", "alias_first", "alias_last", "memory"] {
                let name = format!("{op}{bits}_{mode}");
                c += &format!("extern uint64_t {name}(uint64_t,uint64_t);\n");
                asm += &format!(".globl {name}\n{name}:\nmov r10,rdi\nmov r11,rsi\n");
                let memory = mode == "memory";
                let alias = mode.starts_with("alias");
                if memory {
                    asm += "sub rsp,8\n";
                    asm += if matches!(*op, "andn" | "pext" | "pdep") {
                        "mov [rsp],rsi\n"
                    } else {
                        "mov [rsp],rdi\n"
                    };
                }
                let mem_or = |r| if memory { "[rsp]" } else { reg(r) };
                let dst = match mode {
                    "alias_first" => reg(1),
                    "alias_last" => reg(2),
                    _ => reg(0),
                };
                let instruction = match *op {
                    "andn" | "pext" | "pdep" => format!("{op} {dst},{},{}", reg(1), mem_or(2)),
                    "bextr" | "bzhi" | "shlx" | "shrx" | "sarx" => {
                        format!("{op} {dst},{},{}", mem_or(1), reg(2))
                    }
                    "rorx" => format!("rorx {dst},{},255", mem_or(1)),
                    "mulhi" | "mullo" => {
                        asm += "mov rdx,rsi\n";
                        let other = if dst == reg(1) { reg(2) } else { reg(1) };
                        let (hi, lo) = if *op == "mulhi" {
                            (dst, other)
                        } else {
                            (other, dst)
                        };
                        format!("mulx {hi},{lo},{}", mem_or(1))
                    }
                    _ => format!("{op} {dst},{}", mem_or(1)),
                };
                asm += &format!("{instruction}\n");
                if alias {
                    asm += &format!("mov {},{dst}\n", reg(0));
                }
                if memory {
                    asm += "add rsp,8\n";
                }
                asm += "ret\n";
                let mask = if bits == 64 { "~0ULL" } else { "0xffffffffULL" };
                let expression = match *op {
                    "andn" => "(~a)&b".into(),
                    "bextr" => format!("extract(a,b,{bits})"),
                    "blsi" => "a&(-a)".into(),
                    "blsmsk" => "a^(a-1)".into(),
                    "blsr" => "a&(a-1)".into(),
                    "tzcnt" => format!("trailing(a,{bits})"),
                    "shlx" => format!("a<<(b&{})", bits - 1),
                    "shrx" => format!("a>>(b&{})", bits - 1),
                    "sarx" => format!("(uint64_t)((int{bits}_t)a>>(b&{}))", bits - 1),
                    "rorx" => format!("(a>>{})|(a<<1)", bits - 1),
                    "bzhi" => format!("((b&255)>={bits}?a:(a&((1ULL<<(b&255))-1)))"),
                    "pext" => format!("extract_parallel(a,b,{bits})"),
                    "pdep" => format!("deposit_parallel(a,b,{bits})"),
                    "mulhi" => format!("(uint64_t)(((__uint128_t)a*b)>>{bits})"),
                    "mullo" => "a*b".into(),
                    _ => unreachable!(),
                };
                checks += &format!(
                    "{{ uint64_t a=values[i]&{mask},b=values[j]&{mask}; (void)b; uint64_t expected=({expression})&{mask}; uint64_t actual={name}(values[i],values[j]); if(actual!=expected) {{ fprintf(stderr,\"{name} i=%u j=%u actual=%llx expected=%llx\\n\",i,j,(unsigned long long)actual,(unsigned long long)expected); return 1; }} }}\n"
                );
            }
        }
    }
    if bmi2 {
        for op in ["shlx", "shrx", "sarx", "rorx", "pext", "pdep", "mulx"] {
            for state in [0, 1] {
                let name = format!("flags_{op}_{state}");
                c += &format!("extern uint64_t {name}(uint64_t,uint64_t);\n");
                asm += &format!(
                    ".globl {name}\n{name}:\nmov r10,rdi\nmov r11,rsi\nmov rdx,rsi\nmov ecx,0\ncmp rcx,{state}\n"
                );
                asm += &match op {
                    "rorx" => "rorx rax,r10,255\n".into(),
                    "mulx" => "mulx rax,r8,r10\n".into(),
                    _ => format!("{op} rax,r10,r11\n"),
                };
                asm += if state == 0 {
                    "setne al\nsetb dl\n"
                } else {
                    "sete al\nsetae dl\n"
                };
                asm += "or al,dl\nmovzx eax,al\nret\n";
                checks += &format!(
                    "if({name}(values[i],values[j])) {{ fprintf(stderr,\"{name} changed flags\\n\"); return 2; }}\n"
                );
            }
        }
    }
    asm += ".section .note.GNU-stack,\"\",@progbits\n";
    c += r#"
static uint64_t extract(uint64_t a,uint64_t b,unsigned bits) {
    unsigned start=b&255, len=(b>>8)&255;
    if(start>=bits) return 0;
    unsigned n=bits-start; if(len<n) n=len;
    uint64_t mask=n==64?~0ULL:((1ULL<<n)-1);
    return (a>>start)&mask;
}
static uint64_t trailing(uint64_t a,unsigned bits) {
    if(!a) return bits;
    unsigned n=0; while(!(a&1)) { a>>=1; n++; } return n;
}
static uint64_t extract_parallel(uint64_t a,uint64_t mask,unsigned bits) {
    uint64_t result=0; unsigned out=0;
    for(unsigned k=0;k<bits;k++) if((mask>>k)&1) { result|=((a>>k)&1)<<out; out++; }
    return result;
}
static uint64_t deposit_parallel(uint64_t a,uint64_t mask,unsigned bits) {
    uint64_t result=0; unsigned in=0;
    for(unsigned k=0;k<bits;k++) if((mask>>k)&1) { result|=((a>>in)&1)<<k; in++; }
    return result;
}
int main(void) {
    uint64_t values[]={0,1,2,7,8,15,16,31,32,63,64,65,127,128,255,256,
        0x80000000,0x8000000000000000ULL,~0ULL,0x76543210fedcba98ULL,
        0x100,0x1f00,0x2000,0x3f00,0x4000,0xff00,0xff20,0xff40,0x0120,0x013f};
    for(unsigned i=0;i<sizeof(values)/8;i++) for(unsigned j=0;j<sizeof(values)/8;j++) {
"#;
    c += &checks;
    c += "}\nreturn 0; }\n";
    (asm, c)
}

#[test]
fn bmi_runtime_fixtures_assemble_and_harnesses_compile() {
    let temp = std::env::temp_dir().join(format!("keel-bmi-runtime-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    for bmi2 in [false, true] {
        let (asm, c) = fixtures(bmi2);
        assemble_object_with_features(&asm, BMI).unwrap();
        let input = temp.join("test.c");
        std::fs::write(&input, c).unwrap();
        let output = Command::new("clang")
            .args([
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-function",
                "-fsyntax-only",
            ])
            .arg(input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn bmi_assembler_executes_zero_boundaries_memory_high_registers_and_aliases() {
    let temp = std::env::temp_dir().join(format!("keel-bmi-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    for bmi2 in [false, true] {
        if bmi2 && !std::is_x86_feature_detected!("bmi2")
            || !bmi2 && !std::is_x86_feature_detected!("bmi1")
        {
            continue;
        }
        let (asm, c) = fixtures(bmi2);
        let features = Features {
            bmi1: !bmi2,
            bmi2,
            avx: false,
            ..Features::legacy()
        };
        let input = temp.join("test.c");
        let object = temp.join("test.o");
        let executable = temp.join("test");
        std::fs::write(&input, c).unwrap();
        std::fs::write(
            &object,
            assemble_object_with_features(&asm, features).unwrap(),
        )
        .unwrap();
        let output = Command::new("clang")
            .args(["-O2"])
            .arg(input)
            .arg(object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Command::new(executable).status().unwrap().success());
    }
    std::fs::remove_dir_all(temp).unwrap();
}

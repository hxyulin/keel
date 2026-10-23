//! Bounded strict-v3 join: mixed-width SysV transport through the shared SIMD bank.
//! Architectural FP/mask/gather semantics stay in their existing finite suites.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use std::fs;
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
#[path = "common/userspace.rs"]
mod userspace;

fn mixed() -> (String, String) {
    // Eight shared SIMD argument registers, then a narrow and a wide stack
    // argument. Scalar FP and integer vectors must share that same bank.
    let shapes = [
        ("f32x4", "float", 16),
        ("f32x8", "float", 32),
        ("f64", "double", 8),
        ("i32x8", "int", 32),
        ("f64x2", "double", 16),
        ("f32", "float", 4),
        ("i64x4", "long", 32),
        ("f64x4", "double", 32),
        ("f32x4", "float", 16),
        ("f32x8", "float", 32),
    ];
    let types = shapes
        .iter()
        .map(|s| s.0)
        .chain(["ptr", "ptr"])
        .collect::<Vec<_>>()
        .join(",");
    let params = shapes
        .iter()
        .enumerate()
        .map(|(i, s)| format!("v{i}:{}", s.0))
        .chain(["v10:ptr".into(), "v11:ptr".into()])
        .collect::<Vec<_>>()
        .join(",");
    let args = (0..12)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut ir = format!(
        "decl @clobber()\ndecl @mixed_target({types})->f32x8\nfunc @mixed({types})->f32x8 {{\nblock0({params}):\n call @clobber()\n"
    );
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    for (ty, ct, bytes) in shapes {
        if bytes > 8 {
            c += &format!("typedef {ct} V_{ty} __attribute__((vector_size({bytes})));\n");
        }
    }
    let ctype = |ty: &str| match ty {
        "f32" => "float".into(),
        "f64" => "double".into(),
        _ => format!("V_{ty}"),
    };
    let cparams = shapes
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{} a{i}", ctype(s.0)))
        .chain(["unsigned char*out".into(), "void*fn".into()])
        .collect::<Vec<_>>()
        .join(",");
    let mut copies = String::new();
    let mut init = String::new();
    for (i, (ty, _, bytes)) in shapes.iter().enumerate() {
        let v = 12 + i * 2;
        ir += &format!(
            " v{v} = iconst.i64 {}\n v{} = ptradd v10,v{v}\n store align 1 v{i},v{}\n",
            i * 32,
            v + 1,
            v + 1
        );
        copies += &format!("memcpy(out+{},&a{i},{bytes});", i * 32);
        init += &format!("{} a{i};", ctype(ty));
        if *bytes > 8 {
            init += &format!(
                "for(unsigned j=0;j<sizeof(a{i})/sizeof(a{i}[0]);j++)a{i}[j]=k+{}+j;",
                i + 1
            );
        } else {
            init += &format!("a{i}=k+{}.25;", i + 1);
        }
    }
    ir += " return v1\n}\n";
    for (name, operation) in [
        (
            "direct",
            format!("v32 = call @mixed_target({args})\n return v32"),
        ),
        (
            "indirect",
            format!("v32 = call_indirect v11({args}) : ({types})->f32x8\n return v32"),
        ),
        ("tail", format!("return_call @mixed_target({args})")),
        (
            "tail_indirect",
            format!("return_call_indirect v11({args}) : ({types})->f32x8"),
        ),
    ] {
        ir += &format!("func @{name}({types})->f32x8 {{\nblock0({params}):\n {operation}\n}}\n");
    }
    c += &format!("typedef V_f32x8 (*Fn)({cparams});\n");
    for name in ["mixed", "direct", "indirect", "tail", "tail_indirect"] {
        c += &format!("extern V_f32x8 {name}({cparams});\n");
    }
    c += &format!(
        "__attribute__((noinline)) void clobber(void){{asm volatile(\"vzeroall\" ::: \"xmm0\",\"xmm1\",\"xmm2\",\"xmm3\",\"xmm4\",\"xmm5\",\"xmm6\",\"xmm7\",\"xmm8\",\"xmm9\",\"xmm10\",\"xmm11\",\"xmm12\",\"xmm13\",\"xmm14\",\"xmm15\");}}\nV_f32x8 mixed_target({cparams}){{(void)fn;{copies}return a1;}}\n"
    );
    let cargs = (0..10)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",");
    c += &format!(
        "int main(void){{Fn fns[]={{mixed,direct,indirect,tail,tail_indirect}};for(unsigned k=0;k<32;k++){{{init}unsigned char expected[320],actual[322];memset(expected,0xa5,320);mixed_target({cargs},expected,(void*)mixed_target);for(unsigned f=0;f<5;f++){{memset(actual,0xa5,322);V_f32x8 r=fns[f]({cargs},actual+1,(void*)mixed_target);if(memcmp(&r,&a1,32)||memcmp(expected,actual+1,320)||actual[0]!=0xa5||actual[321]!=0xa5)return 10+f;}}}}return 0;}}\n"
    );
    (ir, c)
}

#[test]
fn strict_v3_mixed_width_register_stack_calls_tails_and_live_spills() {
    let (ir, c) = mixed();
    let export = std::env::var_os("KEEL_V3_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("mixed.ir"), &ir).unwrap();
        fs::write(d.join("mixed.c"), &c).unwrap();
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (object, text) = userspace::compile(&ir, CpuLevel::V3, s, a, spilled);
                assert!(text.contains("ymm"));
                if let Some(d) = &export {
                    let stem = format!("mixed-{s:?}-{a:?}-{spilled}");
                    fs::write(d.join(format!("{stem}.o")), &object).unwrap();
                    fs::write(d.join(format!("{stem}.s")), &text).unwrap();
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                if std::is_x86_feature_detected!("avx2") {
                    use std::process::Command;
                    let d = common::temp("strict-v3-mixed");
                    fs::write(d.join("test.o"), &object).unwrap();
                    fs::write(d.join("test.c"), &c).unwrap();
                    for compiler in ["clang", "gcc"] {
                        common::command(
                            Command::new(compiler)
                                .args(["-O2", "-march=x86-64-v3", "-ffp-contract=off"])
                                .arg(d.join("test.c"))
                                .arg(d.join("test.o"))
                                .arg("-o")
                                .arg(d.join("test")),
                        );
                        common::command(&mut Command::new(d.join("test")));
                    }
                    fs::remove_dir_all(d).unwrap();
                }
            }
        }
    }
}

#[test]
fn strict_v3_count_and_endian_assembly_exact_widths() {
    use keel_asm_ir::x86::Features;
    let mut assembly = String::from(".intel_syntax noprefix\n.text\n");
    for (bits, r, src, mem) in [
        (16, "ax", "di", "word"),
        (32, "eax", "edi", "dword"),
        (64, "rax", "rdi", "qword"),
    ] {
        for op in ["lzcnt", "tzcnt"] {
            assembly += &format!(".globl {op}{bits}\n{op}{bits}:\n {op} {r},{src}\n");
            if bits == 16 {
                assembly += " movzx eax,ax\n";
            }
            assembly += " ret\n";
        }
        assembly += &format!(
            ".globl endian{bits}\nendian{bits}:\n movbe {r},{mem} ptr [rdi]\n movbe {mem} ptr [rsi],{r}\n"
        );
        if bits == 16 {
            assembly += " movzx eax,ax\n";
        }
        assembly += " ret\n";
    }
    let features = Features::for_level(CpuLevel::V3);
    assert!(!features.clflush);
    assert!(features.avx && features.avx2 && features.xsave && features.osxsave);
    assert!(
        keel_asm::x86::assemble_object_with_features(&assembly, Features::for_level(CpuLevel::V2))
            .is_err()
    );
    let object = keel_asm::x86::assemble_object_with_features(&assembly, features).unwrap();
    if let Some(d) = std::env::var_os("KEEL_V3_EXPORT_DIR") {
        let d = std::path::PathBuf::from(d);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("count-endian.o"), &object).unwrap();
        fs::write(d.join("count-endian.s"), &assembly).unwrap();
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("lzcnt")
        && std::is_x86_feature_detected!("bmi1")
        && std::is_x86_feature_detected!("movbe")
    {
        use std::process::Command;
        let d = common::temp("strict-v3-count-endian");
        fs::write(d.join("test.o"), object).unwrap();
        fs::write(d.join("test.c"), COUNT_ENDIAN).unwrap();
        common::command(
            Command::new("cc")
                .args(["-O2", "-march=x86-64-v3"])
                .arg(d.join("test.c"))
                .arg(d.join("test.o"))
                .arg("-o")
                .arg(d.join("test")),
        );
        common::command(&mut Command::new(d.join("test")));
        fs::remove_dir_all(d).unwrap();
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const COUNT_ENDIAN: &str = r#"
#define _GNU_SOURCE
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern unsigned lzcnt16(unsigned),tzcnt16(unsigned),lzcnt32(unsigned),tzcnt32(unsigned);
extern uint64_t lzcnt64(uint64_t),tzcnt64(uint64_t);
extern uint64_t endian64(const void*,void*);
extern unsigned endian16(const void*,void*),endian32(const void*,void*);
int main(void){
 uint64_t state=0x123456789abcdef0ULL;
 for(unsigned k=0;k<4096;k++){
  state=state*6364136223846793005ULL+1;
  for(unsigned n=0;n<3;n++){
   unsigned bits=16u<<n;uint64_t x=k<65?(k==64?0:1ULL<<k):state;
   if(bits<64)x&=(1ULL<<bits)-1;
   unsigned l=bits,t=bits;for(unsigned j=0;j<bits;j++)if(x&(1ULL<<j)){if(t==bits)t=j;l=bits-1-j;}
   uint64_t a=n==0?lzcnt16((unsigned)x):n==1?lzcnt32((unsigned)x):lzcnt64(x);
   uint64_t b=n==0?tzcnt16((unsigned)x):n==1?tzcnt32((unsigned)x):tzcnt64(x);
   if(a!=l||b!=t)return 10+n;
  }
 }
 long p=sysconf(_SC_PAGESIZE);if(p<64)return 20;
 unsigned char*m=mmap(0,3*p,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(m==MAP_FAILED||mprotect(m+p,p,PROT_READ|PROT_WRITE))return 21;
 for(unsigned n=0;n<3;n++)for(unsigned edge=0;edge<3;edge++)for(unsigned alias=0;alias<2;alias++){
  unsigned bytes=2u<<n;unsigned char*src=m+p+(edge==0?0:edge==1?p-bytes:1),out[10];
  for(unsigned j=0;j<bytes;j++)src[j]=17+j*31;
  uint64_t expected=0;for(unsigned j=0;j<bytes;j++)expected=(expected<<8)|src[j];
  unsigned char original[8];memcpy(original,src,bytes);memset(out,0xa5,sizeof(out));
  unsigned char*dst=alias?src:out+1;
  if(src>m+p)src[-1]=0xa5;if(src+bytes<m+2*p)src[bytes]=0xa5;
  uint64_t result=n==0?endian16(src,dst):n==1?endian32(src,dst):endian64(src,dst);
  if(result!=expected||memcmp(dst,original,bytes))return 30+n;
  if(out[0]!=0xa5||out[bytes+1]!=0xa5)return 40;
  if(src>m+p&&src[-1]!=0xa5)return 41;if(src+bytes<m+2*p&&src[bytes]!=0xa5)return 42;
 }
 munmap(m,3*p);return 0;
}
"#;

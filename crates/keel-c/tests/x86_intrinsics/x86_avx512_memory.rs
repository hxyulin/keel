//! Memory aliases preserve exact widths, scalar argument evaluation and masks.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn fixtures() -> (String, String, String) {
    let mut plain = String::from("#include <immintrin.h>\n");
    let mut masked = String::new();
    let mut oracle = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for (bytes, prefix, integer) in [(16, "_mm", "__m128i"), (32, "_mm256", "__m256i")] {
        for bits in [8, 16, 32, 64] {
            for u in if bits < 32 {
                &["u"][..]
            } else {
                &["", "u"][..]
            } {
                let name = format!("copy_{bytes}_{bits}_{u}");
                plain += &format!(
                    "void {name}(void*p,void*q,void*m,unsigned k,unsigned*c){{unsigned np=0,nk=0,nv=0;{integer} v={prefix}_load{u}_epi{bits}((np++,p));{prefix}_store{u}_epi{bits}((np++,q),v);*c=np+nk+nv;}}\n"
                );
                oracle += &format!("extern void {name}(void*,void*,void*,unsigned,unsigned*);\n");
                cases += &format!(
                    "{{{name},{bytes},{bits},0,{},2}},\n",
                    u32::from(!u.is_empty())
                );
            }
        }
        for (suffix, bits, vector) in [
            ("epi32", 32, integer),
            ("epi64", 64, integer),
            ("ps", 32, if bytes == 16 { "__m128" } else { "__m256" }),
            ("pd", 64, if bytes == 16 { "__m128d" } else { "__m256d" }),
        ] {
            for op in 1..=3 {
                let name = format!("masked_{bytes}_{suffix}_{op}");
                let body = match op {
                    1 => format!(
                        "v={prefix}_mask_load_{suffix}((nv++,v),(nk++,k),(np++,p));__builtin_memcpy(q,&v,{bytes});"
                    ),
                    2 => format!(
                        "v={prefix}_maskz_load_{suffix}((nk++,k),(np++,p));__builtin_memcpy(q,&v,{bytes});"
                    ),
                    _ => format!(
                        "__builtin_memcpy(&v,p,{bytes});{prefix}_mask_store_{suffix}((np++,q),(nk++,k),(nv++,v));"
                    ),
                };
                masked += &format!(
                    "__attribute__((target(\"avx512vl\"))) void {name}(void*p,void*q,void*m,unsigned k,unsigned*c){{unsigned np=0,nk=0,nv=0;{vector} v;__builtin_memcpy(&v,m,{bytes});{body}*c=np+nk+nv;}}\n"
                );
                oracle += &format!("extern void {name}(void*,void*,void*,unsigned,unsigned*);\n");
                cases += &format!(
                    "{{{name},{bytes},{bits},{op},0,{}}},\n",
                    if op == 2 { 2 } else { 3 }
                );
            }
        }
    }
    oracle += &format!(
        "struct testcase{{void(*f)(void*,void*,void*,unsigned,unsigned*);unsigned bytes,bits,op,unaligned,calls;}};static struct testcase cases[]={{\n{cases}}};\n"
    );
    oracle += r#"
int main(void){long page=sysconf(_SC_PAGESIZE);unsigned char*g=mmap(0,page*6,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(g==MAP_FAILED||mprotect(g+page,page,PROT_READ|PROT_WRITE)||mprotect(g+page*4,page,PROT_READ|PROT_WRITE))return 1;unsigned char merge[64] __attribute__((aligned(64))),want[64];for(unsigned j=0;j<64;j++)merge[j]=(unsigned char)(j*19+3);unsigned masks[]={0,1,0x55,0x80,0xff};for(unsigned ci=0;ci<sizeof cases/sizeof*cases;ci++){struct testcase t=cases[ci];for(unsigned mi=0;mi<5;mi++){unsigned k=masks[mi],active=k&((1ULL<<(t.bytes*8/t.bits))-1),n=0;unsigned char*p=g+page*2-t.bytes-t.unaligned,*q=g+page*5-t.bytes-t.unaligned;memset(g+page*4,0xa5,page);for(unsigned j=0;j<t.bytes;j++)p[j]=(unsigned char)(j*37+11);memcpy(want,t.op==1?merge:p,t.bytes);if(t.op==2)memset(want,0,t.bytes);if(t.op==3)memset(want,0xa5,t.bytes);if(t.op)for(unsigned lane=0;lane<t.bytes*8/t.bits;lane++)if(k&(1u<<lane))memcpy(want+lane*t.bits/8,p+lane*t.bits/8,t.bits/8);void*input=(!active&&(t.op==1||t.op==2))?g:p;void*output=(!active&&t.op==3)?g+page*3:q;t.f(input,output,merge,k,&n);if(n!=t.calls||memcmp(q,want,t.bytes))return 2;for(long j=0;j<page;j++)if((g+page*4+j<q||g+page*4+j>=q+t.bytes)&&g[page*4+j]!=0xa5)return 3;for(unsigned j=0;j<t.bytes;j++)if(p[j]!=(unsigned char)(j*37+11))return 4;}}munmap(g,page*6);return 0;}
"#;
    let all = plain.clone() + &masked;
    (plain, all, oracle)
}

#[test]
fn memory_aliases_preserve_width_masks_and_single_evaluation() {
    let (plain, source, oracle) = fixtures();
    let base = Features::NONE.with_features("avx").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let plain = parse(&plain, Target::X86_64LinuxGnu).unwrap();
    keel_codegen::x86_64::compile_program_with_pipeline(&plain, &EMPTY, &pipeline).unwrap();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.avx512bw && !features.avx512dq && !features.avx2);
    let without = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&without, &EMPTY, &pipeline).is_err()
    );
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("keel-memory-alias-{}-{unique}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512vl")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        fs::write(directory.join("oracle.c"), oracle).unwrap();
        let output = Command::new("cc")
            .args(["-O2", "-c"])
            .arg(directory.join("oracle.c"))
            .arg("-o")
            .arg(directory.join("oracle.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        if native {
            fs::write(directory.join("memory.o"), object).unwrap();
            let output = Command::new("cc")
                .arg(directory.join("oracle.o"))
                .arg(directory.join("memory.o"))
                .arg("-o")
                .arg(directory.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}

//! C consumers retain the scalar register, load and store contracts.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[test]
fn scalar_move_headers_preserve_upper_bits_and_suppress_inactive_access() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (suffix, container, packed) in [("ss", "__m128", "ps"), ("sd", "__m128d", "pd")] {
        source += &format!(
            "void register_{suffix}(const void*s,const void*a,const void*b,void*out,__mmask8 k){{{container} r=_mm_mask_move_{suffix}(*(const {container}*)s,k,*(const {container}*)a,*(const {container}*)b);_mm_storeu_{packed}(out,r);}}\nvoid load_{suffix}(const void*s,const void*p,void*out,__mmask8 k){{_mm_storeu_{packed}(out,_mm_mask_load_{suffix}(*(const {container}*)s,k,p));}}\nvoid store_{suffix}(const void*s,void*p,__mmask8 k){{_mm_mask_store_{suffix}(p,k,*(const {container}*)s);}}\nvoid zero_{suffix}(const void*a,const void*b,void*out,__mmask8 k){{_mm_storeu_{packed}(out,_mm_maskz_move_{suffix}(k,*(const {container}*)a,*(const {container}*)b));}}\nvoid zero_load_{suffix}(const void*p,void*out,__mmask8 k){{_mm_storeu_{packed}(out,_mm_maskz_load_{suffix}(k,p));}}\n"
        );
    }
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for name in [
        "maskmovess",
        "maskmovesd",
        "maskloadss",
        "maskloadsd",
        "maskstoress",
        "maskstoresd",
    ] {
        assert!(
            ir.contains(&format!("x86_avx512_{name}")),
            "missing {name}: {ir}"
        );
    }
    assert!(parse(&source, Target::Aarch64LinuxGnu).is_err());
    let features = Features::NONE.with_features("avx512f").unwrap();
    let directory = std::env::temp_dir().join(format!("keel-c-scalar-move-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for (suffix, uint, lanes, width) in [("ss", "uint32_t", 4, 4), ("sd", "uint64_t", 2, 8)] {
        harness += &format!(
            "extern void register_{suffix}(const void*,const void*,const void*,void*,uint8_t),load_{suffix}(const void*,const void*,void*,uint8_t),store_{suffix}(const void*,void*,uint8_t),zero_{suffix}(const void*,const void*,void*,uint8_t),zero_load_{suffix}(const void*,void*,uint8_t);\n"
        );
        cases += &format!(
            "{{{uint} merge[{lanes}],upper[{lanes}],low[{lanes}],out[{lanes}],want[{lanes}];for(unsigned i=0;i<{lanes};i++){{merge[i]=({uint})(UINT64_C(0x7ff012347f812345)+i);upper[i]=~({uint})0-i*31;low[i]=i*17;}}uint8_t masks[]={{0,1,0xfe,0xff}};for(unsigned j=0;j<4;j++){{unsigned csr=(saved&~0x1f80u)|0x3f;_mm_setcsr(csr);memcpy(want,upper,16);want[0]=(masks[j]&1)?low[0]:merge[0];register_{suffix}(merge,upper,low,out,masks[j]);if(memcmp(out,want,16)||_mm_getcsr()!=csr)return 1;want[0]=(masks[j]&1)?low[0]:0;zero_{suffix}(upper,low,out,masks[j]);if(memcmp(out,want,16)||_mm_getcsr()!=csr)return 2;unsigned char*p=(masks[j]&1)?m+page-{width}:m+page;if(masks[j]&1)memcpy(p,low,{width});memset(want,0,16);want[0]=(masks[j]&1)?low[0]:merge[0];load_{suffix}(merge,p,out,masks[j]);if(memcmp(out,want,16)||_mm_getcsr()!=csr)return 3;want[0]=(masks[j]&1)?low[0]:0;zero_load_{suffix}(p,out,masks[j]);if(memcmp(out,want,16)||_mm_getcsr()!=csr)return 4;memset(m+page-{width}-1,0xa5,{width}+1);store_{suffix}(merge,p,masks[j]);if(masks[j]&1){{if(memcmp(p,merge,{width}))return 5;}}else for(unsigned i=0;i<{width};i++)if(m[page-{width}+i]!=0xa5)return 6;if(m[page-{width}-1]!=0xa5||_mm_getcsr()!=csr)return 7;_mm_setcsr(saved);}}}}\n"
        );
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m,page,PROT_READ|PROT_WRITE))return 8;{cases}_mm_setcsr(saved);munmap(m,page*2);return 0;}}\n"
    );
    fs::write(directory.join("harness.c"), harness).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("consumer.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                eprintln!("SKIP native scalar move C consumers: AVX512F unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O2", "-mavx512f", "-frounding-math"])
                .arg(directory.join("harness.c"))
                .arg(directory.join("consumer.o"))
                .arg("-o")
                .arg(directory.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(directory.join("run")).status().unwrap();
            assert!(status.success(), "{status}");
        }
    }
}

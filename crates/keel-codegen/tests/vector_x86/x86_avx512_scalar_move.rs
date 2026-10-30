//! Scalar move oracles distinguish load zeroing, register passthrough and exact memory widths.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <unistd.h>\n#include <signal.h>\n#include <sys/prctl.h>\n",
    );
    let mut cases = String::new();
    for (suffix, shape, scalar, uint, lanes, width, seed) in [
        ("ss", "f32x4", "float", "uint32_t", 4, 4, "0x7f812345u"),
        (
            "sd",
            "f64x2",
            "double",
            "uint64_t",
            2,
            8,
            "UINT64_C(0x7ff0123456789abc)",
        ),
    ] {
        for alias in 0..3 {
            let upper = if alias == 1 { "v5" } else { "v6" };
            let low = if alias == 2 { "v5" } else { "v7" };
            ir += &format!(
                "func @move_{suffix}_{alias}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{shape} align 1 v2\n v8=intrinsic x86_avx512_maskmove{suffix}(v5,v4,{upper},{low})\n store align 1 v8,v3\n v9=iconst.i64 16\n v10=ptradd v3,v9\n store align 1 v5,v10\n v11=ptradd v10,v9\n store align 1 v6,v11\n v12=ptradd v11,v9\n store align 1 v7,v12\n return\n}}\n"
            );
            c += &format!("extern void move_{suffix}_{alias}(void*,void*,void*,void*,uint8_t);\n");
            let a = if alias == 1 { "merge" } else { "upper" };
            let b = if alias == 2 { "merge" } else { "low" };
            let packed = if suffix == "ss" { "ps" } else { "pd" };
            let vector = if suffix == "ss" { "__m128" } else { "__m128d" };
            cases += &format!(
                "{{ {uint} merge[{lanes}],upper[{lanes}],low[{lanes}],want[{lanes}],out[{lanes}*4];for(unsigned i=0;i<{lanes};i++){{merge[i]=({uint})({seed}+i*19);upper[i]=({uint})(~({uint})0-i*37);low[i]=({uint})({seed}-i*53);}}uint8_t masks[]={{0,1,0xfe,0xff}};for(unsigned j=0;j<4;j++){{memcpy(want,{a},16);want[0]=(masks[j]&1)?{b}[0]:merge[0];unsigned csr=(saved&~0x1f80u)|0x3f;_mm_setcsr(csr);{vector} oracle=_mm_mask_move_{suffix}(_mm_loadu_{packed}((void*)merge),masks[j],_mm_loadu_{packed}((void*){a}),_mm_loadu_{packed}((void*){b}));if(memcmp(&oracle,want,16)||_mm_getcsr()!=csr)return 1;move_{suffix}_{alias}(merge,upper,low,out,masks[j]);if(_mm_getcsr()!=csr||memcmp(out,want,16)||memcmp(out+{lanes},merge,16)||memcmp(out+2*{lanes},upper,16)||memcmp(out+3*{lanes},low,16))return 2;_mm_setcsr(saved);}} }}\n"
            );
        }
        ir += &format!(
            "func @load_{suffix}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{shape} align 1 v0\n v5=intrinsic x86_avx512_maskload{suffix}(v1,v4,v3)\n store align 1 v5,v2\n v6=iconst.i64 16\n v7=ptradd v2,v6\n store align 1 v4,v7\n return\n}}\nfunc @store_{suffix}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{shape} align 1 v0\n intrinsic x86_avx512_maskstore{suffix}(v1,v4,v3)\n store align 1 v4,v2\n return\n}}\n"
        );
        c += &format!(
            "extern void load_{suffix}(void*,void*,void*,uint8_t),store_{suffix}(void*,void*,void*,uint8_t);\n"
        );
        let packed = if suffix == "ss" { "ps" } else { "pd" };
        let vector = if suffix == "ss" { "__m128" } else { "__m128d" };
        cases += &format!(
            "{{ {uint} merge[{lanes}],out[{lanes}*2],want[{lanes}],bits={seed};for(unsigned i=0;i<{lanes};i++)merge[i]=({uint})(~({uint})0-i*71);uint8_t masks[]={{0,1,0xfe,0xff}};for(unsigned j=0;j<4;j++){{for(unsigned position=0;position<2;position++){{unsigned char*p=position?m+1:m+page-{width};memcpy(p,&bits,{width});memset(want,0,16);want[0]=(masks[j]&1)?bits:merge[0];unsigned char*address=(masks[j]&1)?p:m+page;unsigned csr=(saved&~0x1f80u)|0x3f;_mm_setcsr(csr);{vector} oracle=_mm_mask_load_{suffix}(_mm_loadu_{packed}((void*)merge),masks[j],(const {scalar}*)address);if(memcmp(&oracle,want,16)||_mm_getcsr()!=csr)return 3;load_{suffix}(merge,address,out,masks[j]);if(memcmp(out,want,16)||memcmp(out+{lanes},merge,16)||_mm_getcsr()!=csr)return 4;_mm_setcsr(saved);}}unsigned char region[64],expected[64];memset(region,0xa5,64);memcpy(expected,region,64);if(masks[j]&1)memcpy(expected+17,merge,{width});unsigned csr=(saved&~0x1f80u)|0x3f;_mm_setcsr(csr);store_{suffix}(merge,(masks[j]&1)?region+17:m+page,out,masks[j]);if(memcmp(region,expected,64)||memcmp(out,merge,16)||_mm_getcsr()!=csr)return 5;_mm_setcsr(saved);}}for(unsigned store=0;store<2;store++){{pid_t child=fork();if(child==0){{prctl(PR_SET_DUMPABLE,0);if(store)store_{suffix}(merge,m+page,out,1);else load_{suffix}(merge,m+page,out,1);_exit(0);}}int status;if(child<0||waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 6;}}memset(m+page-{width}-1,0xa5,{width}+1);store_{suffix}(merge,m+page-{width},out,1);if(memcmp(m+page-{width},merge,{width})||m[page-{width}-1]!=0xa5)return 7;}}\n"
        );
        // A live 512-bit value must survive scalar EVEX writes to aliased views.
        ir += &format!(
            "func @mixed_{suffix}(ptr,ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6=load.i64x8 align 1 v0\n v7=load.{shape} align 1 v1\n v8=load.{shape} align 1 v2\n v9=load.{shape} align 1 v3\n v10=intrinsic x86_avx512_maskmove{suffix}(v7,v5,v8,v9)\n store align 1 v10,v4\n v11=iconst.i64 64\n v12=ptradd v4,v11\n store align 1 v6,v12\n return\n}}\n"
        );
        c += &format!("extern void mixed_{suffix}(void*,void*,void*,void*,void*,uint8_t);\n");
        cases += &format!(
            "{{uint64_t wide[8],out[16];{uint} merge[{lanes}],upper[{lanes}],low[{lanes}],want[{lanes}];for(unsigned i=0;i<8;i++)wide[i]=UINT64_C(0xfedcba9876543210)+i*31;for(unsigned i=0;i<{lanes};i++){{merge[i]=({uint})({seed}+i);upper[i]=({uint})(~({uint})0-i);low[i]=i*17;}}memcpy(want,upper,16);want[0]=low[0];mixed_{suffix}(wide,merge,upper,low,out,1);if(memcmp(out,want,16)||memcmp(out+8,wide,64))return 8;}}\n"
        );
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m,page,PROT_READ|PROT_WRITE))return 9;{cases}_mm_setcsr(saved);munmap(m,page*2);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn scalar_moves_preserve_register_lanes_and_exact_memory_contracts() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(keel_codegen::x86_64::compile(&module).is_err());
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let directory = std::env::temp_dir().join(format!("keel-scalar-move-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            fs::write(directory.join("moves.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native scalar EVEX moves: AVX512F unavailable");
                    continue;
                }
                let output = Command::new("cc")
                    .args(["-O2", "-mavx512f", "-frounding-math"])
                    .arg(directory.join("oracle.c"))
                    .arg(directory.join("moves.o"))
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
                assert!(status.success(), "{allocation:?}: {status}");
            }
        }
    }
}

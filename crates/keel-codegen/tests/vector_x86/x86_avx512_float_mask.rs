//! Raw masked FP preserves merge bits, MXCSR effects and inactive-lane suppression.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn sources() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n#include <sys/wait.h>\n#include <signal.h>\n",
    );
    let mut drivers = String::new();
    for (suffix, shape, mask, cmask, ctype, bits, lanes) in [
        ("ps", "f32x16", "i16", "__mmask16", "float", "uint32_t", 16),
        ("pd", "f64x8", "i8", "__mmask8", "double", "uint64_t", 8),
    ] {
        let v = if suffix == "ps" { "__m512" } else { "__m512d" };
        let mut checks = String::new();
        for control in 0..32 {
            for sae in [4, 8] {
                let name = format!("cmp_{suffix}_{control}_{sae}");
                let intrinsic = if sae == 4 {
                    format!("cmp{suffix}")
                } else {
                    format!("cmpround{suffix}")
                };
                let extra = if sae == 4 {
                    String::new()
                } else {
                    ",v8".into()
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v0\n v5 = load.{shape} align 1 v1\n v6 = iconst.i32 {control}\n v8 = iconst.i32 {sae}\n v7 = intrinsic x86_avx512_{intrinsic}(v4,v5,v3,v6{extra})\n store align 1 v7,v2\n return\n}}\n"
                );
                c += &format!(
                    "extern void {name}(void*,void*,void*,{cmask});\n__attribute__((noinline)) static {cmask} ref_{name}(const void*a,const void*b,{cmask} k){{return _mm512_mask_cmp_round_{suffix}_mask(k,_mm512_loadu_{suffix}(a),_mm512_loadu_{suffix}(b),{control},{sae});}}\n"
                );
                checks += &format!(
                    "{{{cmask} got=0;_mm_setcsr(0x1f80);{name}(a,b,&got,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(0x1f80);{cmask} expected=ref_{name}(a,b,k);if(got!=expected||status!=(_mm_getcsr()&63)||((!k||{sae}==8)&&status))return 10;}}\n"
                );
            }
        }
        for op in ["add", "sub", "mul", "div", "sqrt"] {
            let unary = op == "sqrt";
            let rhs = if unary { "" } else { ",v7" };
            ir += &format!(
                "func @op_{op}_{suffix}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:{mask}):\n v5 = load.{shape} align 1 v0\n v6 = load.{shape} align 1 v1\n v7 = load.{shape} align 1 v2\n v8 = intrinsic x86_avx512_mask{op}{suffix}(v5,v4,v6{rhs})\n store align 1 v8,v3\n return\n}}\n"
            );
            let ref_rhs = if unary {
                String::new()
            } else {
                format!(",_mm512_loadu_{suffix}(b)")
            };
            c += &format!(
                "extern void op_{op}_{suffix}(void*,void*,void*,void*,{cmask});\n__attribute__((noinline)) static void ref_{op}_{suffix}(void*o,const void*s,const void*a,const void*b,{cmask} k){{{v} result=_mm512_mask_{op}_{suffix}(_mm512_loadu_{suffix}(s),k,_mm512_loadu_{suffix}(a){ref_rhs});_mm512_storeu_{suffix}(o,result);}}\n"
            );
            checks += &format!(
                "_mm_setcsr(0x1f80);op_{op}_{suffix}(s,a,b,out,k);unsigned status_{op}=_mm_getcsr()&63;_mm_setcsr(0x1f80);ref_{op}_{suffix}(expected,s,a,b,k);if(memcmp(out,expected,64)||status_{op}!=(_mm_getcsr()&63)||(!k&&status_{op}))return 20;\n"
            );
        }
        ir += &format!(
            "func @load_{suffix}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v1\n v5 = intrinsic x86_avx512_maskload{suffix}(v0,v4,v3)\n store align 1 v5,v2\n return\n}}\nfunc @store_{suffix}(ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:{mask}):\n v3 = load.{shape} align 1 v1\n intrinsic x86_avx512_maskstore{suffix}(v0,v3,v2)\n return\n}}\n"
        );
        c += &format!(
            "extern void load_{suffix}(void*,void*,void*,{cmask});\nextern void store_{suffix}(void*,void*,{cmask});\n"
        );
        let snan = if suffix == "ps" {
            "0x7f800001U"
        } else {
            "0x7ff0000000000001ULL"
        };
        let qnan = if suffix == "ps" {
            "0x7fc12345U"
        } else {
            "0x7ff8123456789abcULL"
        };
        drivers += &format!(
            "static int check_{suffix}(void){{{ctype} a[{lanes}],b[{lanes}],s[{lanes}],out[{lanes}],expected[{lanes}];for(unsigned i=0;i<{lanes};i++){{a[i]=i?({ctype})(i*i):-1;b[i]=i?({ctype})(i+1):0;s[i]=({ctype})(i+10);}}{bits} nan={snan};memcpy(a+1,&nan,sizeof(nan));nan={qnan};memcpy(s+2,&nan,sizeof(nan));for(unsigned pattern=0;pattern<5;pattern++){{{cmask} k=pattern==0?0:pattern==1?({cmask})~0u:pattern==2?({cmask})0x5555:pattern==3?({cmask})~3u:({cmask})(1u<<({lanes}-1));{checks}}}\nsize_t page=sysconf(_SC_PAGESIZE);char*map=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(map==MAP_FAILED)return 30;if(mprotect(map+page,page,PROT_NONE))return 31;for(unsigned n=0;n<={lanes};n++){{{ctype}*p=(void*)(map+page-n*sizeof(*p));{cmask} k=n=={lanes}?({cmask})~0u:({cmask})((1u<<n)-1);memcpy(p,a,n*sizeof(*p));load_{suffix}(p,s,out,k);for(unsigned i=0;i<{lanes};i++)memcpy(expected+i,i<n?a+i:s+i,sizeof(*p));if(memcmp(out,expected,64))return 32;store_{suffix}(p,b,k);if(memcmp(p,b,n*sizeof(*p)))return 33;}}load_{suffix}((void*)1,s,out,0);if(memcmp(out,s,64))return 34;store_{suffix}((void*)1,b,0);munmap(map,page*2);return 0;}}\n"
        );
    }
    for (suffix, bits, snan, cmask) in [
        ("ps", "uint32_t", "0x7f800001U", "__mmask16"),
        ("pd", "uint64_t", "0x7ff0000000000001ULL", "__mmask8"),
    ] {
        c += &format!(
            "static int signals_{suffix}(void){{{bits} a[16]={{0}},b[16]={{0}};a[0]={snan};for(int sae=4;sae<=8;sae+=4)for(int active=0;active<2;active++)for(int route=0;route<2;route++){{pid_t pid=fork();if(pid<0)return 41;if(!pid){{{cmask} out=0;_mm_setcsr(0x1f00);if(route){{volatile {cmask} sink=sae==4?ref_cmp_{suffix}_1_4(a,b,active):ref_cmp_{suffix}_1_8(a,b,active);(void)sink;}}else if(sae==4)cmp_{suffix}_1_4(a,b,&out,active);else cmp_{suffix}_1_8(a,b,&out,active);_exit(0);}}int status=0;if(waitpid(pid,&status,0)!=pid)return 42;if(active&&sae==4){{if(!WIFSIGNALED(status)||WTERMSIG(status)!=SIGFPE)return 43;}}else if(!WIFEXITED(status)||WEXITSTATUS(status))return 44;}}return 0;}}\n"
        );
    }
    c += &drivers;
    c += "int main(void){unsigned csr=_mm_getcsr();int r=check_ps();if(!r)r=check_pd();if(!r)r=signals_ps();if(!r)r=signals_pd();_mm_setcsr(csr);return r;}\n";
    (ir, c)
}
#[test]
fn avx512_float_mask_native_oracle_and_fault_suppression() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let directory =
        std::env::temp_dir().join(format!("keel-avx512-float-mask-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    for (tier, alloc, pools, opt) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
    ] {
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &p).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(directory.join("mask.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-mavx512f", "-frounding-math"])
                .arg(directory.join("harness.c"))
                .arg(directory.join("mask.o"))
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
            assert!(status.success(), "native masked FP oracle: {status}");
        }
    }
}
#[test]
fn avx512_float_mask_pinning_and_predicates() {
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_float_mask())
    {
        assert!(!id.info().pure);
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(data.is_pinned());
        assert!(data.may_trap());
        let name = id.info().name;
        let effect = if name.contains("maskload") {
            MemoryEffects::Read
        } else if name.contains("maskstore") {
            MemoryEffects::Write
        } else {
            MemoryEffects::Barrier
        };
        assert_eq!(data.memory_effects(), effect);
    }
    let (source, _) = sources();
    let mut discarded = keel_ir::parse(&source.replace(" store align 1 v8,v3\n", "")).unwrap();
    keel_opt::optimize(&mut discarded).unwrap();
    let retained = discarded
        .functions
        .iter()
        .flat_map(|f| f.layout_insts().map(|(_, i)| f.inst_data(i)))
        .filter(|data| matches!(data,InstData::Intrinsic{id,..} if id.is_x86_avx512_float_mask()))
        .count();
    assert_eq!(
        retained, 142,
        "unused raw FP arithmetic must retain exceptions"
    );
    for predicate in ["-1", "32"] {
        let bad = source.replace("iconst.i32 0", &format!("iconst.i32 {predicate}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    for control in ["0", "9", "v3"] {
        let bad = if control == "v3" {
            source.replace("(v4,v5,v3,v6,v8)", "(v4,v5,v3,v6,v3)")
        } else {
            source.replace("iconst.i32 8\n", &format!("iconst.i32 {control}\n"))
        };
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let m = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
}

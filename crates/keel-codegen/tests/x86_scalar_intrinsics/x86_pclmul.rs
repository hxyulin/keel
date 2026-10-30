//! Finite independent GF(2) semantics, aliases, live inputs, calls and spills.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
const CONTROLS: [u8; 8] = [0, 1, 16, 17, 238, 239, 254, 255];
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::from("decl @clobber()\n");
    for c in CONTROLS {
        for same in 0..2 {
            s += &format!(
                "func @clmul_{c}_{same}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x2 align 1 v0\n v4 = load.i64x2 align 1 v1\n v5 = iconst.i32 {c}\n v6 = intrinsic x86_pclmulqdq128(v3,{},v5)\n call @clobber()\n store align 1 v6,v2\n v7 = iconst.i64 16\n v8 = ptradd v2,v7\n store align 1 v3,v8\n v9 = ptradd v8,v7\n store align 1 v4,v9\n return\n}}\n",
                if same == 1 { "v3" } else { "v4" }
            );
        }
    }
    s
}
fn harness() -> String {
    let mut s = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nvoid clobber(void){__asm__ volatile(\"pxor %%xmm0,%%xmm0;pxor %%xmm1,%%xmm1;pxor %%xmm2,%%xmm2;pxor %%xmm3,%%xmm3;pxor %%xmm4,%%xmm4;pxor %%xmm5,%%xmm5;pxor %%xmm6,%%xmm6;pxor %%xmm7,%%xmm7;pxor %%xmm8,%%xmm8;pxor %%xmm9,%%xmm9;pxor %%xmm10,%%xmm10;pxor %%xmm11,%%xmm11;pxor %%xmm12,%%xmm12;pxor %%xmm13,%%xmm13;pxor %%xmm14,%%xmm14;pxor %%xmm15,%%xmm15\"::: \"xmm0\",\"xmm1\",\"xmm2\",\"xmm3\",\"xmm4\",\"xmm5\",\"xmm6\",\"xmm7\",\"xmm8\",\"xmm9\",\"xmm10\",\"xmm11\",\"xmm12\",\"xmm13\",\"xmm14\",\"xmm15\");}\n",
    );
    for c in CONTROLS {
        for same in 0..2 {
            s += &format!("extern void clmul_{c}_{same}(void*,void*,void*);\n");
        }
    }
    s += "struct Case {void(*fn)(void*,void*,void*); unsigned c,same;};\nstatic struct Case cases[]={\n";
    for c in CONTROLS {
        for same in 0..2 {
            s += &format!("{{clmul_{c}_{same},{c},{same}}},\n");
        }
    }
    s += "};\n";
    s += include_str!("../fixtures/pclmul_harness.c");
    s
}
#[test]
fn pclmul_controls_feature_preflight_and_foreign_target() {
    let m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(keel_ir::Intrinsic::X86Pclmulqdq128 as usize, 767);
    assert_eq!(keel_ir::Intrinsic::X86RepOuts32OwnedTerminal as usize, 766);
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        let f = Features::for_level(level);
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: f,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &p)
                .unwrap_err()
                .message
                .contains("PCLMULQDQ")
        );
        assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
        assert!(
            keel_codegen::x86_64::compile_program_chunks_with_pipeline(&m, &EMPTY, &p, &mut |_| {})
                .is_err()
        );
    }
    for control in ["v2", "v3"] {
        for val in [-1, 256] {
            let bad = format!(
                "func @bad(i64x2,i64x2,i32)->i64x2 {{\nblock0(v0:i64x2,v1:i64x2,v2:i32):\n v3 = iconst.i32 {val}\n v4 = intrinsic x86_pclmulqdq128(v0,v1,{control})\n return v4\n}}\n"
            );
            let m = keel_ir::parse(&bad).unwrap();
            assert!(keel_ir::verify_module(&m).is_err());
            let sel = FeatureSelector {
                inner: &SelectionTier::Full,
                features: Features::all_known(),
            };
            let p = CodegenPipeline {
                selector: &sel,
                allocator: &AllocationTier::Full,
            };
            assert!(keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &p).is_err());
        }
    }
    let bad = source().replace("load.i64x2", "load.i32x4");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
}
#[test]
fn pclmul_all_tiers_allocators_spills_and_object_paths() {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let d = std::env::var_os("KEEL_PCLMUL_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-pclmul-{}", std::process::id()))
        });
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("harness.c"), harness()).unwrap();
    let f = Features::for_level(CpuLevel::V1)
        .with_features("pclmulqdq")
        .unwrap();
    for tier in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (spill, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
                let selector = FeatureSelector {
                    inner: &tier,
                    features: f,
                };
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &alloc,
                };
                let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &p).unwrap();
                let typed =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&typed, f).unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
                );
                let mut chunks = keel_asm::x86::ProgramAssembler::new(f);
                keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                    &m,
                    pools,
                    &p,
                    &mut |c| chunks.push(c),
                )
                .unwrap();
                assert_eq!(object, chunks.finish().unwrap());
                for line in text
                    .lines()
                    .filter(|l| l.split_whitespace().next() == Some("pclmulqdq"))
                {
                    assert!(!line.contains('['), "no eager folding: {line}");
                }
                let name = format!("{tier:?}-{alloc:?}-{spill}");
                let o = d.join(format!("{name}.o"));
                fs::write(&o, object).unwrap();
                fs::write(d.join(format!("{name}.s")), text).unwrap();
                if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                    #[cfg(target_arch = "x86_64")]
                    if !std::is_x86_feature_detected!("pclmulqdq") {
                        eprintln!("SKIP native: PCLMULQDQ unavailable");
                        continue;
                    }
                    let exe = d.join(format!("run-{name}"));
                    let output = Command::new("cc")
                        .args(["-O1"])
                        .arg(d.join("harness.c"))
                        .arg(&o)
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(Command::new(exe).status().unwrap().success(), "{name}");
                }
            }
        }
    }
}

#[test]
fn pclmul_assembler_memory_is_m128_for_every_selector() {
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    for c in [0, 1, 16, 17, 255] {
        asm += &format!(
            ".globl memory_{c}\nmemory_{c}:\n movdqu xmm0,[rdi]\n pclmulqdq xmm0,xmmword ptr [rsi],{c}\n movdqu [rdx],xmm0\n ret\n"
        );
    }
    let object = keel_asm::x86::assemble_object_with_features(&asm, Features::all_known()).unwrap();
    let harness = r#"
#include <stdint.h>
#include <sys/mman.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#include <string.h>
extern void memory_0(void*,void*,void*),memory_1(void*,void*,void*),memory_16(void*,void*,void*),memory_17(void*,void*,void*),memory_255(void*,void*,void*);
int main(void){
 __builtin_cpu_init();if(!__builtin_cpu_supports("pclmul"))return 77;
#ifdef __linux__
 /* Ubuntu's apport handles every deliberate fault; non-dumpable processes skip it. */
 if(prctl(PR_SET_DUMPABLE,0))return 5;
#endif
 void(*ops[])(void*,void*,void*)={memory_0,memory_1,memory_16,memory_17,memory_255};
 long p=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,3*p,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m+p,p,PROT_READ|PROT_WRITE))return 1;
 uint64_t a[2]={1,1},out[2];memset(m+p,0x81,p);
 for(unsigned i=0;i<5;i++){
  ops[i](a,m+p,out);ops[i](a,m+2*p-16,out);
  /* Even low-half selection requires a full 16-byte memory operand. */
  pid_t pid=fork();if(pid<0)return 2;
  if(!pid){ops[i](a,m+2*p-8,out);_exit(3);}
  int status;if(waitpid(pid,&status,0)!=pid||!WIFSIGNALED(status)||WTERMSIG(status)!=SIGSEGV)return 4;
 }
 munmap(m,3*p);return 0;
}
"#;
    let d = std::env::var_os("KEEL_PCLMUL_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-pclmul-memory-{}", std::process::id()))
        });
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("memory.o"), object).unwrap();
    fs::write(d.join("memory.c"), harness).unwrap();
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        #[cfg(target_arch = "x86_64")]
        if !std::is_x86_feature_detected!("pclmulqdq") {
            return;
        }
        let exe = d.join("memory-test");
        assert!(
            Command::new("cc")
                .arg(d.join("memory.c"))
                .arg(d.join("memory.o"))
                .arg("-o")
                .arg(&exe)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new(exe)
                .current_dir(&d)
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn wide_pclmul_lanes_live_inputs_and_spills() {
    let f = Features::for_level(CpuLevel::V3)
        .with_features("avx512f,vpclmulqdq")
        .unwrap();
    let mut source = String::from("decl @clobber()\n");
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\nvoid clobber(void){}\nstatic void product(uint64_t a,uint64_t b,uint64_t*r){r[0]=r[1]=0;for(unsigned i=0;i<64;i++)if((b>>i)&1){r[0]^=a<<i;if(i)r[1]^=a>>(64-i);}}\n",
    );
    for (width, lanes) in [(256, 4), (512, 8)] {
        for c in CONTROLS {
            for same in 0..2 {
                source += &format!(
                    "func @wide_{width}_{c}_{same}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x{lanes} align 1 v0\n v4 = load.i64x{lanes} align 1 v1\n v5 = iconst.i32 {c}\n v6 = intrinsic x86_vpclmulqdq{width}(v3,{},v5)\n call @clobber()\n store align 1 v6,v2\n v7 = iconst.i64 {}\n v8 = ptradd v2,v7\n store align 1 v3,v8\n v9 = ptradd v8,v7\n store align 1 v4,v9\n return\n}}\n",
                    if same == 1 { "v3" } else { "v4" },
                    width / 8
                );
                harness += &format!("extern void wide_{width}_{c}_{same}(void*,void*,void*);\n");
            }
        }
    }
    harness += "int main(void){uint64_t x[8],y[8],z[24],expect[24];for(unsigned n=0;n<128;n++){for(unsigned i=0;i<8;i++){x[i]=(0x9e3779b97f4a7c15ULL*(n+1))^(1ULL<<((i+n)%64));y[i]=(0x123456789abcdefULL*(i+1))^n;}\n";
    for (width, lanes) in [(256, 4), (512, 8)] {
        for c in CONTROLS {
            for same in 0..2 {
                harness += &format!(
                    "for(unsigned lane=0;lane<{lanes}/2;lane++)product(x[2*lane+{}],{}[2*lane+{}],expect+2*lane);memcpy(expect+{lanes},x,{});memcpy(expect+{},y,{});wide_{width}_{c}_{same}(x,y,z);if(memcmp(z,expect,{}))return 1;\n",
                    c & 1,
                    if same == 1 { "x" } else { "y" },
                    (c >> 4) & 1,
                    width / 8,
                    lanes * 2,
                    width / 8,
                    width / 8 * 3
                );
            }
        }
    }
    harness += "}return 0;}\n";
    let original = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-wide-pclmul-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
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
        let mut module = original.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features: f,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
        fs::write(dir.join("wide.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f")
                || !std::is_x86_feature_detected!("vpclmulqdq")
            {
                continue;
            }
            let output = Command::new("cc")
                .arg(dir.join("harness.c"))
                .arg(dir.join("wide.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(dir.join("run")).status().unwrap().success());
        }
    }
}

#[test]
fn wide_pclmul_rejects_missing_features_and_nonconstant_controls() {
    for (width, lanes) in [(256, 4), (512, 8)] {
        let text = format!(
            "func @f(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x{lanes} align 1 v0\n v4 = load.i64x{lanes} align 1 v1\n v5 = iconst.i32 17\n v6 = intrinsic x86_vpclmulqdq{width}(v3,v4,v5)\n store align 1 v6,v2\n return\n}}\n"
        );
        let m = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for additions in ["avx512f", "vpclmulqdq"] {
            if width == 256 && additions == "vpclmulqdq" {
                continue;
            }
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features: Features::for_level(CpuLevel::V3)
                    .with_features(additions)
                    .unwrap(),
            };
            let p = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
        }
        for control in ["-1", "256"] {
            let m =
                keel_ir::parse(&text.replace("iconst.i32 17", &format!("iconst.i32 {control}")))
                    .unwrap();
            assert!(keel_ir::verify_module(&m).is_err());
        }
    }
}

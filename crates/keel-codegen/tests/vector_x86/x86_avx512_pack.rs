//! Packed prefixes have popcount(mask) elements, regardless of mask positions.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};

fn source() -> String {
    let mut source = String::new();
    for (suffix, shape, mask) in [
        ("d", "i32x16", "i16"),
        ("q", "i64x8", "i8"),
        ("ps", "f32x16", "i16"),
        ("pd", "f64x8", "i8"),
    ] {
        for operation in ["compress", "expand"] {
            for same in [false, true] {
                source += &format!(
                    "func @{operation}_{suffix}_{same}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v0\n v5 = load.{shape} align 1 v1\n v6 = intrinsic x86_avx512_{operation}{suffix}(v4,v3,{})\n store align 1 v6,v2\n v7 = iconst.i64 64\n v8 = ptradd v2,v7\n store align 1 v4,v8\n v9 = ptradd v8,v7\n store align 1 v5,v9\n v10 = ptradd v9,v7\n store align 1 v3,v10\n return\n}}\n",
                    if same { "v4" } else { "v5" }
                );
            }
        }
        source += &format!(
            "func @compressstore_{suffix}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v0\n intrinsic x86_avx512_compressstore{suffix}(v1,v4,v3)\n store align 1 v4,v2\n v5 = iconst.i64 64\n v6 = ptradd v2,v5\n store align 1 v3,v6\n return\n}}\nfunc @expandload_{suffix}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v0\n v5 = intrinsic x86_avx512_expandload{suffix}(v1,v4,v3)\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v4,v7\n v8 = ptradd v7,v6\n store align 1 v3,v8\n return\n}}\n"
        );
    }
    source
}

fn harness() -> String {
    let mut harness = String::from(
        r#"
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
"#,
    );
    let mut cases = String::new();
    for (suffix, ctype, mask, lanes) in [
        ("d", "uint32_t", "uint16_t", 16),
        ("q", "uint64_t", "uint8_t", 8),
        ("ps", "uint32_t", "uint16_t", 16),
        ("pd", "uint64_t", "uint8_t", 8),
    ] {
        let setup = format!(
            "{ctype} merge[{lanes}],input[{lanes}],out[{lanes}*4],expected[{lanes}];for(unsigned i=0;i<{lanes};i++){{merge[i]=({ctype})UINT64_C(0xfff812347fc00001)+i*0x12345;input[i]=({ctype})UINT64_C(0x7ff0012380000000)-i*0x713;}}{mask} masks[]={{0,1,1u<<({lanes}-1),({mask})0xa55a,({mask})~0u}};"
        );
        let width = 64 / lanes;
        for operation in ["compress", "expand"] {
            for same in [false, true] {
                let name = format!("{operation}_{suffix}_{same}");
                harness += &format!("extern void {name}(void*,void*,void*,{mask});\n");
                let input = if same { "merge" } else { "input" };
                let oracle = if operation == "compress" {
                    format!("expected[j++]={input}[i];")
                } else {
                    format!("expected[i]={input}[j++];")
                };
                cases += &format!(
                    "{{{setup}for(unsigned p=0;p<5;p++){{{mask} k=masks[p],saved;memcpy(expected,merge,64);unsigned j=0;for(unsigned i=0;i<{lanes};i++)if((k>>i)&1){{{oracle}}}{name}(merge,input,out,k);memcpy(&saved,(unsigned char*)out+192,sizeof saved);if(memcmp(out,expected,64)||memcmp((unsigned char*)out+64,merge,64)||memcmp((unsigned char*)out+128,input,64)||saved!=k)return 10;}}}}\n"
                );
            }
        }
        harness += &format!(
            "extern void compressstore_{suffix}(void*,void*,void*,{mask}),expandload_{suffix}(void*,void*,void*,{mask});\n"
        );
        cases += &format!(
            r#"
{{ {setup}
 for(unsigned p=0;p<5;p++){{
  {mask} k=masks[p],saved;unsigned count=0;for(unsigned i=0;i<{lanes};i++)count+=(k>>i)&1;
  unsigned char *packed=guard+2*page-count*{width};
  memset(guard+page,0xa5,page);
  compressstore_{suffix}(input,count?packed:(void*)1,out,k);
  if(memcmp(out,input,64))return 20;memcpy(&saved,(unsigned char*)out+64,sizeof saved);if(saved!=k)return 21;
  unsigned j=0;for(unsigned i=0;i<{lanes};i++)if((k>>i)&1){{{ctype} got;memcpy(&got,packed+j++*{width},{width});if(got!=input[i])return 22;}}
  if(guard[2*page-count*{width}-1]!=0xa5)return 23;
  memcpy(expected,merge,64);j=0;for(unsigned i=0;i<{lanes};i++)if((k>>i)&1)memcpy(&expected[i],packed+j++*{width},{width});
  expandload_{suffix}(merge,count?packed:(void*)1,out,k);
  memcpy(&saved,(unsigned char*)out+128,sizeof saved);
  if(memcmp(out,expected,64)||memcmp((unsigned char*)out+64,merge,64)||saved!=k)return 24;
 }}
 /* A two-element mask must fault when only one packed element is accessible. */
 for(unsigned store=0;store<2;store++){{pid_t child=fork();if(child<0)return 25;if(!child){{if(store)compressstore_{suffix}(input,guard+2*page-{width},out,3);else expandload_{suffix}(merge,guard+2*page-{width},out,3);_exit(99);}}int status;if(waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 26;}}
}}
"#
        );
    }
    harness += &format!(
        r#"
int main(void){{
#ifdef __linux__
 if(prctl(PR_SET_DUMPABLE,0))return 1;
#endif
 long page=sysconf(_SC_PAGESIZE);unsigned char *guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;
 {cases}
 return 0;
}}
"#
    );
    harness
}

#[test]
fn packed_lane_metadata_feature_permissions_and_signatures() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_pack())
        .collect();
    assert_eq!(ids.len(), 16);
    for id in ids {
        let memory = id.is_x86_avx512_compress_store() || id.is_x86_avx512_expand_load();
        let expected = if id.is_x86_avx512_compress_store() {
            MemoryEffects::Write
        } else if memory {
            MemoryEffects::Read
        } else {
            MemoryEffects::None
        };
        let instruction = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(instruction.memory_effects(), expected, "{id}");
        assert_eq!(instruction.may_trap(), memory, "{id}");
        assert_eq!(id.info().pure, !memory, "{id}");
    }
    let module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::compile_program_with_pipeline(
            &module,
            keel_ir::Target::X86_64LinuxGnu,
            &pipeline
        )
        .is_err()
    );
    assert!(keel_codegen::compile(&module, keel_ir::Target::Aarch64LinuxGnu).is_err());
    let wrong_mask = source().replace(
        "x86_avx512_compressd(v4,v3,v5)",
        "x86_avx512_compressq(v4,v3,v5)",
    );
    assert!(keel_ir::verify_module(&keel_ir::parse(&wrong_mask).unwrap()).is_err());
}

#[test]
fn packed_lane_native_oracle_and_exact_fault_suppressed_memory() {
    let mut module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[0],
        float_callee_saved: &[],
    };
    let directory = std::env::temp_dir().join(format!("keel-avx512-pack-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness()).unwrap();
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool_name, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("empty", &empty),
            ("one", &one),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            for opcode in [
                "vpcompressd",
                "vpcompressq",
                "vcompressps",
                "vcompresspd",
                "vpexpandd",
                "vpexpandq",
                "vexpandps",
                "vexpandpd",
            ] {
                assert!(text.contains(opcode), "missing {opcode}");
            }
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            let name = format!("{allocator:?}-{pool_name}");
            let path = directory.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native: AVX512F unavailable");
                    continue;
                }
                let executable = directory.join(&name);
                let output = Command::new("cc")
                    .arg(directory.join("harness.c"))
                    .arg(path)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let result = Command::new(executable).status().unwrap();
                assert!(result.success(), "{name}: {result}");
            }
        }
    }
}

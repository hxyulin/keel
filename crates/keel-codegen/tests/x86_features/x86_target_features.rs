use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{Module, Target};

fn program(
    module: &Module,
    features: Features,
) -> Result<keel_asm_ir::Program, keel_codegen::CodegenError> {
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    keel_codegen::compile_program_with_pipeline(
        module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
}

#[test]
fn local_permissions_control_lowering_and_encoding_without_granting_neighbors() {
    let source = "func @wide(ptr,ptr) target_features(\"avx\") {\nblock0(v0:ptr,v1:ptr):\n v2 = load.f32x8 align 1 v0\n v3 = intrinsic x86_avx_vsqrtps256(v2)\n store align 1 v3,v1\n return\n}\nfunc @dispatch(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n call @wide(v0,v1)\n return\n}\n";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let code = program(&module, Features::NONE).unwrap();
    assert!(keel_asm::x86::assemble_program_object_with_features(&code, Features::NONE).is_err());
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx && !encoding.avx2);
    keel_asm::x86::assemble_program_object_with_features(&code, encoding).unwrap();
    let without = keel_ir::parse(&source.replace(" target_features(\"avx\")", "")).unwrap();
    assert!(program(&without, Features::NONE).is_err());
    let disabled = keel_ir::parse(&source.replace(
        "target_features(\"avx\")",
        "target_features(\"-avx2,-avx\")",
    ))
    .unwrap();
    assert!(
        program(
            &disabled,
            Features::for_level(keel_codegen::x86_features::CpuLevel::V3)
        )
        .is_err()
    );
    for target in [Target::Aarch64LinuxGnu, Target::Aarch64AppleDarwin] {
        let error = keel_codegen::compile(&module, target).unwrap_err();
        assert!(
            error.message.contains("x86 function target features"),
            "{error:?}"
        );
    }
}

#[test]
fn direct_call_wide_transport_requires_both_ends_to_enable_avx() {
    for attributes in ["", "target_features(\"-avx\")"] {
        let module=keel_ir::parse(&format!("decl @foreign(f32x8) -> f32x8 {attributes}\nfunc @caller(f32x8) -> f32x8 target_features(\"avx\") {{\nblock0(v0:f32x8):\n v1 = call @foreign(v0)\n return v1\n}}\n")).unwrap();
        assert!(program(&module, Features::NONE).is_err());
    }
    let module=keel_ir::parse("decl @foreign(f32x8) -> f32x8 target_features(\"avx\")\nfunc @caller(f32x8) -> f32x8 target_features(\"avx\") {\nblock0(v0:f32x8):\n v1 = call @foreign(v0)\n return v1\n}\n").unwrap();
    program(&module, Features::NONE).unwrap();
}

#[test]
fn integer512_transport_preserves_features_and_exact_spill_widths() {
    use keel_codegen::regalloc::Pools;
    use std::{fs, process::Command};
    let mut source = String::from("decl @clobber()\n");
    for shape in keel_ir::VectorType::ALL_512 {
        assert_eq!(shape.bytes(), 64);
        assert_eq!(
            shape.to_string().parse::<keel_ir::VectorType>().unwrap(),
            shape
        );
        source += &format!(
            "func @copy_{shape}(ptr,ptr) target_features(\"avx512f\") {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n call @clobber()\n store align 1 v2,v1\n return\n}}\n"
        );
    }
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let disabled = keel_ir::parse(&source.replace("target_features(\"avx512f\")", "")).unwrap();
    assert!(program(&disabled, Features::NONE).is_err());
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let local = module.functions[0]
        .attrs
        .target_features
        .vector_capabilities(keel_ir::VectorCapabilities::BASELINE)
        .unwrap();
    assert_eq!(local.value_bits(), 512);
    assert_eq!(local.signed_i32_sum_bits(), 128);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let directory = std::env::temp_dir().join(format!("keel-integer512-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let harness = r#"
#include <sys/mman.h>
#include <unistd.h>
#include <string.h>
extern void copy_i8x64(void*,void*),copy_i16x32(void*,void*),copy_i32x16(void*,void*),copy_i64x8(void*,void*);
void clobber(void) { __asm__ volatile("vzeroall" ::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7","xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15"); }
int main(void) {
 long p=sysconf(_SC_PAGESIZE);
 unsigned char *a=mmap(0,p*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 unsigned char *b=mmap(0,p*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(a==MAP_FAILED||b==MAP_FAILED||mprotect(a,p,PROT_READ|PROT_WRITE)||mprotect(b,p,PROT_READ|PROT_WRITE))return 1;
 void(*ops[])(void*,void*)={copy_i8x64,copy_i16x32,copy_i32x16,copy_i64x8};
 for(unsigned i=0;i<64;i++)a[p-64+i]=(unsigned char)(i*7+1);
 for(unsigned i=0;i<4;i++) { memset(b,0xa5,p);ops[i](a+p-64,b+p-64);if(memcmp(a+p-64,b+p-64,64)||b[p-65]!=0xa5)return 2; }
 return 0;
}
"#;
    fs::write(directory.join("harness.c"), harness).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for (spill, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &empty)] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            assert!(text.contains("zmm"), "{text}");
            let typed =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&typed, encoding).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
            );
            let name = format!("{allocator:?}-{spill}");
            let path = directory.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native: AVX512F unavailable");
                    continue;
                }
                let executable = directory.join(name);
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
                assert!(Command::new(executable).status().unwrap().success());
            }
        }
    }
}

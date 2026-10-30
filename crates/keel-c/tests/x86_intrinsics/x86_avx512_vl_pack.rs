//! The C pack wrappers share exact prefix, fault-suppression and payload oracles.
#[path = "../../../keel-codegen/tests/common/avx512_vl_pack.rs"]
mod common;
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
fn source() -> String {
    let mut source = String::from("#include <immintrin.h>\n");
    for (width, suffix, _, api, vector) in common::SHAPES {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let memory = if api == "ps" || api == "pd" {
            api.to_string()
        } else {
            format!("si{width}")
        };
        for op in ["compress", "expand"] {
            for mode in 0..3 {
                let name = format!("{op}_{suffix}{width}_{mode}");
                let input = if mode == 1 { "m" } else { "a" };
                let expression = if mode == 2 {
                    format!("{prefix}_maskz_{op}_{api}(k,{input})")
                } else {
                    format!("{prefix}_mask_{op}_{api}(m,k,{input})")
                };
                let wide_load = if mode == 1 {
                    "__m512i wide=_mm512_loadu_si512(mp);"
                } else {
                    ""
                };
                let wide_store = if mode == 1 {
                    "_mm512_storeu_si512((void*)((char*)out+256),wide);"
                } else {
                    ""
                };
                source += &format!(
                    "__attribute__((target(\"avx512vl\"))) void {name}(void*mp,void*ap,void*out,__mmask8 k){{{vector} m={prefix}_loadu_{memory}(mp),a={prefix}_loadu_{memory}(ap);{wide_load}{prefix}_storeu_{memory}(out,{expression});{prefix}_storeu_{memory}((void*)((char*)out+64),m);{prefix}_storeu_{memory}((void*)((char*)out+128),a);*(unsigned char*)((char*)out+192)=k;{wide_store}}}\n"
                );
            }
        }
        source += &format!(
            "__attribute__((target(\"avx512vl\"))) void compressstore_{suffix}{width}(void*ap,void*packed,void*out,__mmask8 k){{{vector} a={prefix}_loadu_{memory}(ap);{prefix}_mask_compressstoreu_{api}(packed,k,a);{prefix}_storeu_{memory}(out,a);*(unsigned char*)((char*)out+64)=k;}}\n"
        );
        for mode in 0..2 {
            let expr = if mode == 0 {
                format!("{prefix}_mask_expandloadu_{api}(m,k,packed)")
            } else {
                format!("{prefix}_maskz_expandloadu_{api}(k,packed)")
            };
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void expandload_{suffix}{width}_{mode}(void*mp,void*packed,void*out,__mmask8 k){{{vector} m={prefix}_loadu_{memory}(mp);{prefix}_storeu_{memory}(out,{expr});{prefix}_storeu_{memory}((void*)((char*)out+64),m);*(unsigned char*)((char*)out+128)=k;}}\n"
            );
        }
    }
    source
}
#[test]
fn vl_pack_wrappers_scalar_native_guard_pages_and_partial_prefixes() {
    let source = source();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.avx2 && !features.avx512bw && !features.avx512dq);
    let missing = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&missing, &EMPTY, &pipeline).is_err()
    );
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("keel-vl-pack-c-{}-{unique}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), common::harness()).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("pack.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native C VL pack: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-fno-tree-vectorize", "-mavx512f", "-mavx512vl"])
                .arg(directory.join("oracle.c"))
                .arg(directory.join("pack.o"))
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

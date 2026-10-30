//! Public BW shuffles retain lane boundaries, masks and independent word indices.
#[path = "../../../keel-codegen/tests/common/avx512_bw_permute.rs"]
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
    for case in common::cases() {
        let name = case.name();
        let width = case.width;
        let prefix = if width == 128 {
            "_mm"
        } else if width == 256 {
            "_mm256"
        } else {
            "_mm512"
        };
        let control = if case.mode == 1 || case.operation >= 6 {
            format!("(0x100000000ULL+{})", case.control)
        } else {
            case.control.to_string()
        };
        let index = if case.mode == 1 { "a" } else { "i" };
        let merge = if case.mode == 1 { "a" } else { "m" };
        let mask_bits = case.mask.bytes().unwrap() * 8;
        let mask = format!("(__mmask{mask_bits})raw");
        let arguments = match case.operation {
            0 => format!("a,{index}"),
            1 => format!("a,{index},{control}"),
            2 | 3 | 6 | 7 => format!("a,{control}"),
            4 => format!("{index},a"),
            _ => format!("a,{index},b"),
        };
        let expression = match case.mode {
            0 if case.operation == 5 => format!(
                "{prefix}_mask_mov_epi16(m,{mask},{prefix}_mask_{}(a,{mask},i,b))",
                case.api
            ),
            1 if case.operation == 5 => format!("{prefix}_mask_{}(a,{mask},a,b)", case.api),
            0 | 1 => format!("{prefix}_mask_{}({merge},{mask},{arguments})", case.api),
            2 => format!("{prefix}_maskz_{}({mask},{arguments})", case.api),
            3 => format!("{prefix}_{}({arguments})", case.api),
            _ => format!("{prefix}_mask2_{}(a,i,{mask},b)", case.api),
        };
        let wide_load = if case.mode == 1 {
            "__m512i wide=_mm512_loadu_si512(mp);"
        } else {
            ""
        };
        let wide_store = if case.mode == 1 {
            "_mm512_storeu_si512((void*)((char*)out+384),wide);"
        } else {
            ""
        };
        let feature = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "__attribute__((target(\"{feature}\"))) void {name}(void*mp,void*ap,void*ip,void*bp,void*out,unsigned long long raw){{__m{width}i m={prefix}_loadu_si{width}(mp),a={prefix}_loadu_si{width}(ap),i={prefix}_loadu_si{width}(ip),b={prefix}_loadu_si{width}(bp);{wide_load}__m{width}i result={expression};{prefix}_storeu_si{width}(out,result);{prefix}_storeu_si{width}((void*)((char*)out+64),m);{prefix}_storeu_si{width}((void*)((char*)out+128),a);{prefix}_storeu_si{width}((void*)((char*)out+192),i);{prefix}_storeu_si{width}((void*)((char*)out+256),b);*(unsigned long long*)((char*)out+320)=raw;{wide_store}}}\n"
        );
    }
    source
}
#[test]
fn bw_permutation_wrappers_scalar_system_masks_controls_and_aliases() {
    let source = source();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512bw && features.avx512vl && !features.avx2 && !features.avx512dq);
    let missing = source
        .replace("__attribute__((target(\"avx512bw\"))) ", "")
        .replace("__attribute__((target(\"avx512bw,avx512vl\"))) ", "");
    let missing = parse(&missing, Target::X86_64LinuxGnu).unwrap();
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
        std::env::temp_dir().join(format!("keel-bw-permute-c-{}-{unique}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), common::harness()).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args([
                "-O1",
                "-fno-tree-vectorize",
                "-mavx512f",
                "-mavx512bw",
                "-mavx512vl",
                "-c",
            ])
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
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("permute.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(directory.join("oracle.o"))
                .arg(directory.join("permute.o"))
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
#[test]
fn bw_permutation_c_immediate_bounds() {
    for (api, args) in [
        ("_mm512_mask_alignr_epi8", "a,-1,a,b,"),
        ("_mm512_mask_shufflehi_epi16", "a,-1,a,"),
        ("_mm512_mask_shufflelo_epi16", "a,-1,a,"),
        ("_mm512_bslli_epi128", "a,"),
        ("_mm512_bsrli_epi128", "a,"),
    ] {
        for imm in ["0", "255", "-1", "256", "n"] {
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) __m512i f(__m512i a,__m512i b,int n){{return {api}({args}{imm});}}\n"
            );
            let valid = matches!(imm, "0" | "255");
            match parse(&source, Target::X86_64LinuxGnu) {
                Err(error) => assert!(!valid, "{api}: {error}"),
                Ok(module) => assert_eq!(
                    keel_ir::verify_module(&module).is_ok(),
                    valid,
                    "{api} {imm}"
                ),
            };
        }
    }
}

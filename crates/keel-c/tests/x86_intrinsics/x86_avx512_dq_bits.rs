//! DQ operations preserve bit payloads and wrap integer products.
use super::pclmul::parse;
use keel_codegen::x86_features::module_features;
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
const FORMS: &[(&str, &str, u16)] = &[
    ("mulloq", "i64x2", 128),
    ("andps", "f32x4", 128),
    ("andpd", "f64x2", 128),
    ("andnps", "f32x4", 128),
    ("andnpd", "f64x2", 128),
    ("orps", "f32x4", 128),
    ("orpd", "f64x2", 128),
    ("xorps", "f32x4", 128),
    ("xorpd", "f64x2", 128),
    ("mulloq", "i64x4", 256),
    ("andps", "f32x8", 256),
    ("andpd", "f64x4", 256),
    ("andnps", "f32x8", 256),
    ("andnpd", "f64x4", 256),
    ("orps", "f32x8", 256),
    ("orpd", "f64x4", 256),
    ("xorps", "f32x8", 256),
    ("xorpd", "f64x4", 256),
    ("mulloq", "i64x8", 512),
    ("andps", "f32x16", 512),
    ("andpd", "f64x8", 512),
    ("andnps", "f32x16", 512),
    ("andnpd", "f64x8", 512),
    ("orps", "f32x16", 512),
    ("orpd", "f64x8", 512),
    ("xorps", "f32x16", 512),
    ("xorpd", "f64x8", 512),
];
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for &(op, shape, width) in FORMS {
        let bytes = width / 8;
        let lanes = if shape.starts_with("f32") {
            width / 32
        } else {
            width / 64
        };
        let uint = if lanes == width / 32 {
            "uint32_t"
        } else {
            "uint64_t"
        };
        let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
        let scalar = match op {
            "mulloq" => "a[i]*b[i]",
            "andps" | "andpd" => "a[i]&b[i]",
            "andnps" | "andnpd" => "~a[i]&b[i]",
            "orps" | "orpd" => "a[i]|b[i]",
            "xorps" | "xorpd" => "a[i]^b[i]",
            _ => unreachable!(),
        };
        for mode in 0..3 {
            if mode == 2 && width != 512 && op != "mulloq" {
                continue;
            }
            let prefix = match width {
                128 => "_mm",
                256 => "_mm256",
                _ => "_mm512",
            };
            let suffix = if op == "mulloq" {
                "mullo_epi64".to_owned()
            } else if op.starts_with("andn") {
                format!("andnot_{}", &op[op.len() - 2..])
            } else {
                format!("{}_{}", &op[..op.len() - 2], &op[op.len() - 2..])
            };
            let ty = format!(
                "__m{width}{}",
                if shape.starts_with("i") {
                    "i"
                } else if shape.starts_with("f64") {
                    "d"
                } else {
                    ""
                }
            );
            let expr = match mode {
                0 => format!("{prefix}_mask_{suffix}((ns++,s),(nk++,k),(na++,a),(nb++,b))"),
                1 => format!("{prefix}_maskz_{suffix}((nk++,k),(na++,a),(nb++,b))"),
                _ => format!("{prefix}_{suffix}((na++,a),(nb++,b))"),
            };
            let expr = if width == 512 && op != "mulloq" && mode != 2 {
                let maskty = if lanes == 16 { "short" } else { "signed char" };
                let merge = if mode == 0 {
                    "(ns++,s)".to_owned()
                } else {
                    format!("({ty}){{0}}")
                };
                format!(
                    "__builtin_keel_avx512dq_{op}{width}({merge},({maskty})(nk++,k),(na++,a),(nb++,b))"
                )
            } else {
                expr
            };
            let stdexpr = match mode {
                0 => format!("{prefix}_mask_{suffix}(s,k,a,b)"),
                1 => format!("{prefix}_maskz_{suffix}(k,a,b)"),
                _ => format!("{prefix}_{suffix}(a,b)"),
            };

            let name = format!("{op}_{width}_{mode}");
            source += &format!(
                "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void {name}(const void*ap,const void*bp,const void*sp,void*out,{cmask} k){{{ty} a=*(const {ty}*)ap,b=*(const {ty}*)bp,s=*(const {ty}*)sp;int ns=0,nk=0,na=0,nb=0;{ty} result={expr};if(na!=1||nb!=1||ns!={}||nk!={})return;*({ty}*)out=result;*({ty}*)((char*)out+64)=a;*({ty}*)((char*)out+128)=b;*({ty}*)((char*)out+192)=s;*({cmask}*)((char*)out+256)=k;}}\n",
                u8::from(mode == 0),
                u8::from(mode != 2)
            );
            harness += &format!(
                "static void ref_{name}(const void*ap,const void*bp,const void*sp,void*out,{cmask} k){{{ty} a,b,s;memcpy(&a,ap,{bytes});memcpy(&b,bp,{bytes});memcpy(&s,sp,{bytes});{ty} result={stdexpr};memcpy(out,&result,{bytes});}}\n"
            );
            harness += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,{cmask});\n"
            );
            let merge = if mode == 1 { "0" } else { "s[i]" };
            let execute = if mode == 2 { "1" } else { "(k&(1u<<i))" };
            checks += &format!(
                "{{{uint} a[{lanes}],b[{lanes}],s[{lanes}];unsigned char out[288],reference[64];for(unsigned i=0;i<{lanes};i++){{a[i]=({uint})edges[i%16];b[i]=({uint})edges[(i*5+3)%16];s[i]=({uint})edges[(i+7)%16];}}{cmask} masks[]={{0,1,({cmask})~0u,0x55,({cmask})(1u<<({lanes}-1)),({cmask})~((1u<<{lanes})-1)}};for(unsigned mi=0;mi<6;mi++){{{cmask} k=masks[mi];unsigned csr=(saved|63)&~0x80u;memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(a,b,s,out,k);if(_mm_getcsr()!=csr)return 1;ref_{name}(a,b,s,reference,k);if(memcmp(out,reference,{bytes}))return 5;for(unsigned i=0;i<{lanes};i++){{{uint} expected={execute}?({uint})({scalar}):{merge},got;memcpy(&got,out+i*sizeof(got),sizeof got);if(got!=expected){{fprintf(stderr,\"{name} mask%u lane%u\\n\",mi,i);return 2;}}}}if(memcmp(out+64,a,{bytes})||memcmp(out+128,b,{bytes})||memcmp(out+192,s,{bytes})||memcmp(out+256,&k,sizeof k))return 3;for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 4;_mm_setcsr(saved);}}}}\n"
            );
        }
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t edges[16]={{0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff),UINT64_C(0xffffffff80000000),UINT64_C(0x7ff000007f800001),UINT64_C(0xfff00000ff800001),UINT64_C(0x7ff0000000000001),UINT64_C(0xfff0000000000001),UINT64_C(0x7ff812347fc12345),UINT64_C(0x8000000080000000),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x3ff000003f800000),UINT64_C(0x0000000100000001),UINT64_C(0xffffffff7fffffff)}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
#[test]
fn dq_c_bitwise_multiply_wrappers() {
    let (source, harness) = fixtures();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-dq-bits-{}", std::process::id()));
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
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features: base,
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
        fs::write(dir.join("convert.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512dq") {
                continue;
            }
            let output = Command::new("cc")
                .args([
                    "-O1",
                    "-mavx512f",
                    "-mavx512vl",
                    "-mavx512dq",
                    "-frounding-math",
                ])
                .arg(dir.join("harness.c"))
                .arg(dir.join("convert.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "conversion oracle: {status}");
        }
    }
}
#[test]
fn dq_c_wrappers_require_per_function_features() {
    let (source, _) = fixtures();
    let module = parse(
        &source.replace(
            "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) ",
            "",
        ),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx512f").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).is_err()
    );
}

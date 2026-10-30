//! DQ bridges preserve lane order, exact masks and all-one integer payloads.
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
// Force the only allocatable vector register to overlap live bridge values.
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    for width in [128, 256, 512] {
        for bits in [32, 64] {
            let lanes = width / bits;
            let bytes = width / 8;
            let letter = if bits == 32 { "d" } else { "q" };
            let name = format!("bridge_{letter}_{width}");
            let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
            let uint = if bits == 32 { "uint32_t" } else { "uint64_t" };
            let maskbytes = if lanes == 16 { 2 } else { 1 };
            let prefix = match width {
                128 => "_mm",
                256 => "_mm256",
                _ => "_mm512",
            };
            let cvec = format!("__m{width}i");
            source += &format!(
                "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void {name}(const void*ap,{cmask} k,void*out){{{cvec} a=*(const {cvec}*)ap;unsigned nx=0,nk=0;{cmask} sign={prefix}_movepi{bits}_mask((nx++,a));{cvec} expanded={prefix}_movm_epi{bits}((nk++,k));{cmask} again={prefix}_movepi{bits}_mask(expanded);{cvec} roundtrip={prefix}_movm_epi{bits}(sign);if(nx!=1||nk!=1)return;*({cvec}*)out=expanded;*({cvec}*)((char*)out+64)=roundtrip;*({cvec}*)((char*)out+128)=a;*({cmask}*)((char*)out+192)=sign;*({cmask}*)((char*)out+200)=again;*({cmask}*)((char*)out+208)=k;}}\n"
            );
            harness += &format!(
                "static void ref_{name}(const void*ap,{cmask} k,void*out){{{cvec} a;memcpy(&a,ap,{bytes});{cmask} sign={prefix}_movepi{bits}_mask(a);{cvec} expanded={prefix}_movm_epi{bits}(k);{cmask} again={prefix}_movepi{bits}_mask(expanded);{cvec} roundtrip={prefix}_movm_epi{bits}(sign);memcpy(out,&expanded,{bytes});memcpy((char*)out+64,&roundtrip,{bytes});memcpy((char*)out+128,&a,{bytes});memcpy((char*)out+192,&sign,{maskbytes});memcpy((char*)out+200,&again,{maskbytes});memcpy((char*)out+208,&k,{maskbytes});}}\n"
            );
            harness += &format!("extern void {name}(const void*,{cmask},void*);\n");
            cases += &format!(
                "{{{uint} a[{lanes}];unsigned char out[224],reference[224];{cmask} masks[]={{0,1,({cmask})~0u,0x55,({cmask})(1u<<({lanes}-1)),({cmask})~((1u<<{lanes})-1),({cmask})0x80}};for(unsigned sample=0;sample<4;sample++){{unsigned sign=0;for(unsigned i=0;i<{lanes};i++){{a[i]=({uint})edges[(i+sample*2)%8];sign|=(unsigned)(a[i]>>({bits}-1))<<i;}}for(unsigned mi=0;mi<7;mi++)for(unsigned alias=0;alias<2;alias++){{{cmask} k=masks[mi];memset(out,0xa5,sizeof out);if(alias)memcpy(out,a,{bytes});unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);{name}(alias?out:(void*)a,k,out);if(_mm_getcsr()!=csr)return 1;memset(reference,0xa5,sizeof reference);ref_{name}(a,k,reference);if(memcmp(reference,out,sizeof out))return 6;for(unsigned i=0;i<{lanes};i++){{{uint} expanded,roundtrip;memcpy(&expanded,out+i*sizeof expanded,sizeof expanded);memcpy(&roundtrip,out+64+i*sizeof roundtrip,sizeof roundtrip);if(expanded!=(({uint})0-(({uint})(k>>i)&1))||roundtrip!=(({uint})0-(({uint})(sign>>i)&1))){{fprintf(stderr,\"{name} mask%u lane%u alias%u\\n\",mi,i,alias);return 2;}}}}{cmask} got,again,live;memcpy(&got,out+192,{maskbytes});memcpy(&again,out+200,{maskbytes});memcpy(&live,out+208,{maskbytes});if(got!=sign||again!=(k&((1u<<{lanes})-1))||live!=k||memcmp(out+128,a,{bytes}))return 3;for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5||out[i+64]!=0xa5||out[i+128]!=0xa5)return 4;for(unsigned i={maskbytes};i<8;i++)if(out[192+i]!=0xa5||out[200+i]!=0xa5||out[208+i]!=0xa5)return 5;_mm_setcsr(saved);}}}}}}\n"
            );
        }
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t edges[8]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x7ff000007f800001),UINT64_C(0xfff00000ff800001),UINT64_C(0x8000000080000000)}};{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
#[test]
fn dq_mask_bridge_c_wrappers() {
    let (source, harness) = fixtures();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-dq-mask-{}", std::process::id()));
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
        (SelectionTier::Full, AllocationTier::Full, &ONE, true),
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
        for opcode in ["vpmovd2m", "vpmovq2m", "vpmovm2d", "vpmovm2q"] {
            assert!(text.contains(opcode), "missing {opcode}");
        }
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
fn dq_mask_bridge_c_feature_gates() {
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

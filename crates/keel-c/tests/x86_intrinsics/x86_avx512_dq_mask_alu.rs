//! DQ mask algebra uses scalar values and exact flag/result widths.
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
const ONE: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const FORMS: &[(&str, &str, &str)] = &[
    ("kandb", "Binary", "i8"),
    ("kandnb", "Binary", "i8"),
    ("korb", "Binary", "i8"),
    ("kxorb", "Binary", "i8"),
    ("kxnorb", "Binary", "i8"),
    ("knotb", "Unary", "i8"),
    ("kshiftlb", "Shift", "i8"),
    ("kshiftrb", "Shift", "i8"),
    ("ktestb", "Test", "i8"),
    ("kortestb", "Test", "i8"),
    ("kaddb", "Binary", "i8"),
    ("kaddw", "Binary", "i16"),
];
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for &(op, kind, ty) in FORMS {
        let word = ty == "i16";
        let ct = if word { "uint16_t" } else { "uint8_t" };
        let width = if word { 2 } else { 1 };
        let controls: &[u32] = if kind == "Shift" {
            &[0, 7, 8, 255]
        } else {
            &[0]
        };
        let aliases: &[bool] = if kind == "Binary" || kind == "Test" {
            &[false, true]
        } else {
            &[false]
        };
        for &count in controls {
            for &same in aliases {
                let name = format!("{op}_{count}_{}", u8::from(same));
                let b = if same { "a" } else { "b" };
                let api = match op {
                    "kandb" => "_kand_mask8",
                    "kandnb" => "_kandn_mask8",
                    "korb" => "_kor_mask8",
                    "kxorb" => "_kxor_mask8",
                    "kxnorb" => "_kxnor_mask8",
                    "knotb" => "_knot_mask8",
                    "kshiftlb" => "_kshiftli_mask8",
                    "kshiftrb" => "_kshiftri_mask8",
                    "ktestb" => "_ktest_mask8_u8",
                    "kortestb" => "_kortest_mask8_u8",
                    "kaddb" => "_kadd_mask8",
                    "kaddw" => "_kadd_mask16",
                    _ => unreachable!(),
                };
                let expression = if kind == "Unary" {
                    format!("{api}((na++,a))")
                } else if kind == "Shift" {
                    format!("{api}((na++,a),{count})")
                } else {
                    format!("{api}((na++,a),(nb++,{b}))")
                };
                let plain = if kind == "Unary" {
                    format!("{api}(a)")
                } else if kind == "Shift" {
                    format!("{api}(a,{count})")
                } else {
                    format!("{api}(a,{b})")
                };
                let body = if kind == "Test" {
                    let z = api.replacen("_mask8", "z_mask8", 1);
                    let cf = api.replacen("_mask8", "c_mask8", 1);
                    format!(
                        "unsigned char z={api}((na++,a),(nb++,{b}),(unsigned char*)out+1);if(((unsigned char*)out)[0]!=0xa5||((unsigned char*)out)[2]!=0xa5)return;*(unsigned char*)out=z|(((unsigned char*)out)[1]<<1);((unsigned char*)out)[2]={z}(a,{b});((unsigned char*)out)[3]={cf}(a,{b});"
                    )
                } else {
                    format!("*({ct}*)out={expression};")
                };
                let refbody = if kind == "Test" {
                    let z = api.replacen("_mask8", "z_mask8", 1);
                    let cf = api.replacen("_mask8", "c_mask8", 1);
                    format!(
                        "unsigned char z={api}(a,{b},(unsigned char*)out+1);*(unsigned char*)out=z|(((unsigned char*)out)[1]<<1);((unsigned char*)out)[2]={z}(a,{b});((unsigned char*)out)[3]={cf}(a,{b});"
                    )
                } else {
                    format!("{ct} result={plain};memcpy(out,&result,{width});")
                };
                let binary = kind == "Binary" || kind == "Test";
                source += &format!(
                    "__attribute__((target(\"avx512f,avx512dq\"))) void {name}({ct} a,{ct} b,void*out){{unsigned na=0,nb=0;{body}if(na!=1||nb!={})return;*({ct}*)((char*)out+8)=a;*({ct}*)((char*)out+16)=b;}}\n",
                    u8::from(binary)
                );
                c += &format!(
                    "static void ref_{name}({ct} a,{ct} b,void*out){{{refbody}memcpy((char*)out+8,&a,{width});memcpy((char*)out+16,&b,{width});}}\n"
                );
                c += &format!("extern void {name}({ct},{ct},void*);\n");
                let b = if same { "a" } else { "b" };
                let model = match op {
                    "kandb" => format!("a&{b}"),
                    "kandnb" => format!("~a&{b}"),
                    "korb" => format!("a|{b}"),
                    "kxorb" => format!("a^{b}"),
                    "kxnorb" => format!("~(a^{b})"),
                    "knotb" => "~a".into(),
                    "kshiftlb" => {
                        if count >= 8 {
                            "0".into()
                        } else {
                            format!("a<<{count}")
                        }
                    }
                    "kshiftrb" => {
                        if count >= 8 {
                            "0".into()
                        } else {
                            format!("a>>{count}")
                        }
                    }
                    "ktestb" => format!("((a&{b})==0)|((((~a&{b})&255)==0)<<1)"),
                    "kortestb" => format!("((a|{b})==0)|(((a|{b})==255)<<1)"),
                    "kaddb" | "kaddw" => format!("a+{b}"),
                    _ => unreachable!(),
                };
                let tail = if kind == "Test" { 4 } else { width };
                let flag_check = if kind == "Test" {
                    "if(out[1]!=(expected>>1)||out[2]!=(expected&1)||out[3]!=(expected>>1))return 6;"
                } else {
                    ""
                };
                let na = if word { 8 } else { 256 };
                let nb = if word {
                    8
                } else if kind == "Unary" || kind == "Shift" {
                    1
                } else {
                    256
                };
                let av = if word { "edges[ai]" } else { "ai" };
                let bv = if word {
                    "edges[bi]"
                } else if nb == 1 {
                    "165"
                } else {
                    "bi"
                };
                checks += &format!(
                    "for(unsigned ai=0;ai<{na};ai++)for(unsigned bi=0;bi<{nb};bi++){{unsigned a={av},b={bv};unsigned char out[32],reference[32];memset(out,0xa5,sizeof out);unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);{name}(({ct})a,({ct})b,out);if(_mm_getcsr()!=csr)return 1;memset(reference,0xa5,sizeof reference);ref_{name}(({ct})a,({ct})b,reference);if(memcmp(out,reference,sizeof out))return 5;{ct} got,live_a,live_b,expected=({ct})({model});memcpy(&got,out,{width});memcpy(&live_a,out+8,{width});memcpy(&live_b,out+16,{width});if(got!=expected||live_a!=a||live_b!=b){{fprintf(stderr,\"{name} a%x b%x got%x expected%x\\n\",a,b,(unsigned)got,(unsigned)expected);return 2;}}{flag_check}for(unsigned i={tail};i<8;i++)if(out[i]!=0xa5)return 3;for(unsigned i={width};i<8;i++)if(out[8+i]!=0xa5||out[16+i]!=0xa5)return 3;for(unsigned i=24;i<32;i++)if(out[i]!=0xa5)return 4;_mm_setcsr(saved);}}\n"
                );
            }
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();unsigned edges[8]={{0,1,255,256,32767,32768,65534,65535}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, c)
}
#[test]
fn dq_mask_alu_c_exhaustive_bytes_flags_and_one_byte_stores() {
    let (source, harness) = fixtures();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir =
        std::env::temp_dir().join(format!("keel-c-avx512-dq-mask-alu-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512dq");
    if native {
        // Compile the immutable oracle once, then relink each allocation variant.
        let output = Command::new("cc")
            .args(["-O1", "-mavx512f", "-mavx512dq", "-frounding-math", "-c"])
            .arg(dir.join("harness.c"))
            .arg("-o")
            .arg(dir.join("harness.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(12000)
                .collect::<String>()
        );
    }
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
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("convert.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
                .arg(dir.join("convert.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(12000)
                    .collect::<String>()
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "DQ special oracle: {status}");
        }
    }
}
#[test]
fn dq_mask_alu_c_feature_and_shift_controls() {
    let (source, _) = fixtures();
    let module = parse(
        &source.replace("__attribute__((target(\"avx512f,avx512dq\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE
            .with_features("avx512f,avx512vl,avx512bw")
            .unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).is_err()
    );
    for control in ["-1", "256", "count"] {
        let text = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512dq\"))) __mmask8 bad(__mmask8 a,int count){{return _kshiftli_mask8(a,{control});}}"
        );
        assert!(parse(&text, Target::X86_64LinuxGnu).is_err());
    }
    let wrapped = "#include <immintrin.h>\n__attribute__((target(\"avx512dq\"))) __mmask8 wrapped(__mmask8 a){return _kshiftli_mask8(a,0x100000000ULL);}";
    let module = parse(wrapped, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx512f").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).unwrap();
}

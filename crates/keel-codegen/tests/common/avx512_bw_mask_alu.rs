//! Shared immutable oracle preparation for scalar mask API and IR routes.
use std::sync::atomic::{AtomicU64, Ordering};
pub const FORMS: &[(&str, &str)] = &[
    ("kand", "Binary"),
    ("kandn", "Binary"),
    ("kor", "Binary"),
    ("kxor", "Binary"),
    ("kxnor", "Binary"),
    ("knot", "Unary"),
    ("kshiftl", "Shift"),
    ("kshiftr", "Shift"),
    ("kadd", "Binary"),
    ("ktest", "Test"),
    ("kortest", "Test"),
    ("kunpck", "Binary"),
];
pub fn directory(label: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "keel-bw-mask-alu-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
pub fn fixtures(c_api: bool) -> (String, String) {
    let mut source = if c_api {
        "#include <immintrin.h>\n#include <stdint.h>\n".into()
    } else {
        String::new()
    };
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for bits in [32, 64] {
        let ty = format!("i{bits}");
        let ct = format!("uint{bits}_t");
        let size = bits / 8;
        let suffix = if bits == 32 { "d" } else { "q" };
        for &(stem, kind) in FORMS {
            let op = if stem == "kunpck" {
                if bits == 32 {
                    "kunpckwd".into()
                } else {
                    "kunpckdq".into()
                }
            } else {
                format!("{stem}{suffix}")
            };
            let api = match stem {
                "kunpck" => {
                    if bits == 32 {
                        "_mm512_kunpackw".into()
                    } else {
                        "_mm512_kunpackd".into()
                    }
                }
                "kshiftl" => format!("_kshiftli_mask{bits}"),
                "kshiftr" => format!("_kshiftri_mask{bits}"),
                "ktest" | "kortest" => format!("_{stem}_mask{bits}_u8"),
                _ => format!("_{stem}_mask{bits}"),
            };
            let controls = if kind == "Shift" {
                vec![0, bits - 1, bits, bits + 1, 255]
            } else {
                vec![0]
            };
            let aliases: &[bool] = if kind == "Binary" || kind == "Test" {
                &[false, true]
            } else {
                &[false]
            };
            for count in controls {
                for &same in aliases {
                    let name = format!("{op}_{count}_{}", u8::from(same));
                    let b = if same { "a" } else { "b" };
                    let result_size = if kind == "Test" { 1 } else { size };
                    let result_ct = if kind == "Test" { "uint8_t" } else { &ct };
                    let invoke = |a: &str, b: &str| match kind {
                        "Unary" => format!("{api}({a})"),
                        "Shift" => format!("{api}({a},{count})"),
                        _ => format!("{api}({a},{b})"),
                    };
                    let refbody = if kind == "Test" {
                        format!(
                            "unsigned char cf;unsigned char z={api}(a,{b},&cf);uint8_t result=z|(cf<<1);"
                        )
                    } else {
                        format!("{ct} result={};", invoke("a", b))
                    };
                    // Execute the native instruction even when the system header
                    // optimizer chooses ordinary scalar arithmetic for its API.
                    let transfer = if bits == 32 { "kmovd" } else { "kmovq" };
                    let instruction = match kind {
                        "Unary" => format!("{op} %%k1,%%k3"),
                        "Shift" => format!("{op} ${count},%%k1,%%k3"),
                        "Test" => format!("{op} %%k2,%%k1"),
                        _ => format!("{op} %%k2,%%k1,%%k3"),
                    };
                    let native = if kind == "Test" {
                        format!(
                            "unsigned char nz,nc;__asm__ volatile(\"{transfer} %[a],%%k1\\n\\t{transfer} %[b],%%k2\\n\\t{instruction}\\n\\tsete %[z]\\n\\tsetb %[c]\" : [z]\"=qm\"(nz),[c]\"=qm\"(nc) : [a]\"r\"(a),[b]\"r\"(({ct}){b}) : \"k1\",\"k2\",\"cc\");if(result!=(nz|(nc<<1)))return;"
                        )
                    } else {
                        format!(
                            "{ct} nr;__asm__ volatile(\"{transfer} %[a],%%k1\\n\\t{transfer} %[b],%%k2\\n\\t{instruction}\\n\\t{transfer} %%k3,%[r]\" : [r]\"=r\"(nr) : [a]\"r\"(a),[b]\"r\"(({ct}){b}) : \"k1\",\"k2\",\"k3\",\"cc\");if(result!=nr)return;"
                        )
                    };
                    harness += &format!(
                        "extern void {name}({ct},{ct},void*);\nstatic void ref_{name}({ct} a,{ct} b,void*out){{{refbody}{native}memcpy(out,&result,{result_size});memcpy((char*)out+8,&a,{size});memcpy((char*)out+16,&b,{size});}}\n"
                    );
                    if c_api {
                        let n_b = u8::from(kind == "Binary" || kind == "Test");
                        let body = if kind == "Test" {
                            let zapi = api.replacen("_mask", "z_mask", 1);
                            let capi = api.replacen("_mask", "c_mask", 1);
                            format!(
                                "unsigned char cf;unsigned char z={api}((na++,a),(nb++,{b}),(nc++,&cf));uint8_t result=z|(cf<<1);if(nc!=1||z!={zapi}(a,{b})||cf!={capi}(a,{b}))return;"
                            )
                        } else {
                            format!(
                                "{ct} result={};",
                                invoke("(na++,a)", &format!("(nb++,{b})"))
                            )
                        };
                        source += &format!(
                            "__attribute__((target(\"avx512bw\"))) void {name}({ct} a,{ct} b,void*out){{unsigned na=0,nb=0,nc=0;{body}if(na!=1||nb!={n_b})return;*({result_ct}*)out=result;*({ct}*)((char*)out+8)=a;*({ct}*)((char*)out+16)=b;}}\n"
                        );
                    } else {
                        let b = if same { "v0" } else { "v1" };
                        let args = match kind {
                            "Unary" => "v0".into(),
                            "Shift" => "v0,v3".into(),
                            _ => format!("v0,{b}"),
                        };
                        source += &format!(
                            "func @{name}({ty},{ty},ptr) target_features(\"avx512bw\") {{\nblock0(v0:{ty},v1:{ty},v2:ptr):\n v3=iconst.i32 {count}\n v4=intrinsic x86_avx512bw_{op}({args})\n store align 1 v4,v2\n v5=iconst.i64 8\n v6=ptradd v2,v5\n store align 1 v0,v6\n v7=ptradd v6,v5\n store align 1 v1,v7\n return\n}}\n"
                        );
                    }
                    let all = if bits == 32 {
                        "UINT32_MAX"
                    } else {
                        "UINT64_MAX"
                    };
                    let model = match stem {
                        "kand" => format!("a&{b}"),
                        "kandn" => format!("~a&{b}"),
                        "kor" => format!("a|{b}"),
                        "kxor" => format!("a^{b}"),
                        "kxnor" => format!("~(a^{b})"),
                        "knot" => "~a".into(),
                        "kadd" => format!("a+{b}"),
                        "kshiftl" | "kshiftr" => {
                            if count >= bits {
                                "0".into()
                            } else {
                                format!("a{}{}", if stem == "kshiftl" { "<<" } else { ">>" }, count)
                            }
                        }
                        "ktest" => format!("((a&{b})==0)|((((~a)&{b})==0)<<1)"),
                        "kortest" => format!("((a|{b})==0)|(((a|{b})=={all})<<1)"),
                        "kunpck" => {
                            if bits == 32 {
                                format!("((a&65535u)<<16)|({b}&65535u)")
                            } else {
                                format!("((a&UINT32_MAX)<<32)|({b}&UINT32_MAX)")
                            }
                        }
                        _ => unreachable!(),
                    };
                    checks += &format!(
                        "for(unsigned ai=0;ai<12;ai++)for(unsigned bi=0;bi<12;bi++){{{ct} a=({ct})edges[ai],b=({ct})edges[bi];unsigned char out[32],reference[32];memset(out,0xa5,32);memset(reference,0xa5,32);unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);{name}(a,b,out);if(_mm_getcsr()!=csr)return 1;ref_{name}(a,b,reference);if(memcmp(out,reference,32)){{fprintf(stderr,\"{name} vendor mismatch a%llx b%llx\\n\",(unsigned long long)a,(unsigned long long)b);return 2;}}{result_ct} got,expected=({result_ct})({model});memcpy(&got,out,{result_size});if(got!=expected){{fprintf(stderr,\"{name} scalar mismatch\\n\");return 3;}}}}\n"
                    );
                }
            }
        }
    }
    if c_api {
        source += r#"
unsigned takes;
static __mmask32 *take32(__mmask32*p){takes++;return p;}
static __mmask64 *take64(__mmask64*p){takes++;return p;}
void convenience32(unsigned int a,void*out){takes=0;_store_mask32(take32((__mmask32*)((char*)out+4)),_cvtu32_mask32((takes++,a)));unsigned int b=_cvtmask32_u32(_load_mask32(take32((__mmask32*)((char*)out+4))));*(unsigned int*)out=b;*(unsigned int*)((char*)out+16)=takes;}
void convenience64(unsigned long long a,void*out){takes=0;_store_mask64(take64((__mmask64*)((char*)out+8)),_cvtu64_mask64((takes++,a)));unsigned long long b=_cvtmask64_u64(_load_mask64(take64((__mmask64*)((char*)out+8))));*(unsigned long long*)out=b;*(unsigned int*)((char*)out+16)=takes;}
"#;
        harness += "extern void convenience32(unsigned int,void*);extern void convenience64(unsigned long long,void*);\n";
        checks += "for(unsigned i=0;i<12;i++){unsigned char out[32],expected[32];uint32_t a=(uint32_t)edges[i],count=3;uint64_t b=edges[i];memset(out,0xa5,32);memset(expected,0xa5,32);convenience32(a,out);memcpy(expected,&a,4);memcpy(expected+4,&a,4);memcpy(expected+16,&count,4);if(memcmp(out,expected,32))return 8;memset(out,0xa5,32);memset(expected,0xa5,32);convenience64(b,out);memcpy(expected,&b,8);memcpy(expected+8,&b,8);memcpy(expected+16,&count,4);if(memcmp(out,expected,32))return 9;}";
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t edges[12]={{0,1,0x7fffffffULL,0x80000000ULL,0xffffffffULL,0x100000000ULL,0x8000000000000000ULL,UINT64_MAX,0xaaaaaaaaaaaaaaaaULL,0x5555555555555555ULL,0x0123456789abcdefULL,0xfedcba9876543210ULL}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
pub fn run(module: &keel_ir::Module, harness: &str, label: &str) {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::{AllocationTier, Pools},
        x86_features::{FeatureSelector, Features, module_features},
    };
    use std::{fs, process::Command};
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        caller_saved: &[7],
        ..empty
    };
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(module, base).unwrap();
    assert!(features.avx512bw && !features.avx512vl && !features.avx512dq && !features.avx2);
    let dir = directory(label);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512bw");
    if native {
        let result = Command::new("cc")
            .args(["-O1", "-mavx512bw", "-c"])
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &empty, &one] {
            for optimize in [false, true] {
                let mut m = module.clone();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let tier = if optimize {
                    SelectionTier::Full
                } else {
                    SelectionTier::Baseline
                };
                let selector = FeatureSelector {
                    inner: &tier,
                    features: base,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                assert!(text.contains("kmovd") && text.contains("kmovq"));
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                );
                fs::write(dir.join("mask.o"), object).unwrap();
                if native {
                    let result = Command::new("cc")
                        .arg(dir.join("runner.o"))
                        .arg(dir.join("mask.o"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let result = Command::new(dir.join("run")).output().unwrap();
                    assert!(
                        result.status.success(),
                        "{label} {allocator:?} opt{optimize}: {} {}",
                        result.status,
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
            }
        }
    }
}

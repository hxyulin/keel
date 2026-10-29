//! Strict-v1 SSE/SSE2 architectural conversions retain MXCSR rounding/flags and indefinite
//! invalid results. They do not replace the portable saturating conversion IR.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{Intrinsic, Type};
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn ids() -> impl Iterator<Item = Intrinsic> {
    Intrinsic::ALL
        .into_iter()
        .filter(|i| i.is_x86_legacy_raw_fp_convert())
}
fn source() -> String {
    let mut s = String::new();
    for id in ids() {
        let info = id.info();
        let from = info.params[0];
        let to = info.returns[0];
        let bytes = to.bytes().unwrap();
        s += &format!(
            "func @raw_{id}(ptr,ptr){{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{from} align 1 v0\n v3 = intrinsic {id}(v2)\n store align 1 v3,v1\n v4 = iconst.i64 {bytes}\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\nfunc @unused_{id}(ptr){{\nblock0(v0:ptr):\n v1 = load.{from} align 1 v0\n v2 = intrinsic {id}(v1)\n return\n}}\n"
        );
    }
    s
}
fn expr(id: Intrinsic) -> String {
    let i = id.info();
    let a = i.params[0];
    let b = i.returns[0];
    let op = id.x86_legacy_raw_fp_opcode().unwrap();
    let load = if let Some(s) = a.vector() {
        let wide = s.bytes() == 32;
        let prefix = if wide { "_mm256" } else { "_mm" };
        match s.lane_type() {
            Type::F32 => format!("{prefix}_loadu_ps((const float*)input)"),
            Type::F64 => format!("{prefix}_loadu_pd((const double*)input)"),
            _ => format!(
                "{prefix}_loadu_si{}((const __m{}i*)input)",
                s.bytes() * 8,
                s.bytes() * 8
            ),
        }
    } else if a == Type::F32 {
        "_mm_load_ss((const float*)input)".into()
    } else if a == Type::F64 {
        "_mm_load_sd((const double*)input)".into()
    } else {
        "input[0]".to_string()
    };
    if a.is_vector() {
        let widest = a.bytes().unwrap().max(b.bytes().unwrap());
        let prefix = if widest == 32 { "_mm256" } else { "_mm" };
        let suffix = match op {
            "cvtdq2pd" => "cvtepi32_pd",
            "cvtdq2ps" => "cvtepi32_ps",
            "cvtpd2dq" => "cvtpd_epi32",
            "cvtpd2ps" => "cvtpd_ps",
            "cvtps2dq" => "cvtps_epi32",
            "cvtps2pd" => "cvtps_pd",
            "cvttpd2dq" => "cvttpd_epi32",
            "cvttps2dq" => "cvttps_epi32",
            _ => unreachable!(),
        };
        return format!("{prefix}_{suffix}({load})");
    }
    if a.is_int() {
        let bits = a.bytes().unwrap() * 8;
        let suf = if b == Type::F32 { "ss" } else { "sd" };
        let zero = if b == Type::F32 {
            "_mm_setzero_ps()"
        } else {
            "_mm_setzero_pd()"
        };
        return format!("_mm_cvtss_f32(_mm_cvtsi{bits}_{suf}({zero},{load}))").replace(
            "_mm_cvtss_f32",
            if b == Type::F32 {
                "_mm_cvtss_f32"
            } else {
                "_mm_cvtsd_f64"
            },
        );
    }
    if b.is_int() {
        let bits = b.bytes().unwrap() * 8;
        let scalar = if a == Type::F32 { "ss" } else { "sd" };
        let trunc = if op.starts_with("cvtt") { "t" } else { "" };
        return format!("_mm_cvt{trunc}{scalar}_si{bits}({load})");
    }
    if a == Type::F64 {
        format!("_mm_cvtss_f32(_mm_cvtsd_ss(_mm_setzero_ps(),{load}))")
    } else {
        format!("_mm_cvtsd_f64(_mm_cvtss_sd(_mm_setzero_pd(),{load}))")
    }
}
fn harness() -> String {
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,void*);void(*unused)(const void*);void(*ref)(const void*,void*);unsigned in,out,bits;const char*name;};static const struct Case cases[]={\n",
    );
    for id in ids() {
        let info = id.info();
        let from = info.params[0];
        let to = info.returns[0];
        let cty = if let Some(shape) = to.vector() {
            format!(
                "__m{}{}",
                shape.bytes() * 8,
                if shape.lane_type() == Type::F32 {
                    ""
                } else if shape.lane_type() == Type::F64 {
                    "d"
                } else {
                    "i"
                }
            )
        } else if to == Type::F32 {
            "float".into()
        } else if to == Type::F64 {
            "double".into()
        } else {
            format!("int{}_t", to.bytes().unwrap() * 8)
        };
        let lane = from.vector().map_or(from, |v| v.lane_type());
        let input_cty = if lane == Type::F32 {
            "float".to_string()
        } else if lane == Type::F64 {
            "double".to_string()
        } else {
            format!("int{}_t", lane.bytes().unwrap() * 8)
        };
        let count = from.bytes().unwrap() / lane.bytes().unwrap();
        let from_bytes = from.bytes().unwrap();
        c += &format!(
            "extern void raw_{id}(const void*,void*);extern void unused_{id}(const void*);__attribute__((noinline)) static void ref_{id}(const void*p,void*out){{{input_cty} input[{count}];memcpy(input,p,{from_bytes});{cty} e={};memcpy(out,&e,{});}}\n",
            expr(id),
            to.bytes().unwrap()
        );
        let bits = from
            .vector()
            .map_or(from, |s| s.lane_type())
            .bytes()
            .unwrap()
            * 8;
        table += &format!(
            "{{raw_{id},unused_{id},ref_{id},{},{},{bits},\"{id}\"}},\n",
            from.bytes().unwrap(),
            to.bytes().unwrap()
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
int main(void){unsigned saved=_mm_getcsr();const uint64_t e32[]={0,0x80000000,0x7f800000,0xff800000,0x7fc00001,0x7f800001,0x3fc00000,0xbfc00000,0x4f000000,0xcf000000,0x5f000000,0xdf000000,1,0x7f7fffff,0x7fffffff,0xffffffff};const uint64_t e64[]={0,0x8000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8000000000001ULL,0x7ff0000000000001ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x41e0000000000000ULL,0xc1e0000000000000ULL,0x43e0000000000000ULL,0xc3e0000000000000ULL,1,0x7fefffffffffffffULL,0x7fffffffffffffffULL,~0ULL};uint64_t rng=12345;
for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned mode=0;mode<4;mode++)for(unsigned pattern=0;pattern<1024;pattern++)for(unsigned offset=0;offset<16;offset++)for(unsigned alias=0;alias<2;alias++){
 unsigned char in[128],out[128],e[64];memset(in,0x5a,128);memset(out,0x5a,128);unsigned char*p=in+16+offset,*q=(alias?in:out)+16+offset;for(unsigned i=0;i<c->in*8/c->bits;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t bits=pattern<16?(c->bits==32?e32[(pattern+i)%16]:e64[(pattern+i)%16]):rng;memcpy(p+i*c->bits/8,&bits,c->bits/8);}memcpy(e+c->out,p,c->in);
 unsigned csr=((saved|0x1f80)&~(0x8040|0x603f))|(mode<<13);_mm_setcsr(csr);c->ref(p,e);unsigned ef=_mm_getcsr()&63;unsigned uf=ef;if(pattern<16){_mm_setcsr(csr);c->unused(p);uf=_mm_getcsr()&63;}_mm_setcsr(csr);c->run(p,q);unsigned rf=_mm_getcsr()&63;if(memcmp(q,e,c->in+c->out)||ef!=rf||ef!=uf||q[-1]!=0x5a||q[c->in+c->out]!=0x5a){printf("failed %s mode%u pattern%u offset%u alias%u flags%u/%u\n",c->name,mode,pattern,offset,alias,ef,rf);return 10;}
}}
_mm_setcsr(saved);return 0;}
"#;
    c
}
#[path = "common/matrix.rs"]
mod matrix;
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    static PREPARED: std::sync::OnceLock<matrix::PreparedModule> = std::sync::OnceLock::new();
    let m = PREPARED
        .get_or_init(|| matrix::PreparedModule::parse(&source()))
        .for_selector(s);
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if empty {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((b, text))
}
#[test]
fn raw_fp_conversion_control_and_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("legacy-raw-fp.ir"), source()).unwrap();
        fs::write(d.join("legacy-raw-fp.c"), harness()).unwrap();
    }
    let d = common::temp("legacy-raw-fp-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("raw.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-march=x86-64",
                "-mno-avx",
                "-O2",
                "-c",
            ])
            .arg(d.join("raw.c"))
            .arg("-o")
            .arg(d.join("oracle.o")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                if let Some(d) = &export {
                    let name = format!("legacy-raw-fp-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
    object(
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::NONE,
    )
    .unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_strict_v1_raw_fp_rounding_indefinite_and_exception_flags() {
    let d = common::temp("legacy-raw-fp-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("raw.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                fs::write(d.join("raw.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args([
                            "-O2",
                            "-march=x86-64",
                            "-mno-avx",
                            "-fno-fast-math",
                            "-ffp-contract=off",
                        ])
                        .arg(d.join("raw.c"))
                        .arg(d.join("raw.o"))
                        .arg("-o")
                        .arg(d.join("raw")),
                );
                common::command(&mut Command::new(d.join("raw")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn raw_fp_signatures_foreign_targets_and_pinned_unused_results() {
    let bad = source().replace("x86_sse_cvtdq2pd128(v2)", "x86_sse_cvtpd2dq128(v2)");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let narrow = source()
        .split("func @raw_x86_sse_cvtdq2pd256")
        .next()
        .unwrap()
        .to_string();
    let m = keel_ir::parse(&narrow).unwrap();
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    for f in m.functions.iter().filter(|f| f.name.starts_with("unused_")) {
        assert!(f.layout_insts().any(|(_,i)|matches!(f.inst_data(i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_legacy_raw_fp_convert())));
    }
}

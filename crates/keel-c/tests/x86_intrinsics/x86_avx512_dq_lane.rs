//! DQ lane values preserve every bit across widths, masks and allocation paths.
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
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
// Opcode stem, kind, short input, result, mask, encoded width, maximum index.
const FORMS: &[(&str, &str, &str, &str, &str, u16, u16)] = &[
    ("broadcasti32x2", "Register", "i32x4", "i32x4", "i8", 128, 0),
    ("broadcastf32x2", "Register", "f32x4", "f32x8", "i8", 256, 0),
    ("broadcastf64x2", "Memory", "f64x2", "f64x4", "i8", 256, 0),
    ("broadcasti32x2", "Register", "i32x4", "i32x8", "i8", 256, 0),
    ("broadcasti64x2", "Memory", "i64x2", "i64x4", "i8", 256, 0),
    ("extractf64x2", "Extract", "f64x4", "f64x2", "i8", 256, 1),
    ("extracti64x2", "Extract", "i64x4", "i64x2", "i8", 256, 1),
    ("insertf64x2", "Insert", "f64x2", "f64x4", "i8", 256, 1),
    ("inserti64x2", "Insert", "i64x2", "i64x4", "i8", 256, 1),
    (
        "broadcastf32x2",
        "Register",
        "f32x4",
        "f32x16",
        "i16",
        512,
        0,
    ),
    ("broadcastf32x8", "Memory", "f32x8", "f32x16", "i16", 512, 0),
    ("broadcastf64x2", "Memory", "f64x2", "f64x8", "i8", 512, 0),
    (
        "broadcasti32x2",
        "Register",
        "i32x4",
        "i32x16",
        "i16",
        512,
        0,
    ),
    ("broadcasti32x8", "Memory", "i32x8", "i32x16", "i16", 512, 0),
    ("broadcasti64x2", "Memory", "i64x2", "i64x8", "i8", 512, 0),
    ("extractf32x8", "Extract", "f32x16", "f32x8", "i8", 512, 1),
    ("extractf64x2", "Extract", "f64x8", "f64x2", "i8", 512, 3),
    ("extracti32x8", "Extract", "i32x16", "i32x8", "i8", 512, 1),
    ("extracti64x2", "Extract", "i64x8", "i64x2", "i8", 512, 3),
    ("insertf32x8", "Insert", "f32x8", "f32x16", "i16", 512, 1),
    ("insertf64x2", "Insert", "f64x2", "f64x8", "i8", 512, 3),
    ("inserti32x8", "Insert", "i32x8", "i32x16", "i16", 512, 1),
    ("inserti64x2", "Insert", "i64x2", "i64x8", "i8", 512, 3),
];
fn bytes(shape: &str) -> usize {
    let (bits, lanes) = shape[1..].split_once('x').unwrap();
    bits.parse::<usize>().unwrap() * lanes.parse::<usize>().unwrap() / 8
}
fn ctype(shape: &str) -> String {
    format!(
        "__m{}{}",
        bytes(shape) * 8,
        if shape.starts_with("i") {
            "i"
        } else if shape.starts_with("f64") {
            "d"
        } else {
            ""
        }
    )
}
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for &(op, kind, input, output, mask, width, max) in FORMS {
        let insert = kind == "Insert";
        let extract = kind == "Extract";
        let whole = if insert { output } else { input };
        let rbytes = bytes(output);
        let abytes = bytes(whole);
        let bbytes = if insert { bytes(input) } else { 0 };
        let bits = if output.starts_with("f32") || output.starts_with("i32") {
            32
        } else {
            64
        };
        let lanes = rbytes * 8 / bits;
        let alanes = abytes * 8 / bits;
        let blanes = if insert { bbytes * 8 / bits } else { 1 };
        let uint = if bits == 32 { "uint32_t" } else { "uint64_t" };
        let cmask = if mask == "i16" { "uint16_t" } else { "uint8_t" };
        let mbytes = if mask == "i16" { 2 } else { 1 };
        let block = if insert {
            blanes
        } else if extract {
            lanes
        } else {
            op.rsplit_once('x').unwrap().1.parse::<usize>().unwrap()
        };
        for control in 0..=max {
            for mode in 0..3 {
                let name = format!("{op}_{width}_{control}_{mode}");
                let prefix = match width {
                    128 => "_mm",
                    256 => "_mm256",
                    _ => "_mm512",
                };
                let api = if op.starts_with("broadcast") {
                    op.replacen("broadcast", "broadcast_", 1)
                } else if extract {
                    format!(
                        "{op}{}",
                        if input.starts_with("f32") {
                            "_ps"
                        } else if input.starts_with("f64") {
                            "_pd"
                        } else if input.starts_with("i32") {
                            "_epi32"
                        } else {
                            "_epi64"
                        }
                    )
                } else {
                    op.to_owned()
                };
                let cr = ctype(output);
                let ca = ctype(whole);
                let cb = ctype(input);
                let imm = if insert || extract {
                    format!(",{control}")
                } else {
                    String::new()
                };
                let part = if insert { ",(nb++,b)" } else { "" };
                let plainpart = if insert { ",b" } else { "" };
                let expression = match mode {
                    0 => format!("{prefix}_mask_{api}((ns++,merge),(nk++,k),(na++,a){part}{imm})"),
                    1 => format!("{prefix}_maskz_{api}((nk++,k),(na++,a){part}{imm})"),
                    _ => format!("{prefix}_{api}((na++,a){part}{imm})"),
                };
                let reference = match mode {
                    0 => format!("{prefix}_mask_{api}(merge,k,a{plainpart}{imm})"),
                    1 => format!("{prefix}_maskz_{api}(k,a{plainpart}{imm})"),
                    _ => format!("{prefix}_{api}(a{plainpart}{imm})"),
                };
                let loadb = if insert {
                    format!("{cb} b=*(const {cb}*)bp;")
                } else {
                    String::new()
                };
                let storeb = if insert {
                    format!("*({cb}*)((char*)out+192)=b;")
                } else {
                    String::new()
                };
                let refloadb = if insert {
                    format!("{cb} b;memcpy(&b,bp,{bbytes});")
                } else {
                    String::new()
                };
                let refstoreb = if insert {
                    format!("memcpy((char*)out+192,&b,{bbytes});")
                } else {
                    String::new()
                };
                let flags = if width < 512 {
                    "avx512f,avx512dq,avx512vl"
                } else {
                    "avx512f,avx512dq"
                };
                source += &format!(
                    "__attribute__((target(\"{flags}\"))) void {name}(const void*sp,const void*ap,const void*bp,void*out,{cmask} k){{{cr} merge=*(const {cr}*)sp;{ca} a=*(const {ca}*)ap;{loadb}unsigned ns=0,nk=0,na=0,nb=0;{cr} result={expression};if(ns!={}||nk!={}||na!=1||nb!={})return;*({cr}*)out=result;*({cr}*)((char*)out+64)=merge;*({ca}*)((char*)out+128)=a;{storeb}*({cmask}*)((char*)out+256)=k;}}\n",
                    u8::from(mode == 0),
                    u8::from(mode != 2),
                    u8::from(insert)
                );
                harness += &format!(
                    "static void ref_{name}(const void*sp,const void*ap,const void*bp,void*out,{cmask} k){{{cr} merge;{ca} a;memcpy(&merge,sp,{rbytes});memcpy(&a,ap,{abytes});{refloadb}{cr} result={reference};memcpy(out,&result,{rbytes});memcpy((char*)out+64,&merge,{rbytes});memcpy((char*)out+128,&a,{abytes});{refstoreb}memcpy((char*)out+256,&k,{mbytes});}}\n"
                );
                harness += &format!(
                    "extern void {name}(const void*,const void*,const void*,void*,{cmask});\n"
                );
                let execute = if mode == 2 { "1" } else { "(k&(1u<<i))" };
                let inactive = if mode == 1 { "0" } else { "s[i]" };
                let select = if insert {
                    format!("(i/{block}=={control}?b[i%{block}]:a[i])")
                } else if extract {
                    format!("a[{control}*{block}+i]")
                } else {
                    format!("a[i%{block}]")
                };
                let partfill = if insert {
                    format!(
                        "for(unsigned i=0;i<{blanes};i++)b[i]=({uint})edges[(i*3+5+sample)%8];memcpy(gb,b,{bbytes});"
                    )
                } else {
                    String::new()
                };
                let partcheck = if insert {
                    format!("if(memcmp(out+192,b,{bbytes}))return 4;")
                } else {
                    String::new()
                };
                let bpointer = if insert { "alias==3?out:gb" } else { "0" };
                cases += &format!(
                    "{{{uint} a[{alanes}],b[{blanes}],s[{lanes}],expected[{lanes}];unsigned char out[288],refout[288];unsigned char*gs=g[0]+page-{rbytes},*ga=g[1]+page-{abytes},*gb=g[2]+page-{bbytes};{cmask} masks[]={{0,({cmask})~0u,1,({cmask})0xaaaa,({cmask})(1u<<({lanes}-1)),({cmask})~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<2;sample++){{for(unsigned i=0;i<{lanes};i++)s[i]=({uint})edges[(i*5+2+sample)%8];for(unsigned i=0;i<{alanes};i++)a[i]=({uint})edges[(i+sample)%8]^(sample?0:({uint})(i*17));{partfill}memcpy(gs,s,{rbytes});memcpy(ga,a,{abytes});for(unsigned mi=0;mi<6;mi++)for(unsigned alias=0;alias<{};alias++){{{cmask} k=masks[mi];for(unsigned i=0;i<{lanes};i++)expected[i]={execute}?{select}:{inactive};memset(out,0xa5,sizeof out);if(alias==1)memcpy(out,a,{abytes});if(alias==2)memcpy(out,s,{rbytes});if(alias==3)memcpy(out,b,{bbytes});unsigned csr=((saved|63)&~0x80u)^(sample?0x8040:0);_mm_setcsr(csr);{name}(alias==2?out:gs,alias==1?out:ga,{bpointer},out,k);if(_mm_getcsr()!=csr)return 1;memset(refout,0xa5,sizeof refout);if(alias==1)memcpy(refout,a,{abytes});if(alias==2)memcpy(refout,s,{rbytes});if(alias==3)memcpy(refout,b,{bbytes});ref_{name}(s,a,b,refout,k);if(memcmp(out,refout,sizeof out))return 9;if(memcmp(out,expected,{rbytes})){{fprintf(stderr,\"{name} mask%u alias%u\\n\",mi,alias);return 2;}}if(memcmp(out+64,s,{rbytes})||memcmp(out+128,a,{abytes})||memcmp(out+256,&k,{mbytes}))return 3;{partcheck}for(unsigned i={rbytes};i<64;i++)if(out[i]!=(alias==1&&i<{abytes}?((unsigned char*)a)[i]:0xa5)||out[i+64]!=0xa5)return 5;for(unsigned i={abytes};i<64;i++)if(out[i+128]!=0xa5)return 6;for(unsigned i={bbytes};i<64;i++)if(out[i+192]!=0xa5)return 7;for(unsigned i={mbytes};i<32;i++)if(out[i+256]!=0xa5)return 8;_mm_setcsr(saved);}}}}}}\n",
                    if insert { 4 } else { 3 }
                );
            }
        }
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*g[3];for(unsigned i=0;i<3;i++){{g[i]=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(g[i]==MAP_FAILED)return 90;if(mprotect(g[i]+page,page,PROT_NONE))return 91;}}uint64_t edges[8]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x7ff000007f800001),UINT64_C(0xfff00000ff800001),UINT64_C(0x8000000080000000)}};{cases}for(unsigned i=0;i<3;i++)munmap(g[i],page*2);_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
#[test]
fn dq_lane_c_values_masks_guards_aliases_and_spills() {
    let (source, harness) = fixtures();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-dq-lane-{}", std::process::id()));
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
fn dq_lane_c_features_and_controls() {
    let (source, _) = fixtures();
    for attrs in [
        "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) ",
        "__attribute__((target(\"avx512f,avx512dq\"))) ",
    ] {
        let missing = source.replace(attrs, "");
        let module = parse(&missing, Target::X86_64LinuxGnu).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                .is_err()
        );
    }
    for call in [
        "_mm256_extractf64x2_pd(a,2)",
        "_mm256_extractf64x2_pd(a,-1)",
        "_mm256_extractf64x2_pd(a,i)",
    ] {
        let text = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) __m128d bad(__m256d a,int i){{return {call};}}"
        );
        assert!(parse(&text, Target::X86_64LinuxGnu).is_err());
    }
}

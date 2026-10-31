//! VL lane wrappers preserve payloads, inactive merges and caller permissions.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const XMM12: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
#[derive(Clone, Copy)]
enum Kind {
    Scalar,
    Set1,
    Broadcast,
    Extract,
    Insert,
}
// Public API stem, kind, output width, element bits, floating payload.
const FORMS: &[(&str, Kind, usize, usize, bool)] = &[
    ("broadcastd_epi32", Kind::Scalar, 128, 32, false),
    ("broadcastq_epi64", Kind::Scalar, 128, 64, false),
    ("broadcastss_ps", Kind::Scalar, 128, 32, true),
    ("broadcastd_epi32", Kind::Scalar, 256, 32, false),
    ("broadcastq_epi64", Kind::Scalar, 256, 64, false),
    ("broadcastss_ps", Kind::Scalar, 256, 32, true),
    ("broadcastsd_pd", Kind::Scalar, 256, 64, true),
    ("set1_epi32", Kind::Set1, 128, 32, false),
    ("set1_epi64", Kind::Set1, 128, 64, false),
    ("set1_epi32", Kind::Set1, 256, 32, false),
    ("set1_epi64", Kind::Set1, 256, 64, false),
    ("broadcast_i32x4", Kind::Broadcast, 256, 32, false),
    ("broadcast_f32x4", Kind::Broadcast, 256, 32, true),
    ("extracti32x4_epi32", Kind::Extract, 128, 32, false),
    ("extractf32x4_ps", Kind::Extract, 128, 32, true),
    ("inserti32x4", Kind::Insert, 256, 32, false),
    ("insertf32x4", Kind::Insert, 256, 32, true),
];
fn ctype(width: usize, bits: usize, float: bool) -> String {
    format!(
        "__m{width}{}",
        if !float {
            "i"
        } else if bits == 64 {
            "d"
        } else {
            ""
        }
    )
}
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut cases = String::new();
    let mut n = 0;
    for &(op, kind, width, bits, float) in FORMS {
        let inputwidth = if matches!(kind, Kind::Extract | Kind::Insert) {
            256
        } else {
            128
        };
        let pre = if matches!(kind, Kind::Extract) || width == 256 {
            "_mm256"
        } else {
            "_mm"
        };
        let out = ctype(width, bits, float);
        let input = ctype(inputwidth, bits, float);
        let block = ctype(128, bits, float);
        let bytes = width / 8;
        let ibytes = inputwidth / 8;
        let lane_bytes = bits / 8;
        let lanes = width / bits;
        for index in 0..if matches!(kind, Kind::Extract | Kind::Insert) {
            2
        } else {
            1
        } {
            for mode in if matches!(kind, Kind::Scalar | Kind::Set1) {
                1..3
            } else {
                0..3
            } {
                for alias in 0..if matches!(kind, Kind::Insert)
                    || (width == inputwidth && !matches!(kind, Kind::Set1))
                {
                    2
                } else {
                    1
                } {
                    let f = format!("vl_lane_{n}");
                    n += 1;
                    let merge = if alias == 1 { "a" } else { "m" };
                    let argument = if matches!(kind, Kind::Set1) {
                        "value"
                    } else {
                        "a"
                    };
                    let suffix = match kind {
                        Kind::Extract => format!(",{index}"),
                        Kind::Insert => format!(",b,{index}"),
                        _ => String::new(),
                    };
                    let expr = match mode {
                        0 => format!("{pre}_{op}({argument}{suffix})"),
                        1 => format!("{pre}_mask_{op}({merge},k,{argument}{suffix})"),
                        _ => format!("{pre}_maskz_{op}(k,{argument}{suffix})"),
                    };
                    let declarations = format!(
                        "{out} m;{input} a;{block} b;__builtin_memcpy(&m,mp,{bytes});__builtin_memcpy(&a,ap,{ibytes});__builtin_memcpy(&b,bp,16);{} value;__builtin_memcpy(&value,ap,{lane_bytes});",
                        if bits == 32 { "int" } else { "long long" }
                    );
                    source += &format!(
                        "__attribute__((target(\"avx512vl\"))) void {f}(void*mp,void*ap,void*bp,unsigned char*out,unsigned char k){{{declarations}{out} r={expr};__builtin_memcpy(out,&r,{bytes});__builtin_memcpy(out+64,&m,{bytes});__builtin_memcpy(out+96,&a,{ibytes});__builtin_memcpy(out+128,&b,16);out[160]=k;}}\n"
                    );
                    oracle += &format!(
                        "extern void {f}(void*,void*,void*,unsigned char*,unsigned char);static void vendor_{f}(void*mp,void*ap,void*bp,unsigned char*out,unsigned char k){{{declarations}{out} r={expr};memcpy(out,&r,{bytes});memcpy(out+64,&m,{bytes});memcpy(out+96,&a,{ibytes});memcpy(out+128,&b,16);out[160]=k;}}\n"
                    );
                    let selected = match kind {
                        Kind::Scalar | Kind::Set1 => "load(a,lane_bytes)".to_owned(),
                        Kind::Broadcast => "load(a+(lane%4)*lane_bytes,lane_bytes)".to_owned(),
                        Kind::Extract => format!("load(a+({index}*4+lane)*lane_bytes,lane_bytes)"),
                        Kind::Insert => format!(
                            "lane/4=={index}?load(b+(lane%4)*lane_bytes,lane_bytes):load(a+lane*lane_bytes,lane_bytes)"
                        ),
                    };
                    let inactive = if mode == 2 {
                        "0".to_owned()
                    } else {
                        format!("load({merge}+lane*lane_bytes,lane_bytes)")
                    };
                    cases += &format!(
                        "{{unsigned char m[32],a[32],b[16],out[192],vendor[192],expected[192];const unsigned lane_bytes={lane_bytes};for(unsigned sample=0;sample<8;sample++){{for(unsigned i=0;i<{bytes};i++)m[i]=(unsigned char)(sample+i*19);for(unsigned i=0;i<{ibytes}/{lane_bytes};i++){{uint64_t value=edges[(sample+i)%8];memcpy(a+i*{lane_bytes},&value,{lane_bytes});}}for(unsigned i=0;i<16/{lane_bytes};i++){{uint64_t value=edges[(sample+i+3)%8];memcpy(b+i*{lane_bytes},&value,{lane_bytes});}}for(unsigned mi=0;mi<5;mi++){{unsigned char k=masks[mi];memset(out,0xa5,192);memset(vendor,0xa5,192);memset(expected,0xa5,192);for(unsigned lane=0;lane<{lanes};lane++){{uint64_t value=({mode}==0||((k>>lane)&1))?({selected}):({inactive});memcpy(expected+lane*lane_bytes,&value,lane_bytes);}}memcpy(expected+64,m,{bytes});memcpy(expected+96,a,{ibytes});memcpy(expected+128,b,16);expected[160]=k;unsigned original=_mm_getcsr();unsigned csr=(original&~0x80u)|0x3f;_mm_setcsr(csr);{f}(m,a,b,out,k);if(_mm_getcsr()!=csr)return 1;vendor_{f}(m,a,b,vendor,k);if(_mm_getcsr()!=csr)return 2;_mm_setcsr(original);if(memcmp(out,expected,192)||memcmp(out,vendor,192)){{fprintf(stderr,\"{f} {op} mode={mode} index={index} alias={alias} k=%x sample=%u\\n\",k,sample);return 3;}}}}}}}}\n"
                    );
                }
            }
        }
    }
    source += r#"
static int ac,bc,mc,kc;
static __attribute__((always_inline,target("avx512vl"))) __m256i nexta(void){ac++;return (__m256i){1,2,3,4};}
static __attribute__((always_inline)) __m128i nextb(void){bc++;return (__m128i){5,6};}
static __attribute__((always_inline,target("avx512vl"))) __m256i nextm(void){mc++;return (__m256i){7,8,9,10};}
static __attribute__((always_inline)) int nextk(void){kc++;return 0x85;}
__attribute__((target("avx512vl"))) int eval_lane(void){
 ac=bc=mc=kc=0;
 __m256i result=_mm256_mask_inserti32x4(nextm(),nextk(),nexta(),nextb(),1);
 unsigned long long words[4];__builtin_memcpy(words,&result,32);
 return ac!=1||bc!=1||mc!=1||kc!=1||words[3]!=10;
}
"#;
    // Inactive dword6 retains merge word3.low=10; active dword7 uses
    // source block word1.high=0, so the resulting qword is 10.
    oracle += "extern int eval_lane(void);\n";
    oracle += &format!(
        "static uint64_t load(const unsigned char*p,unsigned n){{uint64_t r=0;memcpy(&r,p,n);return r;}}int main(void){{if(eval_lane())return 4;uint64_t edges[8]={{UINT64_C(0x7ff012347f812345),UINT64_C(0x7ff854327fc54321),UINT64_C(0x8000000080000000),UINT64_MAX,UINT64_C(0x3ff000003f800000),UINT64_C(0xfff01234ff812345),0,1}};unsigned char masks[5]={{0,1,0x80,0x85,255}};{cases}return 0;}}\n"
    );
    (source, oracle)
}
#[test]
fn vl_lane_public_wrappers_preserve_payloads_masks_and_live_sources() {
    let (source, oracle) = fixtures();
    let mut module = parse(&source, keel_ir::Target::X86_64LinuxGnu).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::parse("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512dq && !features.avx512bw);
    let directory = std::env::temp_dir().join(format!("keel-c-vl-lane-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    #[cfg(target_arch = "x86_64")]
    let native = cfg!(target_os = "linux") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(target_arch = "x86_64"))]
    let native = false;
    if native {
        let output = Command::new("cc")
            .args([
                "-O2",
                "-mavx512f",
                "-mavx512vl",
                "-fno-tree-vectorize",
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
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &XMM12] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(directory.join("consumer.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("consumer.o"))
                    .arg(directory.join("oracle.o"))
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
}
#[test]
fn vl_lane_immediates_cast_and_caller_permissions() {
    for op in [
        "extracti32x4_epi32",
        "extractf32x4_ps",
        "inserti32x4",
        "insertf32x4",
    ] {
        let float = op.contains('f');
        let full = ctype(256, 32, float);
        let short = ctype(128, 32, float);
        let result = if op.starts_with("extract") {
            &short
        } else {
            &full
        };
        for imm in ["0", "1", "0x100000000ULL", "2", "-1", "runtime"] {
            let args = if op.starts_with("extract") {
                format!("a,{imm}")
            } else {
                format!("a,b,{imm}")
            };
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512vl\"))) {result} f({full} a,{short} b,int runtime){{return _mm256_{op}({args});}}"
            );
            let parsed = parse(&source, keel_ir::Target::X86_64LinuxGnu);
            assert_eq!(
                parsed.is_ok(),
                ["0", "1", "0x100000000ULL"].contains(&imm),
                "{op} {imm}"
            );
        }
    }
    let (source, _) = fixtures();
    let mut module = parse(
        &source.replacen("target(\"avx512vl\")", "target(\"avx512f\")", 1),
        keel_ir::Target::X86_64LinuxGnu,
    )
    .unwrap();
    keel_opt::optimize(&mut module).unwrap();
    assert!(keel_codegen::compile_program(&module, keel_ir::Target::X86_64LinuxGnu).is_err());
}

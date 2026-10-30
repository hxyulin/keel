//! Public BW word-shift wrappers preserve raw saturating count semantics.
#[path = "../../../keel-codegen/tests/common/avx512_bw_shift.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    for case in cases {
        assert_eq!(
            case.mask.bytes().unwrap() * 8,
            u32::from(case.vector.lanes()).max(8)
        );
        let width = case.vector.bytes() * 8;
        let vector = format!("__m{width}i");
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let lane = if width == 512 {
            "__keel_bw_i16_512".into()
        } else {
            format!("__keel_i16_{width}")
        };
        let features = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        let name = case.name();
        let load = |p: &str| {
            if width == 512 {
                format!("_mm512_loadu_si512({p})")
            } else {
                format!("{prefix}_loadu_si{width}((const {vector}*){p})")
            }
        };
        let store = |p: &str, value: &str| {
            if width == 512 {
                format!("_mm512_storeu_si512({p},{value});")
            } else {
                format!("{prefix}_storeu_si{width}(({vector}*)({p}),{value});")
            }
        };
        source += &format!(
            "__attribute__((target(\"{features}\"))) void {name}(const void*pm,const void*pa,const void*pc,void*out,uint64_t k){{{vector} m={},a={};",
            load("pm"),
            load("pa")
        );
        if case.kind == 1 {
            source += "__m128i count=_mm_loadu_si128((const __m128i*)pc);";
        } else {
            source += &format!("{vector} count={};", load("pc"));
        }
        let merge = if case.alias { "a" } else { "m" };
        let amount = match case.kind {
            0 => case.immediate.to_string(),
            1 => "count".into(),
            2 => {
                if case.alias {
                    "a".into()
                } else {
                    "count".into()
                }
            }
            _ => unreachable!(),
        };
        let api = common::api(*case);
        source += &format!("{vector} result={api}({merge},k,a,{amount});");
        if width == 512 || case.kind == 2 {
            let all = format!("((1ULL<<{})-1)", case.vector.lanes());
            source += &format!(
                "if(k=={all})result={}(a,{amount});",
                api.replacen("_mask_", "_", 1)
            );
        }
        source += &format!(
            "{vector} zero={}(k,a,{amount});",
            api.replacen("_mask_", "_maskz_", 1)
        );
        source += &store("out", "result");
        source += &format!(
            "for(unsigned i=0;i<{};i++)if((((k>>i)&1)?(({lane})result)[i]:0)!=(({lane})zero)[i])((unsigned char*)out)[0]^=1;",
            case.vector.lanes()
        );
        source += &store("(char*)out+64", "m");
        source += &store("(char*)out+128", "a");
        if case.kind == 1 {
            source += "_mm_storeu_si128((__m128i*)((char*)out+192),count);";
        } else {
            source += &store("(char*)out+192", "count");
        }
        source += "*(uint64_t*)((char*)out+256)=k;}\n";
    }
    source
}
#[test]
fn bw_shift_c_counts_masks_aliases_and_live_payloads() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-shift-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let output = Command::new("clang")
            .arg("-O2")
            .arg("-c")
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for optimize in [false, true] {
        let mut module = raw.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let encoding = module_features(&module, Features::NONE).unwrap();
        assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE,
        };
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (spilled, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
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
                    keel_asm::x86::assemble_program_object_with_features(&program, encoding)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
                );
                let name = format!("{allocation:?}-{spilled}-{optimize}");
                let path = dir.join(format!("{name}.o"));
                fs::write(&path, object).unwrap();
                if native {
                    let exe = dir.join(name);
                    let linked = Command::new("clang")
                        .arg(dir.join("runner.o"))
                        .arg(path)
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        linked.status.success(),
                        "{}",
                        String::from_utf8_lossy(&linked.stderr)
                    );
                    let output = Command::new(exe).output().unwrap();
                    assert!(
                        output.status.success(),
                        "{allocation:?} spill={spilled} optimized={optimize}: {output:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn bw_shift_c_raw_immediate_and_permission_boundaries() {
    for count in ["-1", "256", "p"] {
        let source = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) __m512i f(__m512i a,int p){{return (__m512i)__builtin_keel_avx512bw_slliw512((__keel_bw_i16_512)a,-1,(__keel_bw_i16_512)a,{count});}}"
        );
        assert!(parse(&source, Target::X86_64LinuxGnu).is_err(), "{count}");
    }
    for features in [
        "",
        "__attribute__((target(\"avx512f,avx512vl\")))",
        "__attribute__((target(\"avx512bw\")))",
    ] {
        let source = format!(
            "#include <immintrin.h>\n{features} __m128i f(__m128i a){{return _mm_mask_slli_epi16(a,-1,a,1);}}"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        assert!(
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).is_err(),
            "{features}"
        );
    }
}

#[test]
fn bw_shift_c_scalar_counts_convert_to_unsigned_int_once() {
    let mut source = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nunsigned bw_scalar_evaluations;\nstatic __attribute__((always_inline)) inline uint64_t once(uint64_t n){bw_scalar_evaluations++;return n;}\n",
    );
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut table = String::new();
    for width in [128, 256, 512] {
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let vector = format!("__m{width}i");
        let features = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        let load = |p: &str| {
            if width == 512 {
                format!("_mm512_loadu_si512({p})")
            } else {
                format!("{prefix}_loadu_si{width}((const {vector}*){p})")
            }
        };
        let store = |p: &str, v: &str| {
            if width == 512 {
                format!("_mm512_storeu_si512({p},{v});")
            } else {
                format!("{prefix}_storeu_si{width}(({vector}*)({p}),{v});")
            }
        };
        for (direction, stem) in [(0, "slli"), (1, "srli"), (2, "srai")] {
            for mode in 0..if width == 512 { 3 } else { 2 } {
                let name = format!("bw_scalar_{width}_{direction}_{mode}");
                let public = match mode {
                    0 => format!("{prefix}_mask_{stem}_epi16"),
                    1 => format!("{prefix}_maskz_{stem}_epi16"),
                    _ => format!("{prefix}_{stem}_epi16"),
                };
                let invoke = |count: &str| match mode {
                    0 => format!("{public}(m,k,a,{count})"),
                    1 => format!("{public}(k,a,{count})"),
                    _ => format!("{public}(a,{count})"),
                };
                source += &format!(
                    "__attribute__((target(\"{features}\"))) void {name}(const void*pm,const void*pa,void*out,uint64_t k,uint64_t raw){{{vector} m={},a={};bw_scalar_evaluations=0;{vector} r={};",
                    load("pm"),
                    load("pa"),
                    invoke("once(raw)")
                );
                source += &store("out", "r");
                source += "*(unsigned*)((char*)out+64)=bw_scalar_evaluations;";
                source += &store("(char*)out+128", "m");
                source += &store("(char*)out+192", "a");
                source += "*(uint64_t*)((char*)out+256)=k;*(uint64_t*)((char*)out+264)=raw;}\n";
                oracle += &format!(
                    "extern void {name}(const void*,const void*,void*,uint64_t,uint64_t);\n__attribute__((target(\"avx512f,avx512bw,avx512vl\"))) static void ref_{name}(const void*pm,const void*pa,void*out,uint64_t k,uint64_t raw){{{vector} m={},a={};{vector} r={};",
                    load("pm"),
                    load("pa"),
                    invoke("(unsigned int)raw")
                );
                oracle += &store("out", "r");
                oracle += "}\n";
                table += &format!(
                    "{{\"{name}\",{name},ref_{name},{},{direction},{mode}}},\n",
                    width / 8
                );
            }
        }
    }
    oracle += "struct Case{const char*name;void(*run)(const void*,const void*,void*,uint64_t,uint64_t);void(*reference)(const void*,const void*,void*,uint64_t,uint64_t);unsigned bytes,direction,mode;};\nstatic const struct Case cases[]={\n";
    oracle += &table;
    oracle += "};\n";
    oracle += r#"
static unsigned word(const unsigned char*p){unsigned n=0;memcpy(&n,p,2);return n;}
static void put(unsigned char*p,unsigned n){memcpy(p,&n,2);}
static unsigned scalar(unsigned direction,unsigned x,unsigned count){
 if(count>=16)return direction==2&&(x&0x8000)?0xffff:0;
 if(count==0)return x;
 if(direction==0)return (x<<count)&0xffff;
 if(direction==1||!(x&0x8000))return x>>count;
 return ((x>>count)|(0xffffu<<(16-count)))&0xffff;
}
int main(void){
 uint64_t counts[]={0,15,16,17,255,256,UINT32_MAX,UINT64_C(1)<<32,(UINT64_C(1)<<32)+15,(uint64_t)(int64_t)-1,(uint64_t)(int64_t)-16,UINT64_C(1)<<63};
 unsigned edges[]={0,1,0x8000,0xffff,0x7fff,0x8001,15,16};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;unsigned n=t->bytes/2;uint64_t all=(UINT64_C(1)<<n)-1,masks[]={0,all,0xa55a8001,UINT64_C(1)<<(n-1)};
  unsigned char m[64],a[64],out[288],reference[64];for(unsigned i=0;i<n;i++){put(m+i*2,0x1234+i*17);put(a+i*2,edges[i%8]);}
  for(unsigned p=0;p<4;p++)for(unsigned j=0;j<sizeof(counts)/sizeof(counts[0]);j++){
   uint64_t k=masks[p],raw=counts[j],saved_mask=0,saved_raw=0;unsigned evaluated=0;memset(out,0xa5,sizeof out);
   t->run(m,a,out,k,raw);t->reference(m,a,reference,k,raw);
   if(memcmp(out,reference,t->bytes)){printf("native %s count%llx mask%llx\n",t->name,(unsigned long long)raw,(unsigned long long)k);return 1;}
   for(unsigned i=0;i<n;i++){unsigned active=t->mode==2||((k>>i)&1),merge=t->mode==1?0:word(m+i*2);unsigned expected=active?scalar(t->direction,word(a+i*2),(unsigned int)raw):merge;if(word(out+i*2)!=expected)return 2;}
   memcpy(&evaluated,out+64,4);memcpy(&saved_mask,out+256,8);memcpy(&saved_raw,out+264,8);
   if(evaluated!=1||memcmp(m,out+128,t->bytes)||memcmp(a,out+192,t->bytes)||saved_mask!=k||saved_raw!=raw)return 3;
   if((t->bytes<64&&out[t->bytes]!=0xa5)||out[68]!=0xa5||out[272]!=0xa5)return 4;
  }
 }
 return 0;
}
"#;
    // This must accept a runtime scalar count; the raw imm8 builtin remains a
    // separate contract and is tested by the existing immediate boundary test.
    let raw = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-scalar-count-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), oracle).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let output = Command::new("clang")
            .arg("-O2")
            .arg("-c")
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for optimize in [false, true] {
        let mut module = raw.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let encoding = module_features(&module, Features::NONE).unwrap();
        assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE,
        };
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (spilled, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
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
                    keel_asm::x86::assemble_program_object_with_features(&program, encoding)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
                );
                let name = format!("{allocation:?}-{spilled}-{optimize}");
                let path = dir.join(format!("{name}.o"));
                fs::write(&path, object).unwrap();
                if native {
                    let exe = dir.join(name);
                    let linked = Command::new("clang")
                        .arg(dir.join("runner.o"))
                        .arg(path)
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        linked.status.success(),
                        "{}",
                        String::from_utf8_lossy(&linked.stderr)
                    );
                    let output = Command::new(exe).output().unwrap();
                    assert!(
                        output.status.success(),
                        "{allocation:?} spill={spilled} optimized={optimize}: {output:?}"
                    );
                }
            }
        }
    }
}

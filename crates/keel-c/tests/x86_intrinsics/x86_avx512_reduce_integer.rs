//! Integer reductions match unsigned scalar arithmetic and native header semantics.
use super::pclmul::parse;
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
const OPERATIONS: [(&str, &str, u8); 8] = [
    ("add", "epi", 0),
    ("mul", "epi", 1),
    ("and", "epi", 2),
    ("or", "epi", 3),
    ("min", "epi", 4),
    ("max", "epi", 5),
    ("min", "epu", 6),
    ("max", "epu", 7),
];
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(evaluations);\nstatic __inline__ __attribute__((always_inline)) __m512i take_vector(const void*p){__m512i v;FN(evaluations)++;__builtin_memcpy(&v,p,64);return v;}\nstatic __inline__ __attribute__((always_inline)) unsigned short take_mask(unsigned k){FN(evaluations)++;return k;}\n",
    );
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nextern int evaluations,reference_evaluations;\ntypedef void(*Fn)(const void*,void*,unsigned);\nstruct Case{Fn actual,reference;unsigned bits,operation,masked;const char*name;};\n",
    );
    let mut rows = String::new();
    let mut case = 0;
    for bits in [32, 64] {
        for (operation, signedness, code) in OPERATIONS {
            for masked in [false, true] {
                let name = format!(
                    "{}reduce_{operation}_{signedness}{bits}",
                    if masked { "mask_" } else { "" }
                );
                let ty = match (bits, signedness) {
                    (32, "epu") => "unsigned int",
                    (32, _) => "int",
                    (64, "epu") => "unsigned long long",
                    _ => "long long",
                };
                let args = if masked {
                    "take_mask(k),take_vector(p)"
                } else {
                    "take_vector(p)"
                };
                source += &format!(
                    "void FN(case_{case})(const void*p,void*out,unsigned k){{{ty} value=_mm512_{name}({args});__builtin_memcpy(out,&value,{});}}\n",
                    bits / 8
                );
                harness += &format!(
                    "extern void case_{case}(const void*,void*,unsigned);\nextern void reference_case_{case}(const void*,void*,unsigned);\n"
                );
                rows += &format!(
                    "{{case_{case},reference_case_{case},{bits},{code},{},\"{name}\"}},\n",
                    u8::from(masked)
                );
                case += 1;
            }
        }
    }
    harness += &format!("static const struct Case cases[]={{{rows}}};\n");
    harness += "static uint64_t oracle(const unsigned char*p,unsigned bits,unsigned operation,unsigned mask){uint64_t maximum=bits==32?UINT32_MAX:UINT64_MAX,sign=UINT64_C(1)<<(bits-1),result=operation==1?1:operation==2||operation==6?maximum:operation==4?sign-1:operation==5?sign:0;for(unsigned i=0;i<512/bits;i++)if((mask>>i)&1){uint64_t value=0;memcpy(&value,p+i*(bits/8),bits/8);switch(operation){case 0:result=(result+value)&maximum;break;case 1:result=(result*value)&maximum;break;case 2:result&=value;break;case 3:result|=value;break;case 4:if((value^sign)<(result^sign))result=value;break;case 5:if((value^sign)>(result^sign))result=value;break;case 6:if(value<result)result=value;break;case 7:if(value>result)result=value;break;}}return result;}\nint main(void){unsigned saved=_mm_getcsr();unsigned char data[80],expected_data[80],out[16],reference[16];for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){const struct Case*c=&cases[ci];unsigned lanes=512/c->bits,bytes=c->bits/8;uint64_t maximum=c->bits==32?UINT32_MAX:UINT64_MAX,sign=UINT64_C(1)<<(c->bits-1);for(unsigned sample=0;sample<lanes+6;sample++){memset(data,0xa5,sizeof(data));for(unsigned i=0;i<lanes;i++){uint64_t value=sample==0?0:sample==1?maximum:sample==2?i+1:sample==3?(i%2?sign:sign-1):sample==4?(UINT64_C(0x9e3779b97f4a7c15)*(i+1))&maximum:sample==5?(maximum-i*2)&maximum:i==sample-6?sign:(UINT64_C(1)<<(i%(c->bits-1)));memcpy(data+1+i*bytes,&value,bytes);}const unsigned masks[]={0,0xffff,1,1u<<(lanes-1),0xaaaa,0x100,0x80000000u};for(unsigned pi=0;pi<(c->masked?7:1);pi++){unsigned k=c->masked?masks[pi]:(1u<<lanes)-1;uint64_t wanted=oracle(data+1,c->bits,c->operation,k);for(unsigned alias=0;alias<3;alias++){unsigned char actual_data[80];memcpy(actual_data,data,sizeof(data));memcpy(expected_data,data,sizeof(data));memset(out,0xa5,sizeof(out));memset(reference,0xa5,sizeof(reference));void*actual_out=alias==0?out:actual_data+1+(alias==1?0:64-bytes);void*reference_out=alias==0?reference:expected_data+1+(alias==1?0:64-bytes);evaluations=reference_evaluations=0;_mm_setcsr(0x1f80);c->actual(actual_data+1,actual_out,k);unsigned flags=_mm_getcsr()&63;c->reference(expected_data+1,reference_out,k);uint64_t actual=0,native=0;memcpy(&actual,actual_out,bytes);memcpy(&native,reference_out,bytes);if(actual!=wanted||native!=wanted||memcmp(actual_data,expected_data,sizeof(data))||memcmp(out,reference,sizeof(out))||flags||evaluations!=(int)(1+c->masked)||reference_evaluations!=(int)(1+c->masked)){fprintf(stderr,\"%s sample%u mask%x alias%u got%llx native%llx wanted%llx eval%d/%d flags%x\\n\",c->name,sample,k,alias,(unsigned long long)actual,(unsigned long long)native,(unsigned long long)wanted,evaluations,reference_evaluations,flags);return 1;}}}}}_mm_setcsr(saved);return 0;}\n";
    (source, harness)
}
fn command(cmd: &mut Command) {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_integer_reduce_bridge_signatures() {
    let (source, _) = sources();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let covered: std::collections::HashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| match *f.inst_data(i) {
                keel_ir::InstData::VectorReduce { op, arg } => Some((op, f.value_type(arg))),
                _ => None,
            })
        })
        .collect();
    assert_eq!(covered.len(), 14);
    for body in [
        "int f(__m512i a){return __builtin_keel_avx512_reduce_add_i32x16(a);}",
        "long long f(__m256i a){return __builtin_keel_avx512_reduce_smax_i64x8(a);}",
    ] {
        assert!(
            parse(
                &format!("#include <immintrin.h>\n{body}\n"),
                Target::X86_64LinuxGnu
            )
            .is_err()
        );
    }
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &p).is_err());
}
#[test]
fn avx512_integer_reduce_native_identities_wrap_and_single_evaluation() {
    let (source, harness) = sources();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!(
        "keel-c-avx512-reduce-integer-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    };
    if native {
        command(
            Command::new("clang")
                .args(["-O1", "-mavx512f", "-DREFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-c"])
                .arg(dir.join("harness.c"))
                .arg("-o")
                .arg(dir.join("harness.o")),
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
    ] {
        let mut m = original.clone();
        if opt {
            keel_opt::optimize(&mut m).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(dir.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(dir.join("actual.o"))
                    .arg(dir.join("reference.o"))
                    .arg(dir.join("harness.o"))
                    .arg("-o")
                    .arg(dir.join("run")),
            );
            command(&mut Command::new(dir.join("run")));
        }
    }
}

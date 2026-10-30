//! Constructors preserve lane order; casts compare only their defined lanes.
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
struct Case {
    api: String,
    result: &'static str,
    args: Vec<String>,
    bytes: usize,
    defined: usize,
    scalars: usize,
    vectors: usize,
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (suffix, scalar, result, lanes, width) in [
        ("epi32", "int", "__m512i", 16, 4),
        ("epi64", "ll", "__m512i", 8, 8),
        ("ps", "float", "__m512", 16, 4),
        ("pd", "double", "__m512d", 8, 8),
    ] {
        for (prefix, count) in [("set", lanes), ("setr", lanes), ("set4", 4), ("setr4", 4)] {
            cases.push(Case {
                api: format!("_mm512_{prefix}_{suffix}"),
                result,
                args: (0..count)
                    .map(|i| format!("take_{scalar}(a+{})", i * width))
                    .collect(),
                bytes: 64,
                defined: 64,
                scalars: count,
                vectors: 0,
            });
        }
    }
    for (bits, scalar, lanes, width) in [(8, "char", 64, 1), (16, "short", 32, 2)] {
        cases.push(Case {
            api: format!("_mm512_set_epi{bits}"),
            result: "__m512i",
            args: (0..lanes)
                .map(|i| format!("take_{scalar}(a+{})", i * width))
                .collect(),
            bytes: 64,
            defined: 64,
            scalars: lanes,
            vectors: 0,
        });
        cases.push(Case {
            api: format!("_mm512_set1_epi{bits}"),
            result: "__m512i",
            args: vec![format!("take_{scalar}(a)")],
            bytes: 64,
            defined: 64,
            scalars: 1,
            vectors: 0,
        });
    }
    for (api, result) in [
        ("_mm512_setzero", "__m512"),
        ("_mm512_setzero_epi32", "__m512i"),
    ] {
        cases.push(Case {
            api: api.into(),
            result,
            args: vec![],
            bytes: 64,
            defined: 64,
            scalars: 0,
            vectors: 0,
        });
    }
    for (kind, full, small128, small256, reader) in [
        ("si", "__m512i", "__m128i", "__m256i", "i"),
        ("ps", "__m512", "__m128", "__m256", "ps"),
        ("pd", "__m512d", "__m128d", "__m256d", "pd"),
    ] {
        for (width, small) in [(128, small128), (256, small256)] {
            for (zext, prefix) in [(false, "cast"), (true, "zext")] {
                cases.push(Case {
                    api: format!("_mm512_{prefix}{kind}{width}_{kind}512"),
                    result: full,
                    args: vec![format!("take_{reader}{width}(a)")],
                    bytes: 64,
                    defined: if zext { 64 } else { width / 8 },
                    scalars: 0,
                    vectors: 1,
                });
            }
            cases.push(Case {
                api: format!("_mm512_cast{kind}512_{kind}{width}"),
                result: small,
                args: vec![format!("take_{reader}512(a)")],
                bytes: width / 8,
                defined: width / 8,
                scalars: 0,
                vectors: 1,
            });
        }
    }
    for (api, result, reader, bytes) in [
        ("_mm512_cvtsi512_si32", "int", "i", 4),
        ("_mm512_cvtss_f32", "float", "ps", 4),
        ("_mm512_cvtsd_f64", "double", "pd", 8),
    ] {
        cases.push(Case {
            api: api.into(),
            result,
            args: vec![format!("take_{reader}512(a)")],
            bytes,
            defined: bytes,
            scalars: 0,
            vectors: 1,
        });
    }
    cases
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(scalar_count),FN(vector_count);\n",
    );
    for (name, ty, width) in [
        ("char", "char", 1),
        ("short", "short", 2),
        ("int", "int", 4),
        ("ll", "long long", 8),
        ("float", "float", 4),
        ("double", "double", 8),
    ] {
        source += &format!(
            "static __inline__ __attribute__((always_inline)) {ty} take_{name}(const void*p){{{ty} value;FN(scalar_count)++;__builtin_memcpy(&value,p,{width});return value;}}\n"
        );
    }
    for (name, ty, width) in [
        ("i128", "__m128i", 16),
        ("i256", "__m256i", 32),
        ("i512", "__m512i", 64),
        ("ps128", "__m128", 16),
        ("ps256", "__m256", 32),
        ("ps512", "__m512", 64),
        ("pd128", "__m128d", 16),
        ("pd256", "__m256d", 32),
        ("pd512", "__m512d", 64),
    ] {
        source += &format!(
            "static __inline__ __attribute__((always_inline)) {ty} take_{name}(const void*p){{{ty} value;FN(vector_count)++;__builtin_memcpy(&value,p,{width});return value;}}\n"
        );
    }
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nextern int scalar_count,vector_count,reference_scalar_count,reference_vector_count;\ntypedef void(*Fn)(const unsigned char*,void*);\nstruct Case{Fn actual,reference;unsigned bytes,defined,scalars,vectors;};\n",
    );
    let mut entries = String::new();
    for (number, case) in cases().into_iter().enumerate() {
        source += &format!(
            "void FN(case_{number})(const unsigned char*a,void*out){{{} value={}({});__builtin_memcpy(out,&value,{});}}\n",
            case.result,
            case.api,
            case.args.join(","),
            case.bytes
        );
        harness += &format!(
            "extern void case_{number}(const unsigned char*,void*),reference_case_{number}(const unsigned char*,void*);\n"
        );
        entries += &format!(
            "{{case_{number},reference_case_{number},{},{},{},{}}},\n",
            case.bytes, case.defined, case.scalars, case.vectors
        );
    }
    source += "void FN(constant)(void*out){__m512i value=_mm512_set_epi32(15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,0);__builtin_memcpy(out,&value,64);}\n";
    harness += "extern void constant(void*),reference_constant(void*);\n";
    harness += &format!(
        r#"
static const struct Case cases[]={{{entries}}};
int main(void){{long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard,page,PROT_READ|PROT_WRITE))return 1;unsigned char*input=guard+page-64;unsigned char actual[96],expected[96];for(unsigned sample=0;sample<4;sample++){{for(unsigned i=0;i<64;i++)input[i]=(unsigned char)(0x81+i*37+sample*19);uint32_t special[]={{0x80000000,0x7f800001,0xffc12345,0}};memcpy(input,&special[sample],4);for(unsigned index=0;index<sizeof(cases)/sizeof(*cases);index++){{const struct Case*c=&cases[index];scalar_count=vector_count=reference_scalar_count=reference_vector_count=0;memset(actual,0xa5,sizeof actual);memset(expected,0xa5,sizeof expected);c->actual(input,actual);c->reference(input,expected);if(memcmp(actual,expected,c->defined)||scalar_count!=c->scalars||reference_scalar_count!=c->scalars||vector_count!=c->vectors||reference_vector_count!=c->vectors){{fprintf(stderr,"constructor case%u sample%u scalars%d/%d expected%u vectors%d/%d expected%u\n",index,sample,scalar_count,reference_scalar_count,c->scalars,vector_count,reference_vector_count,c->vectors);return 2;}}for(unsigned i=c->bytes;i<sizeof actual;i++)if(actual[i]!=0xa5)return 3;}}}}constant(actual);reference_constant(expected);if(memcmp(actual,expected,64))return 4;for(unsigned i=0;i<16;i++){{uint32_t got;memcpy(&got,actual+i*4,4);if(got!=i)return 5;}}return 0;}}
"#
    );
    (source, harness)
}
fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_constructor_surface_and_feature_legality() {
    assert_eq!(cases().len(), 43);
    let (source, _) = sources();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .is_err()
    );
}
#[test]
fn avx512_constructor_native_vendor_defined_lanes_and_single_evaluation() {
    let (source, harness) = sources();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let directory =
        std::env::temp_dir().join(format!("keel-c-avx512-construct-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.c"), source).unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
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
            Command::new("cc")
                .args([
                    "-mavx512f",
                    "-mno-avx512dq",
                    "-mno-avx512bw",
                    "-O1",
                    "-DREFERENCE",
                    "-c",
                ])
                .arg(directory.join("source.c"))
                .arg("-o")
                .arg(directory.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-c"])
                .arg(directory.join("harness.c"))
                .arg("-o")
                .arg(directory.join("harness.o")),
        );
    }
    for (tier, allocator, pools, optimize) in [
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
        let mut module = original.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &allocator,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(directory.join("actual.o"))
                    .arg(directory.join("reference.o"))
                    .arg(directory.join("harness.o"))
                    .arg("-o")
                    .arg(directory.join("run")),
            );
            command(&mut Command::new(directory.join("run")));
        }
    }
}

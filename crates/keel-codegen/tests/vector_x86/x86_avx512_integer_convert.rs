//! Foundation conversions retain shortened operands and suppress inactive accesses.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};

struct Recipe {
    id: Intrinsic,
    source_bits: usize,
    target_bits: usize,
    input_bytes: usize,
    output_bytes: usize,
    lanes: usize,
    kind: u8,
    load: bool,
    store: bool,
}
impl Recipe {
    fn new(id: Intrinsic) -> Self {
        let opcode = id.x86_avx512_integer_convert_opcode().unwrap();
        let suffix = opcode.trim_start_matches("vpmov");
        let widen = suffix.starts_with("sx") || suffix.starts_with("zx");
        let pair = &suffix[suffix.len() - 2..];
        let bits = |letter| match letter {
            'b' => 8,
            'w' => 16,
            'd' => 32,
            'q' => 64,
            _ => unreachable!(),
        };
        let source_bits = bits(pair.chars().next().unwrap());
        let target_bits = bits(pair.chars().nth(1).unwrap());
        let lanes = 512 / if widen { target_bits } else { source_bits };
        let input_bytes = if widen {
            (lanes * source_bits / 8).max(16)
        } else {
            64
        };
        let output_bytes = if widen {
            64
        } else {
            (lanes * target_bits / 8).max(16)
        };
        let kind = if suffix.starts_with("sx") {
            0
        } else if suffix.starts_with("zx") {
            1
        } else if suffix.starts_with("us") {
            4
        } else if suffix.starts_with('s') {
            3
        } else {
            2
        };
        Self {
            id,
            source_bits,
            target_bits,
            input_bytes,
            output_bytes,
            lanes,
            kind,
            load: id.is_x86_avx512_integer_convert_load(),
            store: id.is_x86_avx512_integer_convert_store(),
        }
    }
    fn mask(&self) -> &'static str {
        if self.lanes == 16 { "i16" } else { "i8" }
    }
    fn input(&self) -> String {
        format!(
            "i{}x{}",
            self.source_bits,
            self.input_bytes * 8 / self.source_bits
        )
    }
    fn output(&self) -> String {
        format!(
            "i{}x{}",
            self.target_bits,
            self.output_bytes * 8 / self.target_bits
        )
    }
    fn reference(&self) -> String {
        if self.kind < 2 {
            format!(
                "_mm512_mask_cvt{}{}_epi{}",
                if self.kind == 0 { "epi" } else { "epu" },
                self.source_bits,
                self.target_bits
            )
        } else {
            format!(
                "_mm512_mask_cvt{}epi{}_epi{}",
                match self.kind {
                    3 => "s",
                    4 => "us",
                    _ => "",
                },
                self.source_bits,
                self.target_bits
            )
        }
    }
}
fn recipes() -> Vec<Recipe> {
    Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_integer_convert())
        .map(Recipe::new)
        .collect()
}
fn source(recipes: &[Recipe]) -> String {
    let mut result = String::new();
    for recipe in recipes {
        let name = recipe.id.info().name;
        let mask = recipe.mask();
        let input = recipe.input();
        let output = recipe.output();
        if recipe.store {
            result += &format!(
                "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{input} align 1 v0\n intrinsic {name}(v1,v4,v3)\n store align 1 v4,v2\n v5 = iconst.i64 64\n v6 = ptradd v2,v5\n store align 1 v3,v6\n return\n}}\n"
            );
        } else if recipe.load {
            result += &format!(
                "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{output} align 1 v0\n v5 = intrinsic {name}(v1,v4,v3)\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v4,v7\n v8 = ptradd v7,v6\n store align 1 v3,v8\n return\n}}\n"
            );
        } else {
            result += &format!(
                "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{output} align 1 v0\n v5 = load.{input} align 1 v1\n v6 = intrinsic {name}(v4,v3,v5)\n store align 1 v6,v2\n v7 = iconst.i64 64\n v8 = ptradd v2,v7\n store align 1 v5,v8\n v9 = ptradd v8,v7\n store align 1 v4,v9\n v10 = ptradd v9,v7\n store align 1 v3,v10\n return\n}}\n"
            );
        }
    }
    result
}
fn harness(recipes: &[Recipe]) -> String {
    let mut result = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
static uint64_t widthmask(unsigned bits){return bits==64?UINT64_MAX:(UINT64_C(1)<<bits)-1;}
static uint64_t readbits(const void*p,unsigned bytes){uint64_t value=0;memcpy(&value,p,bytes);return value;}
static int64_t signedbits(uint64_t value,unsigned bits){uint64_t mask=widthmask(bits);return value&(UINT64_C(1)<<(bits-1))?-(int64_t)((~value)&mask)-1:(int64_t)value;}
static uint64_t convert(unsigned kind,uint64_t value,unsigned from,unsigned to){
 uint64_t mask=widthmask(to);
 if(kind==0)return (uint64_t)signedbits(value,from)&mask;
 if(kind==1||kind==2)return value&mask;
 if(kind==4)return value>mask?mask:value;
 int64_t x=signedbits(value,from),limit=INT64_C(1)<<(to-1),min=-limit,max=limit-1;
 return (uint64_t)(x<min?min:x>max?max:x)&mask;
}
static void initialize(unsigned char*input,unsigned bytes,unsigned from,unsigned to,unsigned kind,unsigned phase){
 uint64_t sm=widthmask(from),sign=UINT64_C(1)<<(from-1),limit=UINT64_C(1)<<(to==64?63:to-1),dm=widthmask(to);
 uint64_t widen[]={0,1,sign-1,sign,sm,sign+1,UINT64_C(0x5555555555555555)&sm,UINT64_C(0xaaaaaaaaaaaaaaaa)&sm};
 uint64_t narrow[]={0,1,limit-1,limit,dm,dm+1,(uint64_t)(-limit)&sm,(uint64_t)(-limit-1)&sm};
 for(unsigned i=0;i<bytes/(from/8);i++){uint64_t value=(kind<2||phase?widen[i%8]:narrow[i%8])&sm;memcpy(input+i*(from/8),&value,from/8);}
}
static void scalar(unsigned char*out,const unsigned char*merge,const unsigned char*input,unsigned kind,unsigned from,unsigned to,unsigned outputbytes,unsigned lanes,unsigned k){
 if(out!=merge)memcpy(out,merge,outputbytes);
 for(unsigned i=0;i<outputbytes/(to/8);i++){
  if(i>=lanes){memset(out+i*(to/8),0,to/8);continue;}
  if((k>>i)&1){uint64_t value=convert(kind,readbits(input+i*(from/8),from/8),from,to);memcpy(out+i*(to/8),&value,to/8);}
 }
}
"#,
    );
    let mut checks = String::new();
    for recipe in recipes {
        let name = recipe.id.info().name;
        let c_mask = if recipe.lanes == 16 {
            "uint16_t"
        } else {
            "uint8_t"
        };
        let from = recipe.source_bits;
        let to = recipe.target_bits;
        let inputbytes = recipe.input_bytes;
        let outputbytes = recipe.output_bytes;
        let kind = recipe.kind;
        let lanes = recipe.lanes;
        let vector = |bytes| match bytes {
            16 => "__m128i",
            32 => "__m256i",
            64 => "__m512i",
            _ => unreachable!(),
        };
        result += &format!(
            "extern void {name}(void*,void*,void*,{c_mask});\nstatic void ref_{name}(const void*merge,const void*input,unsigned k,void*out){{{} m,r;{} a;memcpy(&m,merge,{outputbytes});memcpy(&a,input,{inputbytes});r={}(m,({c_mask})k,a);memcpy(out,&r,{outputbytes});}}\n",
            vector(outputbytes),
            vector(inputbytes),
            recipe.reference()
        );
        let setup = format!(
            "unsigned char input[64],merge[64],out[256],expected[64],reference[64];for(unsigned i=0;i<64;i++)merge[i]=(unsigned char)(0x81+i*37);{c_mask} masks[]={{0,1,1u<<({lanes}-1),({c_mask})0xa55a,({c_mask})~0u}};"
        );
        if !recipe.load && !recipe.store {
            checks += &format!(
                "{{{setup}for(unsigned phase=0;phase<2;phase++){{initialize(input,{inputbytes},{from},{to},{kind},phase);for(unsigned p=0;p<5;p++){{{c_mask} k=masks[p],saved;memset(out,0xa5,sizeof out);scalar(expected,merge,input,{kind},{from},{to},{outputbytes},{lanes},k);ref_{name}(merge,input,k,reference);{name}(merge,input,out,k);memcpy(&saved,out+192,sizeof saved);if(memcmp(out,expected,{outputbytes})||memcmp(out,reference,{outputbytes})||memcmp(out+64,input,{inputbytes})||memcmp(out+128,merge,{outputbytes})||saved!=k)return 10;for(unsigned i={outputbytes};i<64;i++)if(out[i]!=0xa5)return 11;for(unsigned i=64+{inputbytes};i<128;i++)if(out[i]!=0xa5)return 12;for(unsigned i=128+{outputbytes};i<192;i++)if(out[i]!=0xa5)return 13;}}}}}}\n"
            );
        } else {
            let element = if recipe.load { from / 8 } else { to / 8 };
            let capacity = recipe.id.x86_avx512_integer_convert_memory_bytes().unwrap();
            let operation = if recipe.load {
                format!(
                    "for(unsigned i=0;i<{lanes};i++)if((k>>i)&1)memcpy(base+i*{element},input+i*{element},{element});scalar(expected,merge,input,{kind},{from},{to},{outputbytes},{lanes},k);ref_{name}(merge,input,k,reference);{name}(merge,base,out,k);memcpy(&saved,out+128,sizeof saved);if(memcmp(out,expected,{outputbytes})||memcmp(out,reference,{outputbytes})||memcmp(out+64,merge,64)||saved!=k)return 20;"
                )
            } else {
                format!(
                    "memset(expected,0,sizeof expected);scalar(expected,expected,input,{kind},{from},{to},{outputbytes},{lanes},k);unsigned char zero[64]={{0}};ref_{name}(zero,input,k,reference);if(memcmp(expected,reference,{outputbytes}))return 21;for(unsigned i=0;i<{lanes};i++)if((k>>i)&1)memcpy(page_expected+(base+i*{element}-(guard+page)),expected+i*{element},{element});{name}(input,base,out,k);memcpy(&saved,out+64,sizeof saved);if(memcmp(guard+page,page_expected,page)||memcmp(out,input,64)||saved!=k)return 22;"
                )
            };
            checks += &format!(
                "{{{setup}for(unsigned phase=0;phase<2;phase++){{initialize(input,{inputbytes},{from},{to},{kind},phase);for(unsigned p=0;p<5;p++){{{c_mask} k=masks[p],saved;unsigned char*base=p==0?(void*)1:p==1?guard+2*page-{element}:p==2?guard+page-({lanes}-1)*{element}:guard+2*page-{capacity};memset(guard+page,0xa5,page);memset(page_expected,0xa5,page);memset(out,0xa5,sizeof out);{operation}}}}}"
            );
            let opcode = recipe.id.x86_avx512_integer_convert_opcode().unwrap();
            if (recipe.load && matches!(opcode, "vpmovsxbd" | "vpmovsxwd" | "vpmovsxdq"))
                || (recipe.store && matches!(opcode, "vpmovdb" | "vpmovdw" | "vpmovqd"))
            {
                let call = if recipe.load {
                    format!("{name}(merge,guard+2*page-{element},out,3);")
                } else {
                    format!("{name}(input,guard+2*page-{element},out,3);")
                };
                checks += &format!(
                    "pid_t child=fork();if(child<0)return 23;if(!child){{{call}_exit(99);}}int status;if(waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 24;"
                );
            }
            checks += "}\n";
        }
    }
    result += &format!(
        r#"
int main(void){{
#ifdef __linux__
 if(prctl(PR_SET_DUMPABLE,0))return 1;
#endif
 long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0),*page_expected=malloc(page);if(guard==MAP_FAILED||!page_expected||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;
 {checks}
 return 0;
}}
"#
    );
    result
}

#[test]
fn foundation_integer_conversion_metadata_and_features() {
    let recipes = recipes();
    assert_eq!(recipes.len(), 50);
    for recipe in &recipes {
        let id = recipe.id;
        let instruction = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(id.info().pure, !recipe.load && !recipe.store);
        assert_eq!(instruction.may_trap(), recipe.load || recipe.store);
        assert_eq!(
            instruction.memory_effects(),
            if recipe.load {
                MemoryEffects::Read
            } else if recipe.store {
                MemoryEffects::Write
            } else {
                MemoryEffects::None
            }
        );
    }
    let module = keel_ir::parse(&source(&recipes)).unwrap();
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
        keel_codegen::compile_program_with_pipeline(
            &module,
            keel_ir::Target::X86_64LinuxGnu,
            &pipeline
        )
        .is_err()
    );
    assert!(keel_codegen::compile(&module, keel_ir::Target::Aarch64LinuxGnu).is_err());
}

#[test]
fn foundation_integer_conversion_scalar_system_and_guard_oracles() {
    let recipes = recipes();
    let mut module = keel_ir::parse(&source(&recipes)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[0],
        float_callee_saved: &[],
    };
    let directory = std::env::temp_dir().join(format!(
        "keel-avx512-integer-convert-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness(&recipes)).unwrap();
    let harness_object = directory.join("harness.o");
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args(["-mavx512f", "-mno-avx512dq", "-mno-avx512bw", "-O1", "-c"])
            .arg(directory.join("harness.c"))
            .arg("-o")
            .arg(&harness_object)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool_name, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("empty", &empty),
            ("one", &one),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
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
            let name = format!("{allocator:?}-{pool_name}");
            let path = directory.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if native {
                let executable = directory.join(&name);
                let output = Command::new("cc")
                    .arg(&harness_object)
                    .arg(path)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let status = Command::new(executable).status().unwrap();
                assert!(status.success(), "{name}: {status}");
            }
        }
    }
}

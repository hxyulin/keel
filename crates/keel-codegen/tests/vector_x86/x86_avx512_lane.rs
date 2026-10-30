//! Shift counts, bit-preserving broadcasts and native masked tuple boundaries.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use std::{fs, process::Command};

struct Case {
    id: Intrinsic,
    control: Option<i64>,
}
impl Case {
    fn name(&self) -> String {
        format!("{}_{}", self.id.info().name, self.control.unwrap_or(0))
    }
    fn load(&self) -> bool {
        self.id.is_x86_avx512_lane_load()
    }
    fn store(&self) -> bool {
        self.id.is_x86_avx512_lane_store()
    }
    fn kind(&self) -> u8 {
        if self.store() {
            6
        } else if self.load() {
            if self.id.info().params.len() == 5 {
                8
            } else {
                7
            }
        } else if let Some((index, _)) = self.id.x86_avx512_lane_control() {
            if index == 4 {
                5
            } else if self.id.info().returns[0].bytes().unwrap() < 64 {
                4
            } else {
                0
            }
        } else if self.id.info().params.len() == 4 {
            1
        } else if self.id.info().params[2].is_vector() {
            3
        } else {
            2
        }
    }
    fn target(&self) -> Type {
        if self.store() {
            self.id.info().params[1]
        } else {
            self.id.info().returns[0]
        }
    }
    fn a(&self) -> Option<Type> {
        if self.load() || self.store() {
            None
        } else {
            Some(self.id.info().params[2])
        }
    }
    fn b(&self) -> Option<Type> {
        match self.kind() {
            1 | 5 => Some(self.id.info().params[3]),
            8 => Some(self.id.info().params[3]),
            _ => None,
        }
    }
    fn mask(&self) -> Type {
        self.id.info().params[if self.load() || self.store() { 2 } else { 1 }]
    }
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_lane())
    {
        if let Some((_, max)) = id.x86_avx512_lane_control() {
            let controls = if max == 255 {
                let bits = id.info().returns[0]
                    .vector()
                    .unwrap()
                    .lane_type()
                    .bytes()
                    .unwrap()
                    * 8;
                vec![0, i64::from(bits - 1), i64::from(bits), 255]
            } else {
                vec![0, max]
            };
            for control in controls {
                cases.push(Case {
                    id,
                    control: Some(control),
                });
            }
        } else {
            cases.push(Case { id, control: None });
        }
    }
    cases
}
fn source(cases: &[Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let kind = case.kind();
        let mask = case.mask();
        let target = case.target();
        let a = case
            .a()
            .map(|ty| format!(" v6 = load.{ty} align 1 v1\n"))
            .unwrap_or_default();
        let b = case
            .b()
            .map(|ty| format!(" v7 = load.{ty} align 1 v2\n"))
            .unwrap_or_default();
        let control = case
            .control
            .map(|value| format!(" v8 = iconst.i32 {value}\n"))
            .unwrap_or_default();
        let args = match kind {
            0 | 4 => "v5,v4,v6,v8",
            1 => "v5,v4,v6,v7",
            2 | 3 => "v5,v4,v6",
            5 => "v5,v4,v6,v7,v8",
            6 => "v1,v5,v4,v8",
            7 => "v1,v5,v4",
            8 => "v1,v5,v4,v7,v8",
            _ => unreachable!(),
        };
        let call = if case.store() {
            format!(" intrinsic {}({args})\n", case.id.info().name)
        } else {
            format!(
                " v9 = intrinsic {}({args})\n store align 1 v9,v3\n",
                case.id.info().name
            )
        };
        let copy_a = if case.a().is_some() {
            " store align 1 v6,v12\n"
        } else {
            ""
        };
        let copy_b = if case.b().is_some() {
            " store align 1 v7,v14\n"
        } else {
            ""
        };
        source += &format!(
            "func @{}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:{mask}):\n v5 = load.{target} align 1 v0\n{a}{b}{control}{call} v10 = iconst.i64 64\n v11 = ptradd v3,v10\n store align 1 v5,v11\n v12 = ptradd v11,v10\n{copy_a} v13 = ptradd v12,v10\n v14 = ptradd v13,v10\n{copy_b} v15 = ptradd v14,v10\n store align 1 v4,v15\n return\n}}\n",
            case.name()
        );
    }
    source
}
fn reference(case: &Case) -> String {
    let reg = |bytes| match bytes {
        16 => "xmm",
        32 => "ymm",
        64 => "zmm",
        _ => unreachable!(),
    };
    let target = case.target();
    let targetbytes = target.bytes().unwrap();
    let opcode = case.id.x86_avx512_lane_opcode().unwrap();
    let control = case.control.unwrap_or(0);
    let mask = "%{%%k1%}";
    let mut asm = String::from("kmovw %k[k],%%k1\\n\\t ");
    let load = |pointer: &str, number: u8, bytes: u32| {
        format!("vmovups (%[{pointer}]),%%{}{number}\\n\\t ", reg(bytes))
    };
    if case.store() {
        asm += &load("merge", 1, 64);
        asm += &format!("{opcode} ${control},%%zmm1,(%[a]){mask}");
    } else {
        asm += &load("merge", 0, targetbytes);
        match case.kind() {
            0 | 4 => {
                asm += &load("a", 1, case.a().unwrap().bytes().unwrap());
                asm += &format!(
                    "{opcode} ${control},%%{}1,%%{}0{mask}",
                    reg(case.a().unwrap().bytes().unwrap()),
                    reg(targetbytes)
                );
            }
            1 => {
                asm += &load("a", 1, 64);
                asm += &load("b", 2, 16);
                asm += &format!("{opcode} %%xmm2,%%zmm1,%%zmm0{mask}");
            }
            2 | 3 | 7 => {
                asm += &format!("{opcode} (%[a]),%%zmm0{mask}");
            }
            5 => {
                asm += &load("a", 1, 64);
                asm += &load("b", 2, case.b().unwrap().bytes().unwrap());
                asm += &format!(
                    "{opcode} ${control},%%{}2,%%zmm1,%%zmm0{mask}",
                    reg(case.b().unwrap().bytes().unwrap())
                );
            }
            8 => {
                asm += &load("b", 1, 64);
                asm += &format!("{opcode} ${control},(%[a]),%%zmm1,%%zmm0{mask}");
            }
            _ => unreachable!(),
        }
        asm += &format!("\\n\\t vmovups %%{}0,(%[out])", reg(targetbytes));
    }
    format!(
        "static void ref_{}(void*merge,void*a,void*b,void*out,unsigned k){{__asm__ volatile(\"{asm}\"::[merge]\"r\"(merge),[a]\"r\"(a),[b]\"r\"(b),[out]\"r\"(out),[k]\"r\"(k):\"zmm0\",\"zmm1\",\"zmm2\",\"k1\",\"memory\");}}\n",
        case.name()
    )
}
fn harness(cases: &[Case]) -> String {
    let mut harness = String::from(
        r#"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
static uint64_t readbits(const void*p,unsigned bytes){uint64_t x=0;memcpy(&x,p,bytes);return x;}
static uint64_t shift(uint64_t value,uint64_t count,unsigned bits,unsigned op){uint64_t mask=bits==64?UINT64_MAX:UINT32_MAX;unsigned sign=(value>>(bits-1))&1;if(count>=bits)return op==2&&sign?mask:0;if(op==0)return(value<<count)&mask;if(op==1)return value>>count;if(!count)return value;return(value>>count)|(sign?mask<<(bits-count):0);}
typedef void(*operation)(void*,void*,void*,void*,unsigned);
static unsigned faults(operation fn,void*merge,void*a,void*b,void*out,unsigned k){pid_t child=fork();if(child<0)return 2;if(!child){fn(merge,a,b,out,k);_exit(0);}int status;if(waitpid(child,&status,0)!=child)return 2;if(WIFEXITED(status)&&WEXITSTATUS(status)==0)return 0;if(WIFSIGNALED(status)&&(WTERMSIG(status)==SIGSEGV||WTERMSIG(status)==SIGBUS))return 1;return 2;}
"#,
    );
    let mut checks = String::new();
    let mut edges = String::new();
    for case in cases {
        let name = case.name();
        let kind = case.kind();
        let target = case.target();
        let targetbytes = target.bytes().unwrap();
        let scalarbytes = target.vector().unwrap().lane_type().bytes().unwrap();
        let lanes = targetbytes / scalarbytes;
        let c_mask = if case.mask() == Type::I16 {
            "uint16_t"
        } else {
            "uint8_t"
        };
        let control = case.control.unwrap_or(0);
        let abytes = case.a().and_then(Type::bytes).unwrap_or(0);
        let bbytes = case.b().and_then(Type::bytes).unwrap_or(0);
        let opcode = case.id.x86_avx512_lane_opcode().unwrap();
        harness += &format!(
            "extern void {name}(void*,void*,void*,void*,{c_mask});\nstatic void call_{name}(void*m,void*a,void*b,void*out,unsigned k){{{name}(m,a,b,out,({c_mask})k);}}\n"
        );
        harness += &reference(case);
        let setup = format!(
            "unsigned char merge[64],a[64],b[64],out[512],expected[64],reference[64];for(unsigned i=0;i<64;i++){{merge[i]=(unsigned char)(0x81+i*37);a[i]=(unsigned char)(0xf1-i*13);b[i]=(unsigned char)(0x51+i*73);}}unsigned masks[]={{0,1,1u<<({lanes}-1),0xa55a&((1u<<{lanes})-1),(1u<<{lanes})-1}};"
        );
        let preservation = format!(
            "{c_mask} saved;memcpy(&saved,out+320,sizeof saved);if(memcmp(out+64,merge,{targetbytes})||saved!=({c_mask})k)return 12;{}{}",
            if abytes > 0 {
                format!("if(memcmp(out+128,a,{abytes}))return 13;")
            } else {
                String::new()
            },
            if bbytes > 0 {
                format!("if(memcmp(out+256,b,{bbytes}))return 14;")
            } else {
                String::new()
            }
        );
        if !case.load() && !case.store() {
            let expression = match kind {
                0 | 1 => {
                    let operation = if opcode.starts_with("vpsll") {
                        0
                    } else if opcode.starts_with("vpsrl") {
                        1
                    } else {
                        2
                    };
                    let count = if kind == 0 {
                        control.to_string()
                    } else {
                        "readbits(b,8)".to_owned()
                    };
                    format!(
                        "uint64_t value=shift(readbits(a+i*{scalarbytes},{scalarbytes}),{count},{}, {operation});memcpy(expected+i*{scalarbytes},&value,{scalarbytes});",
                        scalarbytes * 8
                    )
                }
                2 => format!("memcpy(expected+i*{scalarbytes},a,{scalarbytes});"),
                3 => {
                    format!("memcpy(expected+i*{scalarbytes},a+(i%4)*{scalarbytes},{scalarbytes});")
                }
                4 => format!(
                    "memcpy(expected+i*{scalarbytes},a+({control}*4+i)*{scalarbytes},{scalarbytes});"
                ),
                5 => format!(
                    "memcpy(expected+i*{scalarbytes},i>={control}*4&&i<({control}+1)*4?b+(i-{control}*4)*{scalarbytes}:a+i*{scalarbytes},{scalarbytes});"
                ),
                _ => unreachable!(),
            };
            let count_setup = if kind == 1 {
                format!(
                    "uint64_t counts[]={{0,1,{}, {},255,UINT64_MAX}};memcpy(b,&counts[countcase],8);uint64_t ignored=UINT64_MAX;memcpy(b+8,&ignored,8);",
                    scalarbytes * 8 - 1,
                    scalarbytes * 8
                )
            } else {
                String::new()
            };
            let countcases = if kind == 1 { 6 } else { 1 };
            checks += &format!(
                "{{{setup}for(unsigned countcase=0;countcase<{countcases};countcase++){{{count_setup}for(unsigned p=0;p<5;p++){{unsigned k=masks[p];memcpy(expected,merge,{targetbytes});for(unsigned i=0;i<{lanes};i++)if((k>>i)&1){{{expression}}}memset(out,0xa5,sizeof out);{name}(merge,a,b,out,({c_mask})k);ref_{name}(merge,a,b,reference,k);if(memcmp(out,expected,{targetbytes})||memcmp(out,reference,{targetbytes}))return 10;for(unsigned i={targetbytes};i<64;i++)if(out[i]!=0xa5)return 11;{preservation}}}}}}}\n"
            );
        } else if case.store() {
            let cap = case.id.x86_avx512_lane_memory_bytes().unwrap();
            checks += &format!(
                "{{{setup}unsigned storemasks[]={{0,1,8,5,15,0xf0}};for(unsigned p=0;p<6;p++){{unsigned k=storemasks[p];unsigned char*base=guard+2*page-{cap};memset(guard+page,0xa5,page);memset(page_expected,0xa5,page);for(unsigned i=0;i<4;i++)if((k>>i)&1)memcpy(page_expected+(base+i*{scalarbytes}-(guard+page)),merge+({control}*4+i)*{scalarbytes},{scalarbytes});memset(out,0xa5,sizeof out);{name}(merge,base,b,out,({c_mask})k);if(memcmp(guard+page,page_expected,page))return 20;{preservation}}}}}\n"
            );
            if control == case.id.x86_avx512_lane_control().unwrap().1 {
                edges += &format!(
                    "{{{setup}unsigned edge_masks[]={{0,0,0,0xf0,1,8,15}};unsigned char*edge_bases[]={{(void*)1,guard+2*page,guard+2*page+1,(void*)1,guard+2*page-{scalarbytes},guard+page-3*{scalarbytes},guard+2*page-{cap}}};unsigned bitset=0;for(unsigned edge=0;edge<7;edge++){{unsigned k=edge_masks[edge];unsigned char*base=edge_bases[edge];unsigned kfault=faults(call_{name},merge,base,b,out,k),rfault=faults(ref_{name},merge,base,b,reference,k);if(kfault>1||rfault>1||kfault!=rfault)return 33;bitset|=rfault<<edge;}}if(getenv(\"KEEL_LANE_REPORT\"))fprintf(stderr,\"{name} edge-fault-bits=%u\\n\",bitset);}}\n"
                );
            }
        } else {
            let cap = case.id.x86_avx512_lane_memory_bytes().unwrap();
            let copy = formatexpectedload(case, scalarbytes, control);
            checks += &format!(
                "{{{setup}for(unsigned p=0;p<5;p++){{unsigned k=masks[p];unsigned char*base=(k||{kind}==8)?guard+2*page-{cap}:(void*)1;memset(guard+page,0xa5,page);if(k)memcpy(base,a,{cap});memcpy(expected,merge,64);for(unsigned i=0;i<{lanes};i++)if((k>>i)&1){{{copy}}}memset(out,0xa5,sizeof out);{name}(merge,base,b,out,({c_mask})k);ref_{name}(merge,base,b,reference,k);if(memcmp(out,expected,64)||memcmp(out,reference,64))return 21;{preservation}}}}}\n"
            );

            if (kind == 7 && opcode.contains("x4"))
                || (kind == 8 && control == case.id.x86_avx512_lane_control().unwrap().1)
            {
                let start = control * 4;
                let (masks, bases, edge_count, zero_check) = if kind == 8 {
                    (
                        format!("{{0,0,1,1u<<{start},1u<<({start}+3),(1u<<{start})|1,0}}"),
                        format!(
                            "{{(void*)1,guard+2*page,(void*)1,guard+2*page-{scalarbytes},guard+page-3*{scalarbytes},guard+2*page-{scalarbytes},guard+2*page-{cap}}}"
                        ),
                        7,
                        "",
                    )
                } else {
                    (
                        "{0,0,1,1u<<4,1u<<3,1u<<1}".to_owned(),
                        format!(
                            "{{(void*)1,guard+2*page,(void*)1,guard+2*page-{scalarbytes},guard+page-3*{scalarbytes},guard+2*page-{scalarbytes}}}"
                        ),
                        6,
                        "if(edge<2&&rfault)return 31;",
                    )
                };
                edges += &format!(
                    "{{{setup}unsigned edge_masks[]={masks};unsigned char*edge_bases[]={bases};unsigned bitset=0;for(unsigned edge=0;edge<{edge_count};edge++){{unsigned k=edge_masks[edge];unsigned char*base=edge_bases[edge];memset(guard+page,0x93,page);unsigned kfault=faults(call_{name},merge,base,b,out,k),rfault=faults(ref_{name},merge,base,b,reference,k);if(kfault>1||rfault>1||kfault!=rfault)return 30;{zero_check}bitset|=rfault<<edge;if(!rfault){{call_{name}(merge,base,b,out,k);ref_{name}(merge,base,b,reference,k);if(memcmp(out,reference,64))return 32;}}}}if(getenv(\"KEEL_LANE_REPORT\"))fprintf(stderr,\"{name} edge-fault-bits=%u\\n\",bitset);}}\n"
                );
            }
        }
    }
    harness += &format!(
        r#"
int main(void){{
#ifdef __linux__
 if(prctl(PR_SET_DUMPABLE,0))return 1;
#endif
 long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0),*page_expected=malloc(page);if(guard==MAP_FAILED||!page_expected||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;
 {checks}
 {edges}
 return 0;
}}
"#
    );
    harness
}
fn formatexpectedload(case: &Case, width: u32, control: i64) -> String {
    if case.kind() == 8 {
        format!(
            "memcpy(expected+i*{width},i>={control}*4&&i<({control}+1)*4?a+(i-{control}*4)*{width}:b+i*{width},{width});"
        )
    } else if case.id.x86_avx512_lane_opcode().unwrap().contains("x4") {
        format!("memcpy(expected+i*{width},a+(i%4)*{width},{width});")
    } else {
        format!("memcpy(expected+i*{width},a,{width});")
    }
}

#[test]
fn foundation_lane_metadata_and_control_ranges() {
    let cases = cases();
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_lane())
        .collect();
    assert_eq!(ids.len(), 44);
    for id in ids {
        let instruction = InstData::Intrinsic { id, args: vec![] };
        let load = id.is_x86_avx512_lane_load();
        let store = id.is_x86_avx512_lane_store();
        assert_eq!(id.info().pure, !load && !store);
        assert_eq!(instruction.may_trap(), load || store);
        assert_eq!(
            instruction.memory_effects(),
            if load {
                MemoryEffects::Read
            } else if store {
                MemoryEffects::Write
            } else {
                MemoryEffects::None
            }
        );
    }
    let good = source(&cases);
    let module = keel_ir::parse(&good).unwrap();
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
    let dynamic = keel_ir::parse(&good.replace("v8 = iconst.i32 0", "v8 = load.i32 v0")).unwrap();
    assert!(keel_ir::verify_module(&dynamic).is_err());
    for invalid in [-1, 256] {
        let bad = keel_ir::parse(&good.replace("iconst.i32 0", &format!("iconst.i32 {invalid}")))
            .unwrap();
        assert!(keel_ir::verify_module(&bad).is_err());
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::compile_program_with_pipeline(
                &bad,
                keel_ir::Target::X86_64LinuxGnu,
                &pipeline
            )
            .is_err()
        );
    }
    for case in cases.iter().filter(|case| {
        case.control == Some(0) && case.id.x86_avx512_lane_control().unwrap().1 < 255
    }) {
        let invalid = Case {
            id: case.id,
            control: Some(case.id.x86_avx512_lane_control().unwrap().1 + 1),
        };
        let bad = keel_ir::parse(&source(&[invalid])).unwrap();
        assert!(keel_ir::verify_module(&bad).is_err());
    }
}

#[test]
fn foundation_lane_scalar_system_and_native_memory_edges() {
    let cases = cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
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
    let directory = std::env::temp_dir().join(format!("keel-avx512-lane-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness(&cases)).unwrap();
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

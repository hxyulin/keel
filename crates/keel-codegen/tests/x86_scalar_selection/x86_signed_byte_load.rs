//! Exact ordinary m8 -> signed i32 selection, allocation and native oracle.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::fs;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
const SOURCE: &str = "func @signed_load(ptr,ptr) {
block0(v0:ptr,v1:ptr):
 v2 = load.i8 align 1 v0
 v3 = sext.i32 v2
 store align 1 v3,v1
 return
}
";
fn compile(
    src: &str,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
    features: Features,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let selector = FeatureSelector {
        inner: &sel,
        features,
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &p).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(&m, pools, &p, &mut |part| {
        chunks.items.extend(part.items)
    })
    .unwrap();
    for part in [&program, &chunks] {
        for item in &part.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap(),
        obj
    );
    assert_eq!(
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
        obj
    );
    (text, obj)
}
fn fused(text: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with("movsx\t") && l.contains("byte ptr ["))
        .count()
}
#[test]
fn exact_shape_and_negative_gates() {
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY] {
                    let features = Features::for_level(level);
                    let text = compile(SOURCE, sel, alloc, pools, features).0;
                    assert_eq!(fused(&text), 1, "{text}");
                    for src in [
                        SOURCE.replace("load.i8 align", "load.i8 volatile align"),
                        SOURCE.replace("load.i8 align 1", "atomic_load.i8 acquire"),
                        SOURCE.replace("load.i8", "load.i16"),
                        SOURCE
                            .replace("load.i8", "load.i32")
                            .replace("sext.i32", "sext.i64"),
                        SOURCE.replace("sext.i32", "sext.i64"),
                        SOURCE.replace("sext.i32", "uext.i32"),
                        SOURCE.replace(" v3 =", " fence seq_cst\n v3 ="),
                        SOURCE.replace(" v3 =", " store v2,v1\n v3 ="),
                        SOURCE.replace(" v3 =", " v4 = load.i8 v1\n v3 ="),
                        SOURCE.replace(" v3 =", " call @clobber()\n v3 =").replacen(
                            "func",
                            "decl @clobber()\nfunc",
                            1,
                        ),
                        SOURCE.replace(" v3 =", " v4 = iconst.i64 2\n v5 = iadd v4,v4\n v3 ="),
                        SOURCE.replace(" return", " store v2,v1\n return"),
                        SOURCE.replace(
                            " v3 =",
                            " x86_rflags_region terminal \"save; reapply; normal\"()\n v3 =",
                        ),
                        SOURCE.replace(" v3 =", " v4 = iconst.i64 7\n v5 = sdiv v4,v4\n v3 ="),
                        SOURCE
                            .replace(" v3 =", " jump block1(v2)\nblock1(v4:i8):\n v3 =")
                            .replace("sext.i32 v2", "sext.i32 v4"),
                    ] {
                        assert_eq!(
                            fused(&compile(&src, sel, alloc, pools, features).0),
                            0,
                            "{src}"
                        );
                    }
                    let constants = SOURCE.replace(" v3 =", " v4 = iconst.i32 7\n v3 =");
                    assert_eq!(
                        fused(&compile(&constants, sel, alloc, pools, features).0),
                        1
                    );
                }
            }
        }
    }
}

#[test]
fn linux_oracle_all_bytes_edges_addresses_calls_and_spills() {
    let evidence = std::env::var_os("KEEL_SEXT8_EVIDENCE").map(std::path::PathBuf::from);
    let mut ir = String::from(
        "decl @clobber()\ndata @cell = align 1 { i8 0 }\nfunc @return_i32(ptr)->i32 {\nblock0(v0:ptr):\n v1 = load.i8 v0\n v2 = sext.i32 v1\n return v2\n}\nfunc @symbolic()->i32 {\nblock0:\n v0 = symbol_addr @cell\n v1 = load.i8 v0\n v2 = sext.i32 v1\n return v2\n}\n",
    );
    let mut c = String::from(
        r#"#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <signal.h>
#include <setjmp.h>
extern unsigned char cell;
extern int32_t return_i32(const void*);
extern int32_t symbolic(void);
static sigjmp_buf env;
static volatile uintptr_t fault_addr;
static void fault(int sig,siginfo_t *info,void *context){(void)sig;(void)context;fault_addr=(uintptr_t)info->si_addr;siglongjmp(env,1);}
static int traps(void (*fn)(const void*,void*,uint64_t),const void *p,void *out,const void *expected){if(sigsetjmp(env,1))return fault_addr==(uintptr_t)expected;fn(p,out,1);return 0;}
extern void store_before(const void*,void*,void*);
extern void store_after(const void*,void*,void*);
static int order_traps(void (*fn)(const void*,void*,void*),const void *p,void *out,void *dest,const void *expected){if(sigsetjmp(env,1))return fault_addr==(uintptr_t)expected;fn(p,out,dest);return 0;}
__attribute__((noinline)) void clobber(void){__asm__ volatile(
 "xor %%eax,%%eax\n\txor %%ecx,%%ecx\n\txor %%edx,%%edx\n\t"
 "xor %%esi,%%esi\n\txor %%edi,%%edi\n\txor %%r8d,%%r8d\n\t"
 "xor %%r9d,%%r9d\n\txor %%r10d,%%r10d\n\txor %%r11d,%%r11d"
 :::"memory","cc","rax","rcx","rdx","rsi","rdi","r8","r9","r10","r11");}
"#,
    );
    let mut checks = String::new();
    for mode in ["direct", "offset", "indexed", "live", "call", "uext"] {
        let address = match mode {
            "offset" => " v3 = iconst.i64 8\n v4 = ptradd v0,v3\n",
            "indexed" => " v3 = iconst.i64 1\n v9 = shl v2,v3\n v4 = ptradd v0,v9\n",
            _ => "",
        };
        let addr = if matches!(mode, "offset" | "indexed") {
            "v4"
        } else {
            "v0"
        };
        let call = if mode == "call" {
            " call @clobber()\n"
        } else {
            ""
        };
        let save = if mode == "live" || mode == "call" {
            " v20 = iconst.i64 8\n v21 = ptradd v1,v20\n store v0,v21\n"
        } else {
            ""
        };
        let extend = if mode == "uext" {
            " v7 = uext.i64 v6\n store align 1 v7,v1\n"
        } else {
            " store align 1 v6,v1\n"
        };
        ir += &format!(
            "func @{mode}(ptr,ptr,i64) {{\nblock0(v0:ptr,v1:ptr,v2:i64):\n{address} v5 = load.i8 align 1 {addr}\n v6 = sext.i32 v5\n{call}{extend}{save} return\n}}\n"
        );
        c += &format!("extern void {mode}(const void*,void*,uint64_t);\n");
        let bias = match mode {
            "offset" => 8,
            "indexed" => 2,
            _ => 0,
        };
        let width = if mode == "uext" { 8 } else { 4 };
        let saved = usize::from(mode == "live" || mode == "call");
        let live = if mode == "live" || mode == "call" {
            "const void *saved;memcpy(&saved,out+9,8);if(saved!=src)return 5;"
        } else {
            ""
        };
        checks += &format!(
            r#"
for(unsigned edge=0;edge<2;edge++)for(unsigned b=0;b<256;b++){{
 unsigned char *src=edge?end-1:start;*src=(unsigned char)b;
 unsigned char out[32];memset(out,0xa5,sizeof out);
 int32_t expected=b<128?(int32_t)b:(int32_t)b-256;
 uint64_t wide=(uint32_t)expected;
 cell=(unsigned char)b;if(symbolic()!=expected||return_i32(src)!=expected)return 10;
 unsigned before,after;__asm__ volatile("stmxcsr %0":"=m"(before));
 {mode}(src-{bias},out+1,1);
 __asm__ volatile("stmxcsr %0":"=m"(after));if(before!=after)return 8;
 if(memcmp(out+1,&wide,{width})||out[0]!=0xa5||out[31]!=0xa5)return 2;
 {live}
 for(unsigned k=1+{width};k<32;k++)if(!({saved}&&k>=9&&k<17)&&out[k]!=0xa5)return 9;
 unsigned char alias[32];memset(alias,0xa5,sizeof alias);alias[8]=(unsigned char)b;
 {mode}(alias+8-{bias},alias+8,1);if(memcmp(alias+8,&wide,{width}))return 3;
}}
for(unsigned edge=0;edge<2;edge++){{unsigned char out[32];memset(out,0xa5,sizeof out);
 if(!traps({mode},(edge?end:start-1)-{bias},out+1,edge?end:start-1))return 6;
 for(unsigned k=0;k<32;k++)if(out[k]!=0xa5)return 7;
}}
"#
        );
    }
    // Distinct invalid read/store addresses identify the first fault. A valid
    // preceding store must be visible, and a following store must remain absent.
    for (name, before, after) in [
        ("store_before", " store v3,v2\n", ""),
        ("store_after", "", " store v3,v2\n"),
    ] {
        ir += &format!(
            "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = iconst.i32 123\n{before} v4 = load.i8 v0\n v5 = sext.i32 v4\n{after} store v5,v1\n return\n}}\n"
        );
    }
    checks += r#"
unsigned char output[32],prior[4];memset(output,0xa5,sizeof output);memset(prior,0,sizeof prior);
if(!order_traps(store_before,start-1,output,end,end))return 11;
if(!order_traps(store_after,start-1,output,end,start-1))return 12;
if(!order_traps(store_before,start-1,output,prior,start-1))return 13;
int32_t marker=123;if(memcmp(prior,&marker,4))return 14;
memset(prior,0,sizeof prior);
if(!order_traps(store_after,start-1,output,prior,start-1))return 15;
for(unsigned k=0;k<4;k++)if(prior[k])return 16;
*start=255;if(!order_traps(store_after,start,output,end,end))return 17;
for(unsigned k=0;k<32;k++)if(output[k]!=0xa5)return 18;
"#;
    c += &format!(
        "int main(void){{struct sigaction action={{0}};action.sa_sigaction=fault;action.sa_flags=SA_SIGINFO;sigemptyset(&action.sa_mask);if(sigaction(SIGSEGV,&action,0))return 19;size_t n=(size_t)sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,3*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED||mprotect(p,n,PROT_NONE)||mprotect(p+2*n,n,PROT_NONE))return 1;unsigned char*start=p+n,*end=p+2*n;{checks}return 0;}}\n"
    );
    if let Some(dir) = &evidence {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.keel"), &ir).unwrap();
        fs::write(
            dir.join("run_native.py"),
            include_str!("../x86_widen/run_native.py"),
        )
        .unwrap();
    }
    for (level, name) in [
        (CpuLevel::V1, "v1"),
        (CpuLevel::V2, "v2"),
        (CpuLevel::V3, "v3"),
    ] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &EMPTY),
                ] {
                    let (text, obj) = compile(&ir, sel, alloc, pools, Features::for_level(level));
                    assert_eq!(fused(&text), 10, "{text}");
                    assert!(
                        !text
                            .lines()
                            .any(|l| l.trim_start().starts_with("movzx\t")
                                && l.contains("byte ptr [")),
                        "{text}"
                    );
                    if let Some(dir) = &evidence {
                        let name = format!("{name}-{sel:?}-{alloc:?}-{pool}");
                        fs::write(dir.join(format!("{name}.s")), text).unwrap();
                        fs::write(dir.join(format!("{name}.o")), obj).unwrap();
                        fs::write(dir.join(format!("{name}.c")), &c).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn debug_locations_and_constraints_remain_boundaries() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Constrained(usize, bool);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "signed-byte-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Full.select(f, t, folds);
            let i = f.layout_insts().nth(self.0).unwrap().1;
            let v = if self.1 {
                f.first_result(i)
            } else {
                let mut args = vec![];
                f.inst_data(i).for_each_operand(|v| args.push(v));
                args[0]
            };
            s.set_constraint(
                i,
                ValuePart::low(f.resolve(v)),
                if self.1 {
                    OperandKind::Def
                } else {
                    OperandKind::Use
                },
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..2 {
        for def in [false, true] {
            let m = keel_ir::parse(SOURCE).unwrap();
            let selector = Constrained(at, def);
            let p = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let text = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &p,
            )
            .unwrap();
            assert_eq!(fused(&text), 0, "{text}");
        }
    }
    for boundary in 0..7 {
        let mut m = keel_ir::parse(SOURCE).unwrap();
        m.files.push("byte.c".into());
        m.files.push("other.c".into());
        let f = &mut m.functions[0];
        f.debug_scopes.push(None);
        let pair: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
        for (at, i) in pair.into_iter().enumerate() {
            let loc = keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(usize::from(boundary == 5 && at == 1)),
                line: if boundary == 1 && at == 1 { 2 } else { 1 },
                col: if boundary == 2 && at == 1 { 2 } else { 1 },
                scope: (boundary == 6 && at == 1).then(|| keel_ir::DebugScope::new(0)),
            };
            if !(boundary == 3 && at == 0 || boundary == 4 && at == 1) {
                f.set_inst_loc(i, Some(loc));
            }
        }
        let text = keel_codegen::x86_64::compile(&m).unwrap();
        assert_eq!(fused(&text), usize::from(boundary == 0), "{text}");
    }
}

#[test]
fn symbolic_stack_address_and_resolved_input_alias() {
    let sources = [
        format!(
            "data @bytes = align 1 {{ i8 255 }}\n{}",
            SOURCE
                .replace(" v2 =", " v4 = symbol_addr @bytes\n v2 =")
                .replace("align 1 v0", "align 1 v4")
        ),
        SOURCE
            .replace(
                "block0(v0:ptr,v1:ptr):",
                "ss0 = stack_slot 1, align 1\nblock0(v0:ptr,v1:ptr):",
            )
            .replace(
                " v2 =",
                " v4 = stack_addr ss0\n v8 = iconst.i8 127\n store v8,v4\n v2 =",
            )
            .replace("align 1 v0", "align 1 v4"),
    ];
    for src in sources {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    assert_eq!(
                        fused(
                            &compile(&src, sel, alloc, pools, Features::for_level(CpuLevel::V1)).0
                        ),
                        1
                    );
                }
            }
        }
    }
    let mut m = keel_ir::parse(
        &SOURCE
            .replace(" v3 =", " v4 = iconst.i8 0\n v3 =")
            .replace("sext.i32 v2", "sext.i32 v4"),
    )
    .unwrap();
    let f = &mut m.functions[0];
    let is: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    f.replace_all_uses(f.first_result(is[1]), f.first_result(is[0]));
    // The alias itself has no uses, and the conversion still resolves to the load.
    assert_eq!(fused(&keel_codegen::x86_64::compile(&m).unwrap()), 1);
}

#[test]
fn encoding_matches_independent_clang() {
    use object::{Object, ObjectSection};
    let dir = std::path::PathBuf::from("/tmp")
        .join(format!("keel-sext8-encoding-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let asm = ".intel_syntax noprefix\n.text\nmovsx edi,byte ptr [rdi]\nmovsx eax,byte ptr [rax]\nmovsx r13d,byte ptr [r12+r9*8+127]\nmovsx r9d,byte ptr [rbp-256]\n";
    fs::write(dir.join("oracle.s"), asm).unwrap();
    let output = std::process::Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("oracle.s"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    fn text(bytes: &[u8]) -> Vec<u8> {
        object::File::parse(bytes)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    }
    let ours = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V1))
        .unwrap();
    assert_eq!(text(&ours), text(&fs::read(dir.join("oracle.o")).unwrap()));
}

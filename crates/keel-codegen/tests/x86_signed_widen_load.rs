//! Bounded SSE4.1 signed dword widening from the original exact m64 access.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use object::{Object, ObjectSection};
use std::{fs, process::Command};
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
 v2 = load.i64 align 1 v0
 v3 = vsplat.i64x2 v2
 v4 = bitcast.i32x4 v3
 v5 = swiden_low.i64x2 v4
 store align 1 v5,v1
 return
}
";
fn profiles() -> Vec<(&'static str, Features)> {
    let v1 = Features::for_level(CpuLevel::V1);
    let v2 = Features::for_level(CpuLevel::V2);
    let v3 = Features::for_level(CpuLevel::V3);
    vec![
        ("v1", v1),
        ("v2", v2),
        ("v3", v3),
        ("v1+sse41", Features { sse41: true, ..v1 }),
        ("v2-sse41", Features { sse41: false, ..v2 }),
        ("v3-sse41", Features { sse41: false, ..v3 }),
    ]
}
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
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
        obj
    );
    (text, obj)
}
fn fused(text: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with("pmovsxdq\txmm") && l.contains("qword ptr ["))
        .count()
}
#[test]
fn exact_chain_features_and_negative_gates() {
    for (_, features) in profiles() {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY] {
                    let text = compile(SOURCE, sel, alloc, pools, features).0;
                    assert_eq!(fused(&text), usize::from(features.sse41), "{text}");
                    if features.sse41 {
                        assert!(
                            !text.contains("pcmpgtd\t") && !text.contains("punpckldq\t"),
                            "{text}"
                        );
                    }
                    for src in [
                        SOURCE.replace("load.i64 align", "load.i64 volatile align"),
                        SOURCE.replace("swiden_low", "uwiden_low"),
                        SOURCE.replace("bitcast.i32x4", "bitcast.i16x8"),
                        SOURCE.replace("swiden_low", "swiden_high"),
                        SOURCE.replace(" v3 =", " fence seq_cst\n v3 ="),
                        SOURCE.replace(" v4 =", " fence seq_cst\n v4 ="),
                        SOURCE.replace(" v5 =", " fence seq_cst\n v5 ="),
                        SOURCE.replace(" return", " store v2,v1\n return"),
                        SOURCE.replace(" return", " store v3,v1\n return"),
                        SOURCE.replace(" return", " store v4,v1\n return"),
                    ] {
                        assert_eq!(
                            fused(&compile(&src, sel, alloc, pools, features).0),
                            0,
                            "{src}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn constraints_and_debug_boundaries_are_preserved() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Selector(usize);
    impl InstructionSelector for Selector {
        fn name(&self) -> &'static str {
            "low-load-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            let inst = f.layout_insts().nth(self.0 % 4).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(inst).for_each_operand(|v| args.push(v));
            s.set_constraint(
                inst,
                ValuePart::low(if self.0 < 4 {
                    f.resolve(args[0])
                } else {
                    f.first_result(inst)
                }),
                if self.0 < 4 {
                    OperandKind::Use
                } else {
                    OperandKind::Def
                },
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..8 {
        let m = keel_ir::parse(SOURCE).unwrap();
        let selector = Selector(at);
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let text =
            keel_codegen::x86_64::compile_with_pipeline(&m, &keel_codegen::x86_64::REGISTERS, &p)
                .unwrap();
        assert!(fused(&text) == 0, "{text}");
    }
    for at in 0..8 {
        let mut m = keel_ir::parse(SOURCE).unwrap();
        m.files.push("load.c".into());
        for f in &mut m.functions {
            let chain: Vec<_> = f.layout_insts().take(4).map(|(_, i)| i).collect();
            for (index, i) in chain.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: if index == at { 11 } else { 10 },
                        col: if at >= 4 && index == at - 4 { 2 } else { 1 },
                        scope: None,
                    }),
                );
            }
        }
        assert!(fused(&keel_codegen::x86_64::compile(&m).unwrap()) == 0);
    }
}

/// Export a portable ELF+C bundle; root runs it on Linux under the shared lock.
#[test]
fn linux_oracle_bundle_exact_footprint_indexed_live_alias_and_spills() {
    use std::fs;
    let evidence = std::env::var_os("KEEL_SIGNED_LOAD_EVIDENCE").map(std::path::PathBuf::from);
    let mut ir = String::new();
    let mut c = String::from(
        r#"#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <signal.h>
#include <setjmp.h>
static sigjmp_buf fault_env;
static volatile sig_atomic_t fault_seen;
static void on_fault(int sig) {fault_seen=sig;siglongjmp(fault_env,1);}
"#,
    );
    let mut checks = String::new();
    for from in [32] {
        let to = 2 * from;
        for (op, scalar) in [("swiden_low", format!("int{from}_t"))] {
            for mode in ["direct", "offset", "indexed", "live"] {
                let name = format!("{op}{from}_{mode}");
                let address = match mode {
                    "offset" => " v3 = iconst.i64 8\n v4 = ptradd v0,v3\n",
                    "indexed" => " v3 = iconst.i64 3\n v9 = shl v2,v3\n v4 = ptradd v0,v9\n",
                    _ => "",
                };
                let live = if mode == "live" {
                    " v20 = load.i64x2 align 1 v1\n"
                } else {
                    ""
                };
                let save = if mode == "live" {
                    " v21 = iconst.i64 16\n v22 = ptradd v1,v21\n store align 1 v20,v22\n v23 = iconst.i64 32\n v24 = ptradd v1,v23\n store align 1 v0,v24\n"
                } else {
                    ""
                };
                let addr = if matches!(mode, "offset" | "indexed") {
                    "v4"
                } else {
                    "v0"
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,i64) {{\nblock0(v0: ptr,v1: ptr,v2: i64):\n{address}{live} v5 = load.i64 align 1 {addr}\n v6 = vsplat.i64x2 v5\n v7 = bitcast.i{from}x{} v6\n v8 = {op}.i{to}x{} v7\n store align 1 v8,v1\n{save} return\n}}\n",
                    128 / from,
                    128 / to
                );
                c += &format!("extern void {name}(const void*,void*,uint64_t);\n");
                let bias = if matches!(mode, "offset" | "indexed") {
                    8
                } else {
                    0
                };
                let live_check = if mode == "live" {
                    "if(memcmp(out+17,before,16))return 5;const void *saved;memcpy(&saved,out+33,sizeof saved);if(saved!=src)return 6;for(unsigned k=41;k<50;k++)if(out[k]!=0xa5)return 11;"
                } else {
                    "for(unsigned k=17;k<50;k++)if(out[k]!=0xa5)return 11;"
                };
                checks += &format!(
                    r#"
 for(unsigned pattern=0;pattern<1024;pattern++)for(unsigned offset=0;offset<16;offset++)for(unsigned edge=0;edge<2;edge++){{
 unsigned char *src=edge?end-8-offset:start+offset,original[8],out[50],before[16],expected[16];
 uint32_t bits[2];uint32_t extrema[]={{0,1,0x7fffffffU,0x80000000U,0xffffffffU}};
 for(unsigned i=0;i<2;i++){{state^=state<<13;state^=state>>17;state^=state<<5;bits[i]=pattern<25?extrema[(pattern/(i?5:1))%5]:state;}}memcpy(src,bits,8);
 memcpy(original,src,8);memset(out,0xa5,sizeof out);memcpy(before,out+1,16);
 for(unsigned i=0;i<{lanes};i++){{{scalar} v;memcpy(&v,original+i*sizeof v,sizeof v);uint{to}_t e=(uint{to}_t)v;memcpy(expected+i*sizeof e,&e,sizeof e);}}
 unsigned mxcsr_before,mxcsr_after;__asm__ volatile("stmxcsr %0":"=m"(mxcsr_before));
 {name}(src-{bias},out+1,1);
 __asm__ volatile("stmxcsr %0":"=m"(mxcsr_after));if(mxcsr_before!=mxcsr_after)return 7;
 if(out[0]!=0xa5||out[49]!=0xa5||memcmp(out+1,expected,16))return 2;
 {live_check}
 unsigned char alias[64];memset(alias,0xa5,sizeof alias);memcpy(alias+16,original,8);
 {name}(alias+16-{bias},alias+16,1);if(memcmp(alias+16,expected,16))return 3;
 }}
 for(unsigned edge=0;edge<2;edge++){{
 unsigned char out[64];memset(out,0xa5,sizeof out);fault_seen=0;
 if(!sigsetjmp(fault_env,1)){{{name}((edge?end-7:start-1)-{bias},out+1,1);return 8;}}
 if(fault_seen!=SIGSEGV)return 9;for(unsigned k=0;k<64;k++)if(out[k]!=0xa5)return 10;
 }}
"#,
                    lanes = 128 / to
                );
            }
        }
    }
    c += &format!(
        "int main(void){{signal(SIGSEGV,on_fault);size_t n=(size_t)sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,3*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED||mprotect(p,n,PROT_NONE)||mprotect(p+2*n,n,PROT_NONE))return 1;unsigned char*start=p+n,*end=p+2*n;uint32_t state=0x12345678U;{checks}munmap(p,3*n);puts(\"passed 4 signed m64 shapes: guard/unaligned/alias/live/indexed/spill\");return 0;}}\n"
    );
    if let Some(dir) = &evidence {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.keel"), &ir).unwrap();
        fs::write(
            dir.join("run_native.py"),
            include_str!("x86_widen/run_native.py"),
        )
        .unwrap();
    }

    for (profile, features) in profiles() {
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
                    let (text, obj) = compile(&ir, sel, alloc, pools, features);
                    assert_eq!(fused(&text), if features.sse41 { 4 } else { 0 }, "{text}");
                    if let Some(dir) = &evidence {
                        let name = format!("{profile}-{sel:?}-{alloc:?}-{pool}");
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
fn memory_encoding_matches_clang() {
    let dir = std::env::temp_dir().join(format!("keel-pmovsxdq-encoding-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let asm = ".intel_syntax noprefix\n.text\npmovsxdq xmm0,qword ptr [rdi]\npmovsxdq xmm15,qword ptr [r12+r9*8+127]\npmovsxdq xmm13,qword ptr [rbp-256]\n";
    fs::write(dir.join("oracle.s"), asm).unwrap();
    let output = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("oracle.s"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let ours = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V2))
        .unwrap();
    fn text(obj: &[u8]) -> Vec<u8> {
        object::File::parse(obj)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    }
    assert_eq!(text(&ours), text(&fs::read(dir.join("oracle.o")).unwrap()));
    fs::remove_dir_all(dir).unwrap();
}

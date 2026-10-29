//! Native AVX2 signed dword widening from the original exact m128 access.
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
 v2 = load.i32x4 align 1 v0
 v5 = swiden_low.i64x4 v2
 store align 1 v5,v1
 return
}
";
fn profiles() -> Vec<(&'static str, Features)> {
    let v1 = Features::for_level(CpuLevel::V1);
    let v2 = Features::for_level(CpuLevel::V2);
    let v3 = Features::for_level(CpuLevel::V3);
    vec![
        ("v3", v3),
        (
            "v1+avx",
            Features {
                avx: true,
                xsave: true,
                osxsave: true,
                ..v1
            },
        ),
        (
            "v2+avx",
            Features {
                avx: true,
                xsave: true,
                osxsave: true,
                ..v2
            },
        ),
        ("v3-avx2", Features { avx2: false, ..v3 }),
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
        .filter(|l| l.trim_start().starts_with("vpmovsxdq\tymm") && l.contains("xmmword ptr ["))
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
                    assert_eq!(
                        fused(&text),
                        usize::from(features.avx && features.avx2),
                        "{text}"
                    );
                    if features.avx && features.avx2 {
                        assert!(
                            !text.contains("pcmpgtd\t") && !text.contains("punpckldq\t"),
                            "{text}"
                        );
                    }
                    for src in [
                        SOURCE.replace("load.i32x4 align", "load.i32x4 volatile align"),
                        SOURCE.replace("swiden_low", "uwiden_low"),
                        SOURCE.replace("swiden_low", "swiden_high"),
                        SOURCE.replace("load.i32x4", "load.i16x8"),
                        SOURCE.replace(" v5 =", " fence seq_cst\n v5 ="),
                        SOURCE.replace(" return", " store v2,v1\n return"),
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
fn optimized_and_unused_results_preserve_the_load_contract() {
    for source in [
        SOURCE.to_string(),
        SOURCE.replace(" store align 1 v5,v1\n", ""),
    ] {
        for optimize in [false, true] {
            let mut m = keel_ir::parse(&source).unwrap();
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for (_, features) in profiles() {
                for sel in [
                    SelectionTier::Baseline,
                    SelectionTier::Trees,
                    SelectionTier::Full,
                ] {
                    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                            // Dead-code removal may delete an unused ordinary load;
                            // the unoptimized dead chain must still compile safely.
                            let text = compile(&m.to_string(), sel, alloc, pools, features).0;
                            if source == SOURCE {
                                assert_eq!(
                                    fused(&text),
                                    usize::from(features.avx && features.avx2),
                                    "{text}"
                                );
                            }
                        }
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
            let inst = f.layout_insts().nth(self.0 % 2).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(inst).for_each_operand(|v| args.push(v));
            s.set_constraint(
                inst,
                ValuePart::low(if self.0 < 2 {
                    f.resolve(args[0])
                } else {
                    f.first_result(inst)
                }),
                if self.0 < 2 {
                    OperandKind::Use
                } else {
                    OperandKind::Def
                },
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..4 {
        let m = keel_ir::parse(SOURCE).unwrap();
        let inner = Selector(at);
        let selector = FeatureSelector {
            inner: &inner,
            features: Features::for_level(CpuLevel::V3),
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let text =
            keel_codegen::x86_64::compile_with_pipeline(&m, &keel_codegen::x86_64::REGISTERS, &p)
                .unwrap();
        assert!(fused(&text) == 0, "{text}");
    }
    for at in 0..4 {
        let mut m = keel_ir::parse(SOURCE).unwrap();
        m.files.push("load.c".into());
        for f in &mut m.functions {
            let chain: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
            for (index, i) in chain.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: if index == at { 11 } else { 10 },
                        col: if at >= 2 && index == at - 2 { 2 } else { 1 },
                        scope: None,
                    }),
                );
            }
        }
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::for_level(CpuLevel::V3),
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert_eq!(
            fused(
                &keel_codegen::x86_64::compile_with_pipeline(
                    &m,
                    &keel_codegen::x86_64::REGISTERS,
                    &p
                )
                .unwrap()
            ),
            0
        );
    }
}

/// Export a portable ELF+C bundle; root runs it on Linux under the shared lock.
#[test]
fn linux_oracle_bundle_exact_footprint_indexed_live_alias_and_spills() {
    use std::fs;
    let evidence = std::env::var_os("KEEL_WIDE_LOAD_EVIDENCE").map(std::path::PathBuf::from);
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
            for mode in ["direct", "offset", "indexed", "live", "sum"] {
                let name = format!("{op}{from}_{mode}");
                let address = match mode {
                    "offset" => " v3 = iconst.i64 8\n v4 = ptradd v0,v3\n",
                    "indexed" => " v3 = iconst.i64 3\n v9 = shl v2,v3\n v4 = ptradd v0,v9\n",
                    _ => "",
                };
                let live = if matches!(mode, "live" | "sum") {
                    " v20 = load.i64x4 align 1 v1\n"
                } else {
                    ""
                };
                let save = if mode == "live" {
                    " v21 = iconst.i64 32\n v22 = ptradd v1,v21\n store align 1 v20,v22\n v23 = iconst.i64 64\n v24 = ptradd v1,v23\n store align 1 v0,v24\n"
                } else {
                    ""
                };
                let addr = if matches!(mode, "offset" | "indexed") {
                    "v4"
                } else {
                    "v0"
                };
                let (add, result) = if mode == "sum" {
                    (" v26 = viadd v20,v8\n", "v26")
                } else {
                    ("", "v8")
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,i64) {{\nblock0(v0: ptr,v1: ptr,v2: i64):\n{address}{live} v5 = load.i32x4 align 1 {addr}\n v8 = {op}.i64x4 v5\n{add} store align 1 {result},v1\n{save} return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,void*,uint64_t);\n");
                let bias = if matches!(mode, "offset" | "indexed") {
                    8
                } else {
                    0
                };
                let live_check = if mode == "live" {
                    "if(memcmp(out+33,before,32))return 5;const void *saved;memcpy(&saved,out+65,sizeof saved);if(saved!=src)return 6;for(unsigned k=73;k<90;k++)if(out[k]!=0xa5)return 11;"
                } else {
                    "for(unsigned k=33;k<90;k++)if(out[k]!=0xa5)return 11;"
                };
                checks += &format!(
                    r#"
 for(unsigned pattern=0;pattern<1024;pattern++)for(unsigned offset=0;offset<16;offset++)for(unsigned edge=0;edge<2;edge++){{
 unsigned char *src=edge?end-16-offset:start+offset,original[16],out[90],before[32],expected[32];
 uint32_t bits[4];uint32_t extrema[]={{0,1,0x7fffffffU,0x80000000U,0xffffffffU}};
 for(unsigned i=0;i<4;i++){{state^=state<<13;state^=state>>17;state^=state<<5;bits[i]=pattern<25?extrema[(pattern/(i?5:1))%5]:state;}}memcpy(src,bits,16);
 memcpy(original,src,16);memset(out,0xa5,sizeof out);if({summing})memset(out+1,0xff,32);memcpy(before,out+1,32);
 for(unsigned i=0;i<{lanes};i++){{{scalar} v;memcpy(&v,original+i*sizeof v,sizeof v);uint{to}_t e=(uint{to}_t)v;if({summing})e+=UINT64_MAX;memcpy(expected+i*sizeof e,&e,sizeof e);}}
 unsigned mxcsr_before,mxcsr_after;__asm__ volatile("stmxcsr %0":"=m"(mxcsr_before));
 {name}(src-{bias},out+1,1);
 __asm__ volatile("stmxcsr %0":"=m"(mxcsr_after));if(mxcsr_before!=mxcsr_after)return 7;
 if(out[0]!=0xa5||out[89]!=0xa5||memcmp(out+1,expected,32))return 2;
 {live_check}
 if(!{summing}){{unsigned char alias[96];memset(alias,0xa5,sizeof alias);memcpy(alias+16,original,16);
 {name}(alias+16-{bias},alias+16,1);if(memcmp(alias+16,expected,32))return 3;}}
 }}
 for(unsigned edge=0;edge<2;edge++){{
 unsigned char out[64];memset(out,0xa5,sizeof out);fault_seen=0;
 if(!sigsetjmp(fault_env,1)){{{name}((edge?end-15:start-1)-{bias},out+1,1);return 8;}}
 if(fault_seen!=SIGSEGV)return 9;for(unsigned k=0;k<64;k++)if(out[k]!=0xa5)return 10;
 }}
"#,
                    summing = usize::from(mode == "sum"),
                    lanes = 256 / to
                );
            }
        }
    }
    c += &format!(
        "int main(void){{if(!__builtin_cpu_supports(\"avx2\"))return 77;signal(SIGSEGV,on_fault);size_t n=(size_t)sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,3*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED||mprotect(p,n,PROT_NONE)||mprotect(p+2*n,n,PROT_NONE))return 1;unsigned char*start=p+n,*end=p+2*n;uint32_t state=0x12345678U;{checks}munmap(p,3*n);puts(\"passed 5 signed m128 shapes: guard/unaligned/alias/live/indexed/spill\");return 0;}}\n"
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
                    assert_eq!(
                        fused(&text),
                        if features.avx && features.avx2 { 5 } else { 0 },
                        "{text}"
                    );
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
    let dir = std::env::temp_dir().join(format!(
        "keel-vpmovsxdq-wide-encoding-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let asm = ".intel_syntax noprefix\n.text\nvpmovsxdq ymm0,xmmword ptr [rdi]\nvpmovsxdq ymm15,xmmword ptr [r12+r9*8+127]\nvpmovsxdq ymm13,xmmword ptr [rbp-256]\n";
    fs::write(dir.join("oracle.s"), asm).unwrap();
    let output = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("oracle.s"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let ours = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V3))
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

#[test]
fn profiles_without_avx_keep_the_existing_wide_rejection() {
    let m = keel_ir::parse(SOURCE).unwrap();
    for level in [CpuLevel::V1, CpuLevel::V2] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::for_level(level),
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_with_pipeline(&m, &keel_codegen::x86_64::REGISTERS, &p)
                .is_err()
        );
    }
}

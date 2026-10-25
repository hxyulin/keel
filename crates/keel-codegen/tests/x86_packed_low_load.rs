//! Exact low-half vector loads retain the original access and selection contracts.
use keel_asm::x86::{assemble_object_with_features, assemble_program_object_with_features};
use keel_asm_ir::x86::Features;
use keel_codegen::x86_features::FeatureSelector;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::new();
    for from in [8, 16, 32] {
        let to = from * 2;
        for op in ["swiden_low", "uwiden_low"] {
            s += &format!(
                "func @{op}{from}(ptr,ptr) {{
block0(v0: ptr,v1: ptr):
 v2 = iconst.i64 8
 v3 = ptradd v0,v2
 v4 = load.i64 v3
 v5 = vsplat.i64x2 v4
 v6 = bitcast.i{from}x{lanes} v5
 v7 = {op}.i{to}x{wide_lanes} v6
 store v7,v1
 return
}}\n",
                lanes = 128 / from,
                wide_lanes = 128 / to
            );
        }
    }
    s
}
fn compile(src: &str, sel: SelectionTier, alloc: AllocationTier, spill: bool) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    // This oracle specifically exercises the SSE4.1 PMOVSXDQ load cover.
    let features = Features::parse("sse41").unwrap();
    let selector = FeatureSelector {
        inner: &sel,
        features,
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &alloc,
    };
    let pools = if spill {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &p).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
    let obj = assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(assemble_object_with_features(&text, features).unwrap(), obj);
    (text, obj)
}
fn direct_loads(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            (line.trim_start().starts_with("movq\txmm")
                || line.trim_start().starts_with("pmovsxdq\txmm"))
                && line.contains("qword ptr [")
        })
        .count()
}
#[test]
fn exact_loads_and_shared_volatile_rejections() {
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                let text = compile(&source(), sel, alloc, spill).0;
                assert_eq!(direct_loads(&text), 6, "{text}");
                for from in [8, 16, 32] {
                    for op in ["swiden_low", "uwiden_low"] {
                        let label = format!("\"{op}{from}\":\n");
                        let body = text.split_once(&label).unwrap().1;
                        let body = body.split_once(".cfi_endproc").unwrap().0;
                        let signed_dwords = op == "swiden_low" && from == 32;
                        assert_eq!(
                            body.lines()
                                .filter(|l| l.trim_start().starts_with("pmovsxdq\t"))
                                .count(),
                            usize::from(signed_dwords),
                            "{body}"
                        );
                        if !signed_dwords {
                            assert!(
                                body.lines().any(|l| l.trim_start().starts_with("movq\txmm")
                                    && l.contains("qword ptr [")),
                                "{body}"
                            );
                        }
                    }
                }
                assert!(!text.contains("punpcklqdq\t"), "{text}");
                // Larger widening ratios observe fewer than eight bytes and
                // deliberately retain their existing lowering.
                let skipped = source()
                    .replace("widen_low.i32x4", "widen_low.i64x2")
                    .replace("widen_low.i16x8", "widen_low.i32x4");
                assert_eq!(direct_loads(&compile(&skipped, sel, alloc, spill).0), 2);

                for src in [
                    source().replace("load.i64 ", "load.i64 volatile "),
                    source().replace(" store v7,v1", " store v5,v1\n store v7,v1"),
                    source().replace(" store v7,v1", " store v4,v1\n store v7,v1"),
                    source().replace(" store v7,v1", " store v6,v1\n store v7,v1"),
                    source().replace(" v6 = bitcast", " fence seq_cst\n v6 = bitcast"),
                    source().replace(" v7 = ", " fence seq_cst\n v7 = "),
                    source().replace(" v5 = vsplat", " store v1,v1\n v5 = vsplat"),
                ] {
                    assert!(direct_loads(&compile(&src, sel, alloc, spill).0) == 0);
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
            let inst = f.layout_insts().nth(2 + self.0).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(inst).for_each_operand(|v| args.push(v));
            s.set_constraint(
                inst,
                ValuePart::low(f.resolve(args[0])),
                OperandKind::Use,
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..4 {
        let m = keel_ir::parse(&source()).unwrap();
        let selector = Selector(at);
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let text =
            keel_codegen::x86_64::compile_with_pipeline(&m, &keel_codegen::x86_64::REGISTERS, &p)
                .unwrap();
        assert!(direct_loads(&text) == 0, "{text}");
    }
    for at in 0..4 {
        let mut m = keel_ir::parse(&source()).unwrap();
        m.files.push("load.c".into());
        for f in &mut m.functions {
            let chain: Vec<_> = f.layout_insts().skip(2).take(4).map(|(_, i)| i).collect();
            for (index, i) in chain.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: if index == at { 11 } else { 10 },
                        col: 1,
                        scope: None,
                    }),
                );
            }
        }
        assert!(direct_loads(&keel_codegen::x86_64::compile(&m).unwrap()) == 0);
    }
}

/// Export a portable ELF+C bundle; root runs it on Linux under the shared lock.
#[test]
fn linux_oracle_bundle_exact_footprint_indexed_live_alias_and_spills() {
    use std::fs;
    let evidence = std::env::var_os("KEEL_LOW64_EVIDENCE").map(std::path::PathBuf::from);
    let mut ir = String::new();
    let mut c = String::from(
        r#"#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
typedef void (*Fn)(const void*,void*,uint64_t);
"#,
    );
    let mut checks = String::new();
    for from in [8, 16, 32] {
        let to = 2 * from;
        for (op, scalar) in [
            ("swiden_low", format!("int{from}_t")),
            ("uwiden_low", format!("uint{from}_t")),
        ] {
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
                    " v21 = iconst.i64 16\n v22 = ptradd v1,v21\n store align 1 v20,v22\n"
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
                    "if(memcmp(out+17,before,16))return 5;"
                } else {
                    ""
                };
                checks += &format!(
                    r#"
 for(unsigned pattern=0;pattern<257;pattern++)for(unsigned offset=0;offset<3;offset++){{
 unsigned char *src=end-8-offset,original[8],out[50],before[16],expected[16];
 for(unsigned i=0;i<8;i++)src[i]=(unsigned char)(pattern+i*31);
 memcpy(original,src,8);memset(out,0xa5,sizeof out);memcpy(before,out+1,16);
 for(unsigned i=0;i<{lanes};i++){{{scalar} v;memcpy(&v,original+i*sizeof v,sizeof v);uint{to}_t e=(uint{to}_t)v;memcpy(expected+i*sizeof e,&e,sizeof e);}}
 {name}(src-{bias},out+1,1);
 if(out[0]!=0xa5||out[49]!=0xa5||memcmp(out+1,expected,16))return 2;
 {live_check}
 unsigned char alias[64];memset(alias,0xa5,sizeof alias);memcpy(alias+16,original,8);
 {name}(alias+16-{bias},alias+16,1);if(memcmp(alias+16,expected,16))return 3;
 }}
"#,
                    lanes = 128 / to
                );
            }
        }
    }
    c += &format!(
        "int main(void){{size_t n=(size_t)sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED||mprotect(p+n,n,PROT_NONE))return 1;unsigned char*end=p+n;{checks}munmap(p,2*n);puts(\"passed 24 exact-low64 shapes: guard/unaligned/alias/live/indexed/spill\");return 0;}}\n"
    );
    if let Some(dir) = &evidence {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.keel"), &ir).unwrap();
        fs::write(
            dir.join("run_native.py"),
            include_str!("x86_widen/run_native.py").replace(
                "target/packed-widen-20261021/lock.jsonl",
                "target/clang-gap-round2-20261021/lock.jsonl",
            ),
        )
        .unwrap();
    }
    if let Some(dir) = &evidence {
        let control = ir.replace("load.i64 align", "load.i64 volatile align");
        let (text, obj) = compile(&control, SelectionTier::Full, AllocationTier::Full, false);
        assert_eq!(direct_loads(&text), 0);
        fs::write(dir.join("control.s"), text).unwrap();
        fs::write(dir.join("control.o"), obj).unwrap();
        fs::write(dir.join("control.c"), &c).unwrap();
    }
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                let (text, obj) = compile(&ir, sel, alloc, spill);
                assert_eq!(direct_loads(&text), 24, "{sel:?}/{alloc:?}/{spill}: {text}");
                if let Some(dir) = &evidence {
                    let name = format!("{sel:?}-{alloc:?}-spill{spill}");
                    fs::write(dir.join(format!("{name}.s")), text).unwrap();
                    fs::write(dir.join(format!("{name}.o")), obj).unwrap();
                    fs::write(dir.join(format!("{name}.c")), &c).unwrap();
                }
            }
        }
    }
}

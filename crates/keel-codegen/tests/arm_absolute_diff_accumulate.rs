//! Exact seven-node SABAL selection; manual IR independent of optimizer work.
#[path = "arm_narrow/oracle.rs"]
mod support;
use keel_codegen::{
    SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
const SOURCE: &str = "func @sabal(i64x2,i32x4,i32x4) -> i64x2 {
block0(v0:i64x2,v1:i32x4,v2:i32x4):
 v3 = iconst.i64 0
 v4 = vsplat.i64x2 v3
 v5 = swiden_low.i64x2 v1
 v6 = swiden_low.i64x2 v2
 v7 = visub v5,v6
 v8 = vicmp.slt v7,v4
 v9 = vineg v7
 v10 = vselect v8,v9,v7
 v11 = viadd v0,v10
 return v11
}
";
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[],
};

fn oracle_c(names: &[String], same: bool) -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\ntypedef uint64_t W __attribute__((vector_size(16)));\ntypedef int32_t N __attribute__((vector_size(16)));\n",
    );
    for n in names {
        c += &format!("extern W {n}(W,N,N);\n");
    }
    c += "int main(void){const int32_t e[]={INT32_MIN,INT32_MAX,-1,0,1,1234567};const uint64_t a[]={0,1,UINT64_MAX,UINT64_MAX-4294967294ULL,0x8000000000000000ULL}; for(unsigned p=0;p<216;p++)for(unsigned k=0;k<5;k++){N x,y;W acc={a[k],a[(k+1)%5]}, expected;for(unsigned l=0;l<4;l++){x[l]=e[(p+l)%6];y[l]=e[(p/6+2*l)%6];}";
    if same {
        c += "y=x;";
    }
    c += "for(unsigned l=0;l<2;l++){int64_t d=(int64_t)x[l]-(int64_t)y[l];uint64_t mag=(uint64_t)(d<0?-d:d);expected[l]=acc[l]+mag;}";
    for n in names {
        c += &format!(
            "{{W got={n}(acc,x,y);if(memcmp(&got,&expected,16)){{puts(\"{n}\");return 1;}}}}"
        );
    }
    c += "}return 0;}\n";
    c
}
#[test]
fn exact_signed_extrema_wrapping_and_both_routes_all_pools() {
    let mut oracle = support::Oracle::new("sabal");
    let c = oracle_c(&["sabal".into()], false);
    for commute in [false, true] {
        let src = if commute {
            SOURCE.replace("viadd v0,v10", "viadd v10,v0")
        } else {
            SOURCE.to_owned()
        };
        let m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::aarch64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &support::EMPTY),
                ] {
                    let (text, obj) = support::compile(&m, &sel, &alloc, pools);
                    assert_eq!(text.matches("sabal\t").count(), 1, "{text}");
                    assert!(
                        !text.contains("sshll\t") && !text.contains("abs\t"),
                        "{text}"
                    );
                    oracle.check(
                        &format!("sabal-{commute}-{sel:?}-{alloc:?}-{pool}"),
                        &text,
                        &obj,
                        &c,
                    );
                }
            }
        }
    }
}

struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "sabal-controlled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let parts: Vec<_> = input
            .func
            .name
            .rsplit('_')
            .take(4)
            .map(|x| x.parse::<usize>().unwrap())
            .collect();
        let mut locs = [Loc::Stack(0); 4];
        for (i, &p) in parts.iter().rev().enumerate() {
            locs[i] = if p == 3 {
                Loc::Stack(i as u32 * 2)
            } else {
                Loc::FReg(input.pools.float_caller_saved[p])
            };
        }
        for (v, loc) in [0, 1, 2, 11].into_iter().zip(locs) {
            a.locs[0][v] = Some(loc);
        }
        a.locs[0][4] = Some(Loc::Stack(8));
        a.spill_slots = a.spill_slots.max(10);
        a
    }
}
#[test]
fn destructive_destination_and_all_input_spill_alias_layouts() {
    let mut oracle = support::Oracle::new("sabal-controlled");
    for same in [false, true] {
        let mut src = String::new();
        let mut names = Vec::new();
        // Inputs may alias only when their SSA values agree. The accumulator
        // and distinct narrow inputs otherwise start in independent locations.
        for acc in 0..4 {
            for left in 0..4 {
                for right in 0..4 {
                    for dest in 0..4 {
                        if (acc == left && acc != 3)
                            || (acc == right && acc != 3)
                            || (left == right && left != 3)
                        {
                            continue;
                        }
                        let name = format!("case_{acc}_{left}_{right}_{dest}");
                        src += &SOURCE.replace("@sabal", &format!("@{name}"));
                        names.push(name);
                    }
                }
            }
        }
        if same {
            src = src.replace("swiden_low.i64x2 v2", "swiden_low.i64x2 v1");
        }
        let m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            let (text, obj) =
                support::compile(&m, &sel, &Controlled, &keel_codegen::aarch64::REGISTERS);
            assert_eq!(text.matches("sabal\t").count(), names.len());
            oracle.check(
                &format!("sabal-controlled-{same}-{sel:?}"),
                &text,
                &obj,
                &oracle_c(&names, same),
            );
        }
    }
}

#[test]
fn shared_wide_uses_and_effect_barriers_reject_cover() {
    let mut cases = vec![
        SOURCE.replace("iconst.i64 0", "iconst.i64 1"),
        SOURCE.replace("swiden_low", "uwiden_low"),
        SOURCE.replace("vicmp.slt", "vicmp.ult"),
        SOURCE.replace("vselect v8,v9,v7", "vselect v8,v7,v9"),
        SOURCE.replace("viadd v0,v10", "viadd v10,v10"),
        SOURCE.replace("v9 = vineg", "v12 = iconst.i64 99\n v9 = vineg"),
        SOURCE.replace("return v11", "asm \"nop\", \"\"()\n return v11"),
    ];
    for v in 5..=10 {
        cases.push(SOURCE.replace("return v11", &format!("v12 = viadd v{v},v11\n return v12")));
    }
    for source in cases {
        let m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &sel,
                allocator: &AllocationTier::Full,
            };
            let text = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!text.contains("sabal\t"), "{source}\n{text}");
        }
    }
}

#[test]
fn shared_live_leaves_and_accumulator_survive_root_reuse() {
    let src=SOURCE.replace("func @sabal(i64x2,i32x4,i32x4) -> i64x2", "func @live(ptr,ptr,ptr,ptr)")
        .replace("block0(v0:i64x2,v1:i32x4,v2:i32x4):", "block0(v20:ptr,v21:ptr,v22:ptr,v23:ptr):\n v0 = load.i64x2 align 1 v20\n v1 = load.i32x4 align 1 v21\n v2 = load.i32x4 align 1 v22")
        .replace(" return v11", " store align 1 v11,v23\n v12 = iconst.i64 16\n v13 = ptradd v23,v12\n store align 1 v0,v13\n v14 = iconst.i64 32\n v15 = ptradd v23,v14\n store align 1 v1,v15\n v16 = iconst.i64 48\n v17 = ptradd v23,v16\n store align 1 v2,v17\n v18 = iconst.i64 64\n v19 = ptradd v23,v18\n store align 1 v11,v19\n return");
    let m = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let c = r#"#include <stdint.h>
#include <string.h>
extern void live(const void*,const void*,const void*,void*);
int main(void){for(unsigned p=0;p<128;p++){for(unsigned alias=0;alias<4;alias++){
 uint64_t a[12],x[12],y[12],out[12];for(unsigned l=0;l<12;l++){a[l]=UINT64_MAX-l;x[l]=0x800000007fffffffULL+l+p;y[l]=0x7fffffff80000000ULL-l-p;out[l]=0;}
 uint64_t expected[10];int32_t xx[4],yy[4];memcpy(xx,x,16);memcpy(yy,y,16);
 for(unsigned l=0;l<2;l++){int64_t d=(int64_t)xx[l]-(int64_t)yy[l];expected[l]=a[l]+(uint64_t)(d<0?-d:d);}
 memcpy(expected+2,a,16);memcpy(expected+4,x,16);memcpy(expected+6,y,16);memcpy(expected+8,expected,16);
 void *dest=alias==0?(void*)out:alias==1?(void*)a:alias==2?(void*)x:(void*)y;
 live(a,x,y,dest);if(memcmp(dest,expected,80))return 1;
}}return 0;}
"#;
    let mut oracle = support::Oracle::new("sabal-live");
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::aarch64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &support::EMPTY),
            ] {
                let (text, obj) = support::compile(&m, &sel, &alloc, pools);
                assert_eq!(text.matches("sabal\t").count(), 1);
                oracle.check(
                    &format!("sabal-live-{sel:?}-{alloc:?}-{pool}"),
                    &text,
                    &obj,
                    c,
                );
            }
        }
    }
}

#[test]
fn replicated_history_keeps_upper_duplicate_and_all_window_lanes() {
    let src = "func @history(ptr,ptr,i32x4,i64x2) -> i64x2 {
block0(v0:ptr,v1:ptr,v2:i32x4,v3:i64x2):
 v4 = load.i64 align 1 v0
 v5 = vsplat.i64x2 v4
 v6 = bitcast.i32x4 v5
 v7 = vshuffle v2,v6,[3,4,5,6]
 v8 = iconst.i64 0
 v9 = vsplat.i64x2 v8
 v10 = swiden_low.i64x2 v6
 v11 = swiden_low.i64x2 v7
 v12 = visub v10,v11
 v13 = vicmp.slt v12,v9
 v14 = vineg v12
 v15 = vselect v13,v14,v12
 v16 = viadd v3,v15
 store align 1 v5,v1
 v17 = iconst.i64 16
 v18 = ptradd v1,v17
 store align 1 v6,v18
 v19 = iconst.i64 32
 v20 = ptradd v1,v19
 store align 1 v7,v20
 return v16
}";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let c = r#"#include <stdint.h>
#include <string.h>
typedef int32_t N __attribute__((vector_size(16)));
typedef uint64_t W __attribute__((vector_size(16)));
extern W history(const void*,void*,N,W);
int main(void){const int32_t e[]={INT32_MIN,INT32_MAX,0,-1,1};for(unsigned p=0;p<125;p++){
 int32_t input[2]={e[p%5],e[(p/5)%5]},output[12];N prev={13,-99,41,e[(p/25)%5]};W acc={UINT64_MAX,UINT64_MAX-4294967294ULL};
 int32_t want[12]={input[0],input[1],input[0],input[1],input[0],input[1],input[0],input[1],prev[3],input[0],input[1],input[0]};
 W got=history(input,output,prev,acc);for(unsigned l=0;l<2;l++){int64_t d=(int64_t)input[l]-(l?(int64_t)input[0]:(int64_t)prev[3]);if(got[l]!=acc[l]+(uint64_t)(d<0?-d:d))return 1;}if(memcmp(output,want,48))return 2;
}return 0;}
"#;
    let mut oracle = support::Oracle::new("sabal-history");
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::aarch64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &support::EMPTY),
            ] {
                let (text, obj) = support::compile(&m, &sel, &alloc, pools);
                assert_eq!(text.matches("sabal\t").count(), 1);
                assert!(text.contains("ld1r\t"));
                assert!(text.contains("#12"));
                oracle.check(
                    &format!("sabal-history-{sel:?}-{alloc:?}-{pool}"),
                    &text,
                    &obj,
                    c,
                );
            }
        }
    }
}

#[test]
fn constraints_on_every_matched_instruction_keep_exact_fallback() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{Function, InstData, Value};
    struct Constrained(SelectionTier, usize);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "sabal-constrained"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = self.0.select(f, t, folds);
            let i = f.block_insts(f.entry_block().unwrap())[self.1];
            s.set_constraint(
                i,
                ValuePart::low(f.first_result(i)),
                OperandKind::Def,
                Constraint::Register,
            );
            s
        }
    }
    let m = keel_ir::parse(SOURCE).unwrap();
    let mut oracle = support::Oracle::new("sabal-constraint");
    let c = oracle_c(&["sabal".into()], false);
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for at in 2..9 {
                let (text, obj) = support::compile(
                    &m,
                    &Constrained(sel, at),
                    &alloc,
                    &keel_codegen::aarch64::REGISTERS,
                );
                assert!(!text.contains("sabal\t"));
                // Selection does not depend on the allocator, and with the
                // normal registers both allocators assign this fallback the
                // same registers, so the Full allocator executes it.
                if alloc == AllocationTier::Full {
                    oracle.check(&format!("sabal-constraint-{sel:?}-{at}"), &text, &obj, &c);
                }
            }
        }
    }
}

#[test]
fn barriers_between_every_pair_prevent_cover() {
    let lines = [" v6 =", " v7 =", " v8 =", " v9 =", " v10 =", " v11 ="];
    for line in lines {
        for barrier in [
            " fence seq_cst\n",
            " store v0,v13\n",
            " v14 = load.i64 volatile v13\n",
        ] {
            let src = SOURCE
                .replace("i32x4) ->", "i32x4,ptr) ->")
                .replace("v2:i32x4):", "v2:i32x4,v13:ptr):")
                .replace(line, &format!("{barrier}{line}"));
            let m = keel_ir::parse(&src).unwrap();
            keel_ir::verify_module(&m).unwrap();
            for sel in [SelectionTier::Baseline, SelectionTier::Full] {
                for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                    let (text, _) =
                        support::compile(&m, &sel, &alloc, &keel_codegen::aarch64::REGISTERS);
                    assert!(!text.contains("sabal\t"), "{src}");
                }
            }
        }
    }
}

#[test]
fn accumulator_and_narrow_inputs_can_share_registers() {
    use keel_ir::{ConvertOp, EntityRef, InstData};
    struct Aliased;
    impl RegisterAllocator for Aliased {
        fn name(&self) -> &'static str {
            "sabal-accumulator-alias"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let mut a = AllocationTier::Full.allocate(input);
            let f = input.func;
            let params = f.block_params(f.entry_block().unwrap());
            let r = |n: usize| Loc::FReg(input.pools.float_caller_saved[n]);
            a.locs[0][params[0].index()] = Some(r(0));
            a.locs[0][params[1].index()] = Some(r(1));
            for (_, i) in f.layout_insts() {
                let loc = match f.inst_data(i) {
                    InstData::Convert {
                        op: ConvertOp::Bitcast,
                        ..
                    } => Some(r(0)),
                    InstData::VectorSplat { .. } => Some(Loc::Stack(0)),
                    InstData::VectorBinary {
                        op: keel_ir::VectorBinaryOp::Iadd,
                        ..
                    } => Some(match f.name.rsplit('_').next().unwrap() {
                        "0" => r(0),
                        "1" => r(1),
                        "2" => r(2),
                        _ => Loc::Stack(2),
                    }),
                    _ => None,
                };
                if let Some(loc) = loc {
                    a.locs[0][f.first_result(i).index()] = Some(loc);
                }
            }
            a.spill_slots = a.spill_slots.max(4);
            a
        }
    }
    let mut oracle = support::Oracle::new("sabal-acc-alias");
    for side in ["left", "right", "both"] {
        let src = SOURCE.replace("i64x2,i32x4,i32x4", "i64x2,i32x4").replace(
            "block0(v0:i64x2,v1:i32x4,v2:i32x4):",
            "block0(v0:i64x2,v2:i32x4):\n v1 = bitcast.i32x4 v0",
        );
        let src = match side {
            "right" => src
                .replace("swiden_low.i64x2 v1", "swiden_low.i64x2 TEMP")
                .replace("swiden_low.i64x2 v2", "swiden_low.i64x2 v1")
                .replace("TEMP", "v2"),
            "both" => src.replace("swiden_low.i64x2 v2", "swiden_low.i64x2 v1"),
            _ => src,
        };
        let mut ir = String::new();
        let mut c = String::from(
            "#include <stdint.h>\n#include <string.h>\ntypedef uint64_t W __attribute__((vector_size(16)));typedef int32_t N __attribute__((vector_size(16)));\n",
        );
        for d in 0..4 {
            ir += &src.replace("@sabal", &format!("@alias_{d}"));
            c += &format!("extern W alias_{d}(W,N);\n");
        }
        c += "int main(void){for(unsigned p=0;p<1024;p++){W acc={0x800000007fffffffULL+p,UINT64_MAX-p};N x,y,input={-2147483647-1,2147483647,-1,1},bits;memcpy(&bits,&acc,16);";
        c += match side {
            "left" => "x=bits;y=input;",
            "right" => "x=input;y=bits;",
            _ => "x=bits;y=bits;",
        };
        c += "W expected;for(unsigned l=0;l<2;l++){int64_t diff=(int64_t)x[l]-(int64_t)y[l];expected[l]=acc[l]+(uint64_t)(diff<0?-diff:diff);}";
        for d in 0..4 {
            c += &format!("{{W got=alias_{d}(acc,input);if(memcmp(&got,&expected,16))return 1;}}");
        }
        c += "}return 0;}\n";
        let m = keel_ir::parse(&ir).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            let (text, obj) =
                support::compile(&m, &sel, &Aliased, &keel_codegen::aarch64::REGISTERS);
            assert_eq!(text.matches("sabal\t").count(), 4);
            oracle.check(&format!("sabal-acc-alias-{side}-{sel:?}"), &text, &obj, &c);
        }
    }
}

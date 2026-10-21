//! Exact wrapping I64x2 absolute value: native ARM and exportable SSE2 oracles.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
// PSRAD also lowers standalone sign comparisons. Identify the complete abs
// recipe rather than treating that shared instruction as proof of fusion.
fn abs_recipes(text: &str, arm: bool) -> usize {
    if arm {
        return text.matches("abs\t").count();
    }
    let opcodes: Vec<_> = text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    opcodes
        .windows(4)
        .filter(|w| *w == ["psrad", "pshufd", "pxor", "psubq"])
        .count()
}
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "packed-abs-controlled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let r = |i| Loc::FReg(input.pools.float_caller_saved[i]);
        let (x, y) = match input.func.name.rsplit('_').next().unwrap() {
            "alias" => (r(0), r(0)),
            "distinct" => (r(0), r(1)),
            "spillinput" => (Loc::Stack(0), r(1)),
            "spillresult" => (r(0), Loc::Stack(2)),
            "allspill" => (Loc::Stack(0), Loc::Stack(2)),
            _ => unreachable!(),
        };
        a.spill_slots = 4;
        a.locs[0][0] = Some(x);
        a.locs[0][5] = Some(y);
        a
    }
}
fn fixture(controlled: bool) -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ U; typedef U V __attribute__((vector_size(16))); extern void *memcpy(void*,const void*,__SIZE_TYPE__); extern int printf(const char*,...);\n",
    );
    let mut calls = String::new();
    for reverse in [false, true] {
        for layout in if controlled {
            &["alias", "distinct", "spillinput", "spillresult", "allspill"][..]
        } else {
            &["live"][..]
        } {
            let name = format!("abs_{}_{layout}", u8::from(reverse));
            let pair = if reverse {
                " v4 = vineg v0\n v3 = vicmp.slt v0,v2\n"
            } else {
                " v3 = vicmp.slt v0,v2\n v4 = vineg v0\n"
            };
            if controlled {
                ir += &format!(
                    "func @{name}(i64x2) -> i64x2 {{\nblock0(v0:i64x2):\n v1 = iconst.i64 0\n v2 = vsplat.i64x2 v1\n{pair} v5 = vselect v3,v4,v0\n return v5\n}}\n"
                );
                c += &format!(
                    "extern V {name}(V); static void call_{name}(const U *p,const U *q,U *o){{ V x; (void)q;memcpy(&x,p,16);x={name}(x);memcpy(o,&x,16); }}\n"
                );
            } else {
                // Both difference operands and the difference remain live beyond the cover.
                let pair = pair
                    .replace("v0", "v6")
                    .replace("v4", "v10")
                    .replace("v3", "v9")
                    .replace("v2", "v8");
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x2 align 1 v0\n v4 = load.i64x2 align 1 v1\n v6 = visub v3,v4\n v7 = iconst.i64 0\n v8 = vsplat.i64x2 v7\n{pair} v11 = vselect v9,v10,v6\n store align 1 v11,v2\n v12 = iconst.i64 16\n v13 = ptradd v2,v12\n store align 1 v3,v13\n v14 = iconst.i64 32\n v15 = ptradd v2,v14\n store align 1 v4,v15\n v16 = iconst.i64 48\n v17 = ptradd v2,v16\n store align 1 v6,v17\n return\n}}\n"
                );
                c += &format!("extern void {name}(const U*,const U*,U*);\n");
            }
            calls += &format!(
                " if(check(\"{name}\",{},{}))return 1;\n",
                if controlled {
                    format!("call_{name}")
                } else {
                    name.clone()
                },
                u8::from(!controlled)
            );
        }
    }
    c += r#"
static U state=0x42deadbeefUL;
static U random64(void){state^=state<<13;state^=state>>7;state^=state<<17;return state;}
static int check(const char *name, void (*run)(const U*,const U*,U*),int live){
 const U edges[]={0,1,2,0x7fffffffUL,0x80000000UL,0xffffffffUL,0x100000000UL,0x7fffffffffffffffUL,0x8000000000000000UL,0x8000000000000001UL,0xffffffffffffffffUL};
 for(unsigned i=0;i<121+4096;i++){
  U x[2],y[2],o[8]={0};
  for(unsigned l=0;l<2;l++){x[l]=i<121?edges[(i/11+l)%11]:random64();y[l]=i<121?edges[(i%11+l)%11]:random64();}
  run(x,y,o);
  for(unsigned l=0;l<2;l++){U z=live?x[l]-y[l]:x[l];U want=(z>>63)?0-z:z;
   if(o[l]!=want || (live && (o[2+l]!=x[l] || o[4+l]!=y[l] || o[6+l]!=z))){printf("failed %s case %u lane %u\n",name,i,l);return 1;}}
 }
 return 0;
}
int main(void){
"#;
    c += &calls;
    c += "return 0;}\n";
    (ir, c)
}
fn compile(
    arm: bool,
    source: &str,
    sel: SelectionTier,
    alloc: &dyn RegisterAllocator,
    spill: bool,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    compile_module(arm, &m, &sel, alloc, spill)
}
fn compile_module(
    arm: bool,
    m: &keel_ir::Module,
    selector: &dyn keel_codegen::InstructionSelector,
    alloc: &dyn RegisterAllocator,
    spill: bool,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector,
        allocator: alloc,
    };
    if arm {
        let pools = if spill {
            &EMPTY
        } else {
            &keel_codegen::aarch64::REGISTERS
        };
        let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
        let p = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
        assert_eq!(p.stats().text_bytes, 0);
        let obj = keel_asm::assemble_program_object(&p, Default::default()).unwrap();
        assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
        (text, obj)
    } else {
        let pools = if spill {
            &EMPTY
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
        let p = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
        assert_eq!(p.stats().text_bytes, 0);
        let obj = keel_asm::x86::assemble_program_object(&p).unwrap();
        assert_eq!(obj, keel_asm::x86::assemble_object(&text).unwrap());
        (text, obj)
    }
}
fn command(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(out.status.success(), "{c:?}: {out:?}");
}
fn oracle(arm: bool, stem: &str, source: &str, c: &str, asm: &str, obj: &[u8]) {
    let root = std::env::var_os("KEEL_PACKED_ABS_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-packed-abs-{}", std::process::id()))
        });
    let dir = root.join(if arm { "arm" } else { "x86" });
    fs::create_dir_all(&dir).unwrap();
    for (ext, bytes) in [
        ("keel", source.as_bytes()),
        ("c", c.as_bytes()),
        ("s", asm.as_bytes()),
        ("o", obj),
    ] {
        fs::write(dir.join(format!("{stem}.{ext}")), bytes).unwrap();
    }
    if !arm {
        fs::write(
            dir.join("run_native.py"),
            include_str!("packed_absolute/run_native.py"),
        )
        .unwrap();
    }
    let native = if arm {
        cfg!(all(target_arch = "aarch64", target_os = "macos"))
    } else {
        cfg!(all(target_arch = "x86_64", target_os = "linux"))
    };
    if native {
        let exe = dir.join(format!("{stem}.native"));
        let mut cc = Command::new("clang");
        cc.args(["-O2", "-Wall", "-Wextra", "-Werror"]);
        if !arm {
            cc.args(["-march=x86-64", "-msse2", "-mno-avx"]);
        }
        command(
            cc.arg(dir.join(format!("{stem}.c")))
                .arg(dir.join(format!("{stem}.o")))
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    } else {
        command(
            Command::new("clang")
                .args([
                    "-target",
                    if arm {
                        "arm64-apple-macos11"
                    } else {
                        "x86_64-unknown-linux-gnu"
                    },
                    "-ffreestanding",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-fsyntax-only",
                ])
                .arg(dir.join(format!("{stem}.c"))),
        );
    }
    // Check instruction bytes against an independent assembler as well as the
    // typed/text object equality above. The ELF/Mach-O containers may differ.
    let clang_obj = dir.join(format!("{stem}.clang.o"));
    command(
        Command::new("clang")
            .args([
                "-target",
                if arm {
                    "arm64-apple-macos11"
                } else {
                    "x86_64-unknown-linux-gnu"
                },
                "-c",
            ])
            .arg(dir.join(format!("{stem}.s")))
            .arg("-o")
            .arg(&clang_obj),
    );
    {
        use object::{Object, ObjectSection};
        let reference = fs::read(&clang_obj).unwrap();
        let name = if arm { "__text" } else { ".text" };
        let actual = object::File::parse(obj).unwrap();
        let expected = object::File::parse(reference.as_slice()).unwrap();
        assert_eq!(
            actual.section_by_name(name).unwrap().data().unwrap(),
            expected.section_by_name(name).unwrap().data().unwrap()
        );
    }
    if std::env::var_os("KEEL_PACKED_ABS_EVIDENCE").is_some() {
        fs::write(dir.join(format!("{stem}.json")),format!("{{\"passed\":true,\"native_executed\":{native},\"typed_text_objects_equal\":true,\"clang_text_equal\":true}}\n")).unwrap();
        fs::write(dir.join(format!("{stem}.html")),format!("<!doctype html><meta charset=utf-8><title>{stem}</title><h1>{stem}</h1><p>Passed. Native execution: {native}. Typed/text objects match. Clang instruction bytes match.</p>")).unwrap();
    }
    if std::env::var_os("KEEL_PACKED_ABS_EVIDENCE").is_none() {
        for ext in ["keel", "c", "s", "o", "native", "clang.o"] {
            let path = dir.join(format!("{stem}.{ext}"));
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }
}

#[test]
fn controlled_aliases_and_spills() {
    let (ir, c) = fixture(true);
    for arm in [true, false] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let (text, obj) = compile(arm, &ir, sel, &Controlled, false);
            assert_eq!(abs_recipes(&text, arm), 10, "{text}");
            oracle(arm, &format!("controlled-{sel:?}"), &ir, &c, &text, &obj);
        }
    }
}
#[test]
fn live_sources_full_fast_and_all_spills() {
    let (ir, c) = fixture(false);
    for arm in [true, false] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for spill in [false, true] {
                    let (text, obj) = compile(arm, &ir, sel, &alloc, spill);
                    assert_eq!(abs_recipes(&text, arm), 2, "{text}");
                    oracle(
                        arm,
                        &format!("live-{sel:?}-{alloc:?}-{spill}"),
                        &ir,
                        &c,
                        &text,
                        &obj,
                    );
                }
            }
        }
    }
}

#[test]
fn arbitrary_masks_nonzero_compare_and_swapped_arms() {
    let mut ir = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ U;typedef U V __attribute__((vector_size(16)));extern void *memcpy(void*,const void*,__SIZE_TYPE__);\n",
    );
    for kind in ["mask", "nonzero", "swapped"] {
        let cmp = if kind == "mask" {
            "v3 = bitcast.i64x2 v1"
        } else {
            "v3 = vicmp.slt v0,v2"
        };
        let arms = if kind == "swapped" {
            "v3,v0,v4"
        } else {
            "v3,v4,v0"
        };
        ir += &format!(
            "func @{kind}(i64x2,i64x2) -> i64x2 {{\nblock0(v0:i64x2,v1:i64x2):\n v6 = iconst.i64 {}\n v2 = vsplat.i64x2 v6\n {cmp}\n v4 = vineg v0\n v5 = vselect {arms}\n return v5\n}}\n",
            u8::from(kind == "nonzero")
        );
        c += &format!("extern V {kind}(V,V);\n");
    }
    c += r#"
int main(void){const U edges[]={0,1,0x7fffffffffffffffUL,0x8000000000000000UL,0xffffffffffffffffUL,0x80000000UL,0x100000000UL};
for(unsigned i=0;i<49;i++){U x[2]={edges[i/7],edges[(i/7+1)%7]},m[2]={edges[i%7],edges[(i%7+1)%7]},o[2];V a,b,r;memcpy(&a,x,16);memcpy(&b,m,16);
r=mask(a,b);memcpy(o,&r,16);for(unsigned l=0;l<2;l++)if(o[l]!=(m[l]?0-x[l]:x[l]))return 1;
r=nonzero(a,b);memcpy(o,&r,16);for(unsigned l=0;l<2;l++)if(o[l]!=(((x[l]>>63)||x[l]==0)?0-x[l]:x[l]))return 2;
r=swapped(a,b);memcpy(o,&r,16);for(unsigned l=0;l<2;l++)if(o[l]!=((x[l]>>63)?x[l]:0-x[l]))return 3;
}return 0;}
"#;
    for arm in [true, false] {
        for spill in [false, true] {
            let (text, obj) = compile(arm, &ir, SelectionTier::Full, &AllocationTier::Full, spill);
            assert!(abs_recipes(&text, arm) == 0, "{text}");
            oracle(arm, &format!("negative-{spill}"), &ir, &c, &text, &obj);
        }
    }
}

#[test]
fn custom_constraints_and_debug_lines_keep_fallback_semantics() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Constrained;
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "packed-abs-constrained"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            for (_, i) in f.layout_insts() {
                if let InstData::VectorSelect { args } = *f.inst_data(i) {
                    // Preserve the original negate result at a constrained boundary.
                    s.set_constraint(
                        i,
                        ValuePart::low(f.first_result(i)),
                        OperandKind::Def,
                        Constraint::Register,
                    );
                    s.set_constraint(
                        i,
                        ValuePart::low(args[1]),
                        OperandKind::Use,
                        Constraint::Register,
                    );
                }
            }
            s
        }
    }
    let (ir, c) = fixture(true);
    for arm in [true, false] {
        let mut m = keel_ir::parse(&ir).unwrap();
        let (text, obj) = compile_module(arm, &m, &Constrained, &AllocationTier::Full, false);
        assert!(abs_recipes(&text, arm) == 0, "{text}");
        oracle(arm, "custom-constraints", &ir, &c, &text, &obj);
        m.files.push("packed-abs.c".into());
        for f in &mut m.functions {
            let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
            for (n, i) in insts.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: 10 + n as u32,
                        col: 1,
                        scope: None,
                    }),
                );
            }
        }
        let (text, obj) =
            compile_module(arm, &m, &SelectionTier::Full, &AllocationTier::Full, true);
        assert!(abs_recipes(&text, arm) == 0, "{text}");
        for function in text.split(".globl").skip(1) {
            let mut cursor = 0;
            for line in 12..=14 {
                let marker = format!(".loc\t1 {line} 1");
                cursor += function[cursor..].find(&marker).unwrap() + marker.len();
            }
        }
        oracle(arm, "debug-barriers", &ir, &c, &text, &obj);
    }
}

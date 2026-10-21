//! Bounded I32x4 min/max: independent Clang encoding and four-lane runtime oracle.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use keel_ir::EntityRef;
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "arm-minmax-controlled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let r = |i| Loc::FReg(input.pools.float_caller_saved[i]);
        let (x, y, z) = match input.func.name.rsplit('_').next().unwrap() {
            "aliasa" => (r(0), r(1), r(0)),
            "aliasb" => (r(0), r(1), r(1)),
            "distinct" => (r(0), r(1), r(2)),
            "spilla" => (Loc::Stack(0), r(1), r(2)),
            "spillb" => (r(0), Loc::Stack(2), r(2)),
            "spillresult" => (r(0), r(1), Loc::Stack(4)),
            "allspill" => (Loc::Stack(0), Loc::Stack(2), Loc::Stack(4)),
            _ => unreachable!(),
        };
        a.spill_slots = 6;
        let (_, cmp) = input
            .func
            .layout_insts()
            .find(|&(_, i)| {
                matches!(
                    input.func.inst_data(i),
                    keel_ir::InstData::VectorIcmp { .. }
                )
            })
            .unwrap();
        let keel_ir::InstData::VectorIcmp { args, .. } = *input.func.inst_data(cmp) else {
            unreachable!()
        };
        let (_, root) = input
            .func
            .layout_insts()
            .find(|&(_, i)| {
                matches!(
                    input.func.inst_data(i),
                    keel_ir::InstData::VectorSelect { .. }
                        | keel_ir::InstData::VectorBinary {
                            op: keel_ir::VectorBinaryOp::And,
                            ..
                        }
                )
            })
            .unwrap();
        for (v, l) in [
            (args[0].index(), x),
            (args[1].index(), y),
            (input.func.first_result(root).index(), z),
        ] {
            a.locs[0][v] = Some(l);
        }
        a
    }
}
fn dir() -> PathBuf {
    let dir = std::env::var_os("KEEL_ARM_MINMAX_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-arm-minmax-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn command(c: &mut Command) {
    let o = c.output().unwrap();
    assert!(o.status.success(), "{c:?}: {o:?}");
}
fn compile(
    m: &keel_ir::Module,
    s: &dyn keel_codegen::InstructionSelector,
    a: &dyn RegisterAllocator,
    spill: bool,
) -> (String, keel_asm_ir::Program, Vec<u8>) {
    let p = CodegenPipeline {
        selector: s,
        allocator: a,
    };
    let pools = if spill {
        &EMPTY
    } else {
        &keel_codegen::aarch64::REGISTERS
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &p).unwrap();
    let program = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &p).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let obj = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
    (text, program, obj)
}
fn module(s: &str) -> keel_ir::Module {
    let m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn fixture(controlled: bool) -> (String, String, usize) {
    fixture_mode(controlled, false)
}
fn fixture_mode(controlled: bool, zero: bool) -> (String, String, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\ntypedef uint32_t V __attribute__((vector_size(16)));\n",
    );
    let mut calls = String::new();
    let mut count = 0;
    for (cc, signed, less) in [
        ("slt", true, true),
        ("sle", true, true),
        ("sgt", true, false),
        ("sge", true, false),
        ("ult", false, true),
        ("ule", false, true),
        ("ugt", false, false),
        ("uge", false, false),
    ] {
        if zero && !matches!(cc, "sgt" | "slt") {
            continue;
        }
        for rev in [false, true] {
            for layout in if controlled {
                &[
                    "aliasa",
                    "aliasb",
                    "distinct",
                    "spilla",
                    "spillb",
                    "spillresult",
                    "allspill",
                ][..]
            } else {
                &["live"][..]
            } {
                let name = format!("mm_{cc}_{}_{layout}", u8::from(rev));
                let (yes, no) = if rev { ("v1", "v0") } else { ("v0", "v1") };
                if controlled {
                    ir += &format!(
                        "func @{name}(i32x4,i32x4) -> i32x4 {{\nblock0(v0:i32x4,v1:i32x4):\n v2 = vicmp.{cc} v0,v1\n v3 = vselect v2,{yes},{no}\n return v3\n}}\n"
                    );
                    c += &format!(
                        "extern V {name}(V,V);static void call_{name}(const uint32_t*a,const uint32_t*b,uint32_t*o){{V x,y,z;memcpy(&x,a,16);memcpy(&y,b,16);z={name}(x,y);memcpy(o,&z,16);}}\n"
                    );
                } else {
                    let (yes, no) = if rev { ("v4", "v3") } else { ("v3", "v4") };
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i32x4 align 1 v0\n v4 = load.i32x4 align 1 v1\n v5 = vicmp.{cc} v3,v4\n v6 = vselect v5,{yes},{no}\n store align 1 v6,v2\n v7 = iconst.i64 16\n v8 = ptradd v2,v7\n store align 1 v3,v8\n v9 = iconst.i64 32\n v10 = ptradd v2,v9\n store align 1 v4,v10\n return\n}}\n"
                    );
                    c += &format!(
                        "extern void {name}(const uint32_t*,const uint32_t*,uint32_t*);\n"
                    );
                }
                calls += &format!(
                    "if(check(\"{name}\",{},{},{},{}))return 1;\n",
                    if controlled {
                        format!("call_{name}")
                    } else {
                        name.clone()
                    },
                    u8::from(signed),
                    u8::from(if zero { less } else { less != rev }),
                    u8::from(!controlled)
                );
                count += 1;
            }
        }
    }
    c += r#"
static uint32_t state=0xabcdef01;
static uint32_t rnd(void){state^=state<<13;state^=state>>17;state^=state<<5;return state;}
static int check(const char*name,void(*run)(const uint32_t*,const uint32_t*,uint32_t*),int sign,int minimum,int live){
 const uint32_t edges[]={0,1,2,0x7ffffffe,0x7fffffff,0x80000000,0x80000001,0xfffffffe,0xffffffff};
 for(unsigned i=0;i<81+4096;i++){
  uint32_t x[4],y[4],o[12]={0};
  for(unsigned l=0;l<4;l++){x[l]=i<81?edges[(i/9+l)%9]:rnd();y[l]=i<81?edges[(i%9+l)%9]:rnd();}
  run(x,y,o);
  for(unsigned l=0;l<4;l++){
   int less=sign?((int32_t)x[l]<(int32_t)y[l]):x[l]<y[l];
   uint32_t want=(less==minimum)?x[l]:y[l];
   if(o[l]!=want||(live&&(o[4+l]!=x[l]||o[8+l]!=y[l]))){printf("%s case %u lane %u\n",name,i,l);return 1;}
  }
 }
 return 0;
}
int main(void){
"#;
    c += &calls;
    c += "return 0;}\n";
    if zero {
        for cc in ["slt", "sgt"] {
            let (x, y, m, r) = if controlled {
                ("v0", "v1", "v2", "v3")
            } else {
                ("v3", "v4", "v5", "v6")
            };
            ir = ir.replace(
                &format!(" {m} = vicmp.{cc} {x},{y}"),
                &format!(" v20 = iconst.i32 0\n v21 = vsplat.i32x4 v20\n {m} = vicmp.{cc} {x},v21"),
            );
            ir = ir.replace(
                &format!("{r} = vselect {m},{x},{y}"),
                &format!("{r} = vand {m},{x}"),
            );
            ir = ir.replace(
                &format!("{r} = vselect {m},{y},{x}"),
                &format!("{r} = vand {x},{m}"),
            );
        }
        c = c.replace(
            "  run(x,y,o);",
            "  for(unsigned l=0;l<4;l++)y[l]=0;\n  run(x,y,o);",
        );
    }
    (ir, c, count)
}
fn oracle(stem: &str, ir: &str, c: &str, text: &str, obj: &[u8]) {
    let d = dir();
    for (ext, bytes) in [
        ("keel", ir.as_bytes()),
        ("c", c.as_bytes()),
        ("s", text.as_bytes()),
        ("o", obj),
    ] {
        fs::write(d.join(format!("{stem}.{ext}")), bytes).unwrap();
    }
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        let exe = d.join(format!("{stem}.native"));
        command(
            Command::new("clang")
                .args(["-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(d.join(format!("{stem}.c")))
                .arg(d.join(format!("{stem}.o")))
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    }
}
fn recipes(s: &str) -> usize {
    ["smin\t", "smax\t", "umin\t", "umax\t"]
        .iter()
        .map(|op| s.matches(op).count())
        .sum()
}
#[test]
fn independent_clang_native_encoding() {
    use object::{Object, ObjectSection};
    for zero in [false, true] {
        let (ir, _, n) = fixture_mode(true, zero);
        let m = module(&ir);
        let (text, p, _) = compile(&m, &SelectionTier::Baseline, &Controlled, false);
        assert_eq!(recipes(&text), n);
        assert!(!text.contains("bsl\t"));
        let d = dir();
        let input = d.join(format!("encoding-{}.s", u8::from(zero)));
        let out = d.join(format!("encoding-{}-clang.o", u8::from(zero)));
        fs::write(&input, &text).unwrap();
        command(
            Command::new("clang")
                .args(["-target", "arm64-apple-macos11", "-c"])
                .arg(input)
                .arg("-o")
                .arg(&out),
        );
        let bytes = fs::read(out).unwrap();
        let f = object::File::parse(&*bytes).unwrap();
        assert_eq!(
            keel_asm::assemble_program(&p, Default::default())
                .unwrap()
                .text,
            f.section_by_name("__text").unwrap().data().unwrap()
        );
    }
}
#[test]
fn all_four_lanes_aliases_spills_live_inputs_and_tiers() {
    for controlled in [true, false] {
        let (ir, c, n) = fixture(controlled);
        let m = module(&ir);
        for (si, s) in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ]
        .iter()
        .enumerate()
        {
            for (ai, a) in [AllocationTier::Fast, AllocationTier::Full]
                .iter()
                .enumerate()
            {
                for spill in [false, true] {
                    let alloc: &dyn RegisterAllocator = if controlled { &Controlled } else { a };
                    if controlled && (spill || ai != 0) {
                        continue;
                    }
                    let (text, _, obj) = compile(&m, s, alloc, spill);
                    assert_eq!(recipes(&text), n, "{text}");
                    assert!(!text.contains("bsl\t"));
                    oracle(
                        &format!(
                            "stage1-{}-{si}-{ai}-{}",
                            u8::from(controlled),
                            u8::from(spill)
                        ),
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

struct Constrained;
impl keel_codegen::InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "arm-minmax-constrained"
    }
    fn select(
        &self,
        f: &keel_ir::Function,
        t: keel_codegen::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        use keel_codegen::regalloc::{Constraint, OperandKind, ValuePart};
        let mut s =
            keel_codegen::InstructionSelector::select(&SelectionTier::Baseline, f, t, folds);
        for (_, i) in f.layout_insts() {
            let source = match *f.inst_data(i) {
                keel_ir::InstData::VectorSelect { args } => Some(args[1]),
                keel_ir::InstData::VectorBinary {
                    op: keel_ir::VectorBinaryOp::And,
                    args,
                } => Some(args[1]),
                _ => None,
            };
            if let Some(source) = source {
                s.set_constraint(
                    i,
                    ValuePart::low(f.first_result(i)),
                    OperandKind::Def,
                    Constraint::Register,
                );
                s.set_constraint(
                    i,
                    ValuePart::low(source),
                    OperandKind::Use,
                    Constraint::Register,
                );
            }
        }
        s
    }
}
#[test]
fn constraints_and_debug_keep_normal_fallback_correct() {
    for zero in [false, true] {
        let (ir, c, _) = fixture_mode(false, zero);
        let mut m = module(&ir);
        let (text, _, obj) = compile(&m, &Constrained, &AllocationTier::Full, false);
        assert_eq!(recipes(&text), 0);
        assert!(text.contains(if zero { "and\tv" } else { "bsl\t" }));
        oracle(
            &format!("constraints-{}", u8::from(zero)),
            &ir,
            &c,
            &text,
            &obj,
        );
        m.files.push("minmax.c".into());
        for f in &mut m.functions {
            let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
            for (n, i) in insts.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: 1 + n as u32,
                        col: 1,
                        scope: None,
                    }),
                );
            }
        }
        let (text, _, obj) = compile(&m, &SelectionTier::Full, &AllocationTier::Full, true);
        assert_eq!(recipes(&text), 0);
        assert!(text.contains(if zero { "and\tv" } else { "bsl\t" }));
        assert!(text.contains(".loc\t1 3 1"));
        assert!(text.contains(".loc\t1 4 1"));
        oracle(&format!("debug-{}", u8::from(zero)), &ir, &c, &text, &obj);
    }
}
#[test]
fn arbitrary_mask_select_preserves_lane_truth() {
    let ir = "func @arbitrary(ptr,ptr,ptr,ptr) {\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.i32x4 v0\n v5 = load.i32x4 v1\n v6 = load.i32x4 v2\n v7 = vselect v4,v5,v6\n store v7,v3\n return\n}\n";
    let c = r#"#include <stdint.h>
extern void arbitrary(const uint32_t*,const uint32_t*,const uint32_t*,uint32_t*);
int main(void){uint32_t m[4]={0,0x55555555,0x80000001,0xffffffff},a[4]={0xffffffff,0xdeadbeef,0x76543210,0},b[4]={1,0x12345678,0xfedcba98,0xffffffff},o[4]; arbitrary(m,a,b,o);for(unsigned i=0;i<4;i++)if(o[i]!=(m[i]?a[i]:b[i]))return 1;return 0;}
"#;
    for spill in [false, true] {
        let (text, _, obj) = compile(
            &module(ir),
            &SelectionTier::Full,
            &AllocationTier::Full,
            spill,
        );
        assert_eq!(recipes(&text), 0);
        assert!(text.contains("bsl\t"));
        oracle(
            &format!("arbitrary-{}", u8::from(spill)),
            ir,
            c,
            &text,
            &obj,
        );
    }
}

#[test]
fn zero_filters_all_lanes_native_aliases_spills_and_tiers() {
    for controlled in [true, false] {
        let (ir, c, n) = fixture_mode(controlled, true);
        let m = module(&ir);
        for (si, s) in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ]
        .iter()
        .enumerate()
        {
            for (ai, a) in [AllocationTier::Fast, AllocationTier::Full]
                .iter()
                .enumerate()
            {
                for spill in [false, true] {
                    let alloc: &dyn RegisterAllocator = if controlled { &Controlled } else { a };
                    if controlled && (spill || ai != 0) {
                        continue;
                    }
                    let (text, _, obj) = compile(&m, s, alloc, spill);
                    assert_eq!(recipes(&text), n, "{text}");
                    assert!(!text.contains("cmgt\t"));
                    assert!(!text.contains("and\tv"));
                    oracle(
                        &format!(
                            "stage2-{}-{si}-{ai}-{}",
                            u8::from(controlled),
                            u8::from(spill)
                        ),
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
fn external_masks_and_nonzero_threshold_keep_fallback_results() {
    let mut ir = String::new();
    for (name, compare, root) in [
        ("external_select", "vicmp.slt v3,v4", "vselect v5,v3,v4"),
        ("external_zero", "vicmp.sgt v3,v8", "vand v5,v3"),
        ("threshold_one", "vicmp.sgt v3,v8", "vand v5,v3"),
    ] {
        let threshold = if name == "threshold_one" { 1 } else { 0 };
        ir += &format!(
            "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i32x4 align 1 v0\n v4 = load.i32x4 align 1 v1\n v7 = iconst.i32 {threshold}\n v8 = vsplat.i32x4 v7\n v5 = {compare}\n v6 = {root}\n store align 1 v6,v2\n v9 = iconst.i64 16\n v10 = ptradd v2,v9\n store align 1 v5,v10\n return\n}}\n"
        );
    }
    let c = r#"#include <stdint.h>
extern void external_select(const uint32_t*,const uint32_t*,uint32_t*);
extern void external_zero(const uint32_t*,const uint32_t*,uint32_t*);
extern void threshold_one(const uint32_t*,const uint32_t*,uint32_t*);
static uint32_t state=0x12345678;
static uint32_t rnd(void){state^=state<<13;state^=state>>17;state^=state<<5;return state;}
int main(void){
 void(*run[])(const uint32_t*,const uint32_t*,uint32_t*)={external_select,external_zero,threshold_one};
 const uint32_t edges[]={0,1,2,0x7fffffff,0x80000000,0xffffffff};
 for(unsigned n=0;n<3;n++)for(unsigned k=0;k<36+4096;k++){
  uint32_t a[4],b[4],out[8];
  for(unsigned l=0;l<4;l++){a[l]=k<36?edges[(k/6+l)%6]:rnd();b[l]=k<36?edges[(k%6+l)%6]:rnd();}
  run[n](a,b,out);
  for(unsigned l=0;l<4;l++){
   uint32_t m=0u-(uint32_t)(n==0?(int32_t)a[l]<(int32_t)b[l]:(int32_t)a[l]>(n==2?1:0));
   uint32_t want=n==0?(m?a[l]:b[l]):(m&a[l]);
   if(out[l]!=want||out[4+l]!=m)return 1;
  }
 }
 return 0;
}
"#;
    for spill in [false, true] {
        let (text, _, obj) = compile(
            &module(&ir),
            &SelectionTier::Full,
            &AllocationTier::Full,
            spill,
        );
        assert_eq!(recipes(&text), 0);
        assert!(text.contains("cmgt\t"));
        assert!(text.contains("bsl\t"));
        oracle(
            &format!("external-masks-{}", u8::from(spill)),
            &ir,
            c,
            &text,
            &obj,
        );
    }
}

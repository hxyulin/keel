//! Direct AVX2 integer adds: allocated destinations, live sources and exact footprints.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use std::{fs, path::PathBuf, process::Command};

const EMPTY: Pools = Pools {
    float_caller_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};
const ALL_EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    float_caller_saved: &[0],
    ..keel_codegen::x86_64::REGISTERS
};
const SHAPES: [(&str, usize); 4] = [("i8x32", 1), ("i16x16", 2), ("i32x8", 4), ("i64x4", 8)];

fn compile(
    source: &str,
    s: SelectionTier,
    a: &dyn RegisterAllocator,
    p: &Pools,
    f: Features,
) -> (String, keel_asm_ir::Program, Vec<u8>) {
    let mut module = keel_ir::parse(source).unwrap();
    // Keep distinct SSA operand IDs until resolution: the second load aliases
    // the first in these functions, without running any optimizer.
    for f in &mut module.functions {
        if f.name.starts_with("add_") && f.name.ends_with("_2") {
            f.replace_all_uses(keel_ir::Value::new(4), keel_ir::Value::new(3));
        }
    }
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, p, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, p, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(&module, p, &pipeline, &mut |p| {
        chunks.items.extend(p.items)
    })
    .unwrap();
    for p in [&program, &chunks] {
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(f.contains(i.required_features()), "{i}");
            }
        }
    }
    assert_eq!(
        keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap(),
        keel_asm::x86::assemble_program_object_with_features(&chunks, f).unwrap()
    );
    let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    (text, program, object)
}

fn assert_fixture_selection(text: &str) {
    for suffix in ["b", "w", "d", "q"] {
        assert!(text.contains(&format!("\tvpadd{suffix}\t")));
    }
}

fn fixture() -> (String, String) {
    let mut ir = String::from("decl @clobber()\n");
    let mut c = String::from(
        r#"
typedef unsigned char u8;
typedef __UINT64_TYPE__ u64;
typedef __SIZE_TYPE__ usize;
extern void *memcpy(void*,const void*,usize);
extern void *memset(void*,int,usize);
extern int memcmp(const void*,const void*,usize);
extern int puts(const char*);
extern void *mmap(void*,usize,int,int,int,long);
extern int mprotect(void*,usize,int);
extern int munmap(void*,usize);
extern long sysconf(int);
static unsigned csr(void) {unsigned x; __asm__ volatile("stmxcsr %0":"=m"(x));return x;}
__attribute__((noinline)) void clobber(void) {
 __asm__ volatile("vzeroall" ::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7",
 "xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15");
}
struct Case {void (*run)(const void*,const void*,void*);unsigned width,same;};
"#,
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for (shape, width) in SHAPES {
        for mode in 0..4 {
            let name = format!("add_{shape}_{mode}");
            let rhs = if mode == 1 { "v3" } else { "v4" };
            let call = if mode == 3 { " call @clobber()\n" } else { "" };
            ir += &format!(
                "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n{call} v5 = viadd v3,{rhs}\n store align 1 v5,v2\n v6 = iconst.i64 32\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
            );
            c += &format!("extern void {name}(const void*,const void*,void*);\n");
            cases += &format!("{{{name},{width},{mode}}},\n");
        }
    }
    c += &(cases + "};\n");
    c += r#"
static int check(const struct Case *c,const u8 *a,const u8 *b,u8 *o) {
 u8 aa[32],bb[32],expected[96];memcpy(aa,a,32);memcpy(bb,b,32);
 if(c->same==2)memcpy(bb,aa,32);
 for(unsigned i=0;i<32;i+=c->width) {
  u64 x=0,y=0;memcpy(&x,aa+i,c->width);memcpy(&y,(c->same==1?aa:bb)+i,c->width);
  u64 sum=x+y;memcpy(expected+i,&sum,c->width);
 }
 memcpy(expected+32,aa,32);memcpy(expected+64,bb,32);
 unsigned state=csr();c->run(a,b,o);
 return memcmp(o,expected,96)||csr()!=state;
}
int main(void) {
 // Modular integer edges plus deterministic byte patterns; unsigned scalar oracle.
 const u64 edges[]={0,0x8000000000000000ULL,0x7ff0123456789abcULL,0x7ff8123456789abcULL,
  0x800000007f812345ULL,0x7fc123457f800000ULL,~0ULL,0x0102030405060708ULL};
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++)
 for(unsigned pattern=0;pattern<264;pattern++)for(unsigned offset=0;offset<32;offset++)
 for(unsigned alias=0;alias<3;alias++) {
  u8 aa[192],bb[192],oo[192];memset(aa,0x5a,192);memset(bb,0x5a,192);memset(oo,0x5a,192);
  u8 *a=aa+16+offset,*b=bb+16+offset,*o=(alias==0?oo:alias==1?aa:bb)+16+offset;
  for(unsigned i=0;i<32;i++) {a[i]=(u8)(pattern+31*i);b[i]=(u8)(pattern*7+13*i);}
  if(pattern>=256)for(unsigned i=0;i<4;i++) {memcpy(a+8*i,&edges[(pattern+i)%8],8);memcpy(b+8*i,&edges[(pattern+i+3)%8],8);}
  if(check(&cases[k],a,b,o)||o[-1]!=0x5a||o[96]!=0x5a)return 1;
 }
 usize page=(usize)sysconf(30);u8 *maps[3];
 for(unsigned i=0;i<3;i++){maps[i]=mmap(0,3*page,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 2;}
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++)for(unsigned edge=0;edge<2;edge++) {
  u8 *a=maps[0]+page+(edge?page-32:0),*b=maps[1]+page+(edge?page-32:0),*o=maps[2]+page+(edge?page-96:0);
  for(unsigned i=0;i<32;i++){a[i]=(u8)(17*i);b[i]=(u8)(255-11*i);}
  if(check(&cases[k],a,b,o))return 3;
 }
 for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 4;
 puts("PASS AVX2 adds: wrapping, live sources, aliases, unaligned and guard boundaries, MXCSR");return 0;
}
"#;
    (ir, c)
}

fn directory(test: &str) -> PathBuf {
    std::env::var_os("KEEL_AVX2_ADD_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-avx2-add-{}-{test}", std::process::id()))
        })
}
fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(output.status.success(), "{c:?}: {output:?}");
}
#[test]
fn avx2_add_encodings_and_semantic_oracles() {
    let (source, c) = fixture();
    fs::create_dir_all(directory("semantic")).unwrap();
    fs::write(directory("semantic").join("fixture.keel"), &source).unwrap();
    let dir = directory("semantic");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("add.c"), &c).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(dir.join("add.c")),
    );
    for level in [CpuLevel::V3] {
        let features = Features::for_level(level);
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, p) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("spilled", &EMPTY),
                    ("tiny", &TINY),
                    ("empty", &ALL_EMPTY),
                ] {
                    let (text, _, object) = compile(&source, s, &a, p, features);
                    assert_fixture_selection(&text);
                    let stem = format!("add-{level:?}-{s:?}-{a:?}-{pool}");
                    fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                    fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                    fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
                    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                    if std::is_x86_feature_detected!("avx2") {
                        command(
                            Command::new("clang")
                                .args(["-O2", "-march=x86-64", "-mno-avx"])
                                .arg(dir.join(format!("{stem}.c")))
                                .arg(dir.join(format!("{stem}.o")))
                                .arg("-o")
                                .arg(dir.join(&stem)),
                        );
                        command(&mut Command::new(dir.join(&stem)));
                    }
                }
            }
        }
    }
    if std::env::var_os("KEEL_AVX2_ADD_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

struct Controlled(u8);
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-wide-add"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let r = |n| Loc::FReg(input.pools.float_caller_saved[n]);
        let base = a.spill_slots;
        let slots = [Loc::Stack(base), Loc::Stack(base + 4), Loc::Stack(base + 8)];
        let locs = match self.0 {
            0 => [r(0), r(1), r(0)], // Source/result alias.
            1 => [r(0), r(1), r(1)],
            2 => [slots[0], r(1), r(2)],
            3 => [r(0), r(1), slots[2]],
            4 => slots,
            5 => [r(0), slots[1], r(2)],
            6 => [slots[0], slots[1], r(2)],
            _ => unreachable!(),
        };
        a.spill_slots += 12;
        // Find definitions after moving captures before the add: textual SSA
        // names are not guaranteed to retain their numeric entity IDs.
        let mut values = Vec::new();
        for (_, inst) in input.func.layout_insts() {
            if matches!(
                input.func.inst_data(inst),
                keel_ir::InstData::Load { .. } | keel_ir::InstData::VectorBinary { .. }
            ) {
                values.push(input.func.first_result(inst));
            }
        }
        assert_eq!(values.len(), 3);
        for (value, loc) in values.into_iter().zip(locs) {
            a.locs[0][value.index()] = Some(loc);
        }
        a
    }
}

#[test]
fn add_controlled_register_aliases_and_spills() {
    let (mut source, c) = fixture();
    // Capture the original sources before the add so their registers may
    // legally be reused by the result. The ordinary fixture keeps them live.
    let stores = " v6 = iconst.i64 32\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v4,v9\n";
    source = source.replace(" call @clobber()\n", "");
    source = source
        .replace(stores, "")
        .replace(" v5 = viadd", &(stores.to_owned() + " v5 = viadd"));
    let dir = directory("controlled");
    fs::create_dir_all(&dir).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for layout in 0..7 {
            let (text, _, object) = compile(
                &source,
                s,
                &Controlled(layout),
                &keel_codegen::x86_64::REGISTERS,
                Features::for_level(CpuLevel::V3),
            );
            assert_fixture_selection(&text);
            // Check each controlled function, not just one instruction in the corpus.
            let (d, a, b) = [
                (0, 0, 1),
                (1, 0, 1),
                (2, 13, 1),
                (13, 0, 1),
                (13, 13, 14),
                (2, 0, 14),
                (2, 13, 14),
            ][layout as usize];
            for (shape, _) in SHAPES {
                for mode in 0..4 {
                    let label = format!("\"add_{shape}_{mode}\":\n");
                    let body = text
                        .split_once(&label)
                        .unwrap()
                        .1
                        .split_once("\t.cfi_endproc")
                        .unwrap()
                        .0;
                    let add = body.lines().find(|l| l.starts_with("\tvpadd")).unwrap();
                    let b = if mode == 1 || mode == 2 {
                        if a == 13 { 14 } else { a }
                    } else {
                        b
                    };
                    assert!(
                        add.ends_with(&format!("ymm{d}, ymm{a}, ymm{b}")),
                        "{label}: {add}"
                    );
                }
            }
            let stem = format!("controlled-{s:?}-{layout}");
            fs::write(dir.join(format!("{stem}.o")), object).unwrap();
            fs::write(dir.join(format!("{stem}.s")), text).unwrap();
            fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
            #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
            if std::is_x86_feature_detected!("avx2") {
                command(
                    Command::new("clang")
                        .args(["-O2", "-march=x86-64", "-mno-avx"])
                        .arg(dir.join(format!("{stem}.c")))
                        .arg(dir.join(format!("{stem}.o")))
                        .arg("-o")
                        .arg(dir.join(&stem)),
                );
                command(&mut Command::new(dir.join(&stem)));
            }
        }
    }
}

#[test]
fn feature_boundaries_and_direct_register_destination() {
    let source = "func @add(i64x4,i64x4)->i64x4 {\nentry(v0:i64x4,v1:i64x4):\n v2 = viadd v0,v1\n return v2\n}\n";
    let module = keel_ir::parse(source).unwrap();
    for level in [CpuLevel::V1, CpuLevel::V2] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::for_level(level),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let p = &keel_codegen::x86_64::REGISTERS;
        assert!(keel_codegen::x86_64::compile_with_pipeline(&module, p, &pipeline).is_err());
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&module, p, &pipeline).is_err()
        );
        assert!(
            keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                &module,
                p,
                &pipeline,
                &mut |_| {}
            )
            .is_err()
        );
    }
    for a in [AllocationTier::Fast, AllocationTier::Full] {
        let (text, _, _) = compile(
            source,
            SelectionTier::Full,
            &a,
            &keel_codegen::x86_64::REGISTERS,
            Features::for_level(CpuLevel::V3),
        );
        let add = text.lines().find(|l| l.starts_with("\tvpaddq\t")).unwrap();
        assert!(!add.contains("ymm13") && !add.contains("ymm14"), "{text}");
        assert!(
            !text
                .lines()
                .any(|l| l.starts_with("\tvmovups\t")
                    && (l.contains("ymm13") || l.contains("ymm14"))),
            "{text}"
        );
    }
    let (source, _) = fixture();
    for (pool, p) in [
        ("spilled", &EMPTY),
        ("normal", &keel_codegen::x86_64::REGISTERS),
    ] {
        let (text, _, object) = compile(
            &source,
            SelectionTier::Full,
            &AllocationTier::Full,
            p,
            Features::for_level(CpuLevel::V3)
                .with_features("-avx2")
                .unwrap(),
        );
        assert!(
            !text.contains("\tvpadd"),
            "AVX-only must retain legal fallback"
        );
        let dir = directory("fallback");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("fallback-{pool}.o")), object).unwrap();
        fs::write(dir.join(format!("fallback-{pool}.s")), text).unwrap();
        fs::write(dir.join(format!("fallback-{pool}.c")), fixture().1).unwrap();
    }
}

/// Fixed explicit-wide IR comparison. Run unchanged at base and candidate;
/// parsing/optimization are outside the measured codegen and assembly stages.
#[test]
#[ignore = "fixed-width codegen measurement; run both revisions with --nocapture"]
fn measure_signed_sum_direct_add() {
    use object::{Object, ObjectSection};
    use std::time::Instant;
    let source = include_str!("../../keel-opt/tests/fixtures/signed_widen_sum.ir");
    let mut module = keel_ir::parse(source).unwrap();
    let features = Features::for_level(CpuLevel::V3);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let context = keel_opt::OptimizationContext {
        target: keel_opt::VectorTarget::X86Sse2,
        capabilities: keel_codegen::x86_features::vector_capabilities(
            keel_codegen::Target::X86_64LinuxGnu,
            &selector,
        )
        .unwrap()
        .with_wide_preference(true),
    };
    keel_opt::run_passes_with_context_and_options(
        &mut module,
        keel_opt::DEFAULT_PIPELINE,
        context,
        keel_opt::OptimizeOptions::default(),
    )
    .unwrap();
    assert!(module.to_string().contains("i64x4"));
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let p = &keel_codegen::x86_64::REGISTERS;
    let dir = directory("measure").join("measure");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("sum.keel"), module.to_string()).unwrap();
    let mut times = Vec::new();
    for round in 0..103 {
        let at = Instant::now();
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, p, &pipeline).unwrap();
        let text_ns = at.elapsed().as_nanos();
        let at = Instant::now();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, p, &pipeline).unwrap();
        let typed_ns = at.elapsed().as_nanos();
        let at = Instant::now();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        let object_ns = at.elapsed().as_nanos();
        if round >= 3 {
            times.push(format!("{text_ns},{typed_ns},{object_ns}"));
        }
        if round == 0 {
            let bytes = object::File::parse(object.as_slice())
                .unwrap()
                .section_by_name(".text")
                .unwrap()
                .data()
                .unwrap()
                .len();
            println!(
                "signed_sum: instructions={} text_bytes={bytes} vmovups={} vpaddq={}",
                program.stats().instructions,
                text.matches("\tvmovups\t").count(),
                text.matches("\tvpaddq\t").count()
            );
            fs::write(dir.join("sum.s"), text).unwrap();
            fs::write(dir.join("sum.o"), object).unwrap();
        }
    }
    fs::write(
        dir.join("compile-ns.csv"),
        format!(
            "text_codegen,typed_codegen,object_encoding\n{}\n",
            times.join("\n")
        ),
    )
    .unwrap();
}

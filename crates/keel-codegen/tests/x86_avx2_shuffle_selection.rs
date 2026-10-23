//! AVX2 half-local dword shuffles must preserve bits, live inputs and exact load/store widths.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use object::{Object, ObjectSection};
use std::{fs, path::PathBuf, process::Command};

const EMPTY: Pools = Pools {
    float_caller_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};
const SHAPES: [(&str, usize); 2] = [("i32x8", 4), ("f32x8", 4)];

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
        if f.name.starts_with("shuffle_") && f.name.ends_with("_2") {
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
    let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    (text, program, object)
}

fn patterns(_: usize) -> Vec<Vec<usize>> {
    [0u8, 27, 78, 177, 228, 255]
        .into_iter()
        .map(|imm| {
            (0..8)
                .map(|i| {
                    usize::from((imm >> (2 * (i % 4))) & 3)
                        + i / 4 * 4
                        + if i % 2 == 0 { 8 } else { 0 }
                })
                .collect()
        })
        .chain([
            vec![3, 2, 1, 0, 4, 5, 6, 7], // Different high-half control: fallback.
            vec![7, 6, 5, 4, 3, 2, 1, 0], // Cross-half: fallback.
        ])
        .collect()
}

// Check every exported oracle, so native success cannot silently exercise only
// the general recipe after a regression in equal-source resolution or selection.
fn assert_fixture_selection(text: &str) {
    for (shape, width) in SHAPES {
        for (index, lanes) in patterns(width).iter().enumerate() {
            for mode in 0..3 {
                let name = format!("shuffle_{shape}_{index}_{mode}");
                let label = format!("\"{name}\":\n");
                let body = text
                    .split_once(&label)
                    .unwrap_or_else(|| panic!("missing {name}"))
                    .1
                    .split_once("\t.cfi_endproc")
                    .unwrap()
                    .0;
                let selected = body
                    .lines()
                    .filter(|line| line.starts_with("\tvpshufd\t"))
                    .collect::<Vec<_>>();
                if mode != 0 && index < 6 {
                    assert_eq!(selected.len(), 1, "{name}");
                    let control = (0..4).fold(0, |imm, i| imm | ((lanes[i] % 8) << (2 * i)));
                    assert_eq!(
                        selected[0].rsplit_once(',').unwrap().1.trim(),
                        control.to_string(),
                        "{name}"
                    );
                    assert!(!body.contains("vpshufb"), "{name}");
                } else {
                    assert!(selected.is_empty(), "{name}");
                    assert!(body.contains("vpshufb"), "{name}");
                }
            }
        }
    }
}

fn fixture() -> (String, String) {
    let mut ir = String::new();
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
struct Case {void (*run)(const void*,const void*,void*);unsigned width,same;u8 lanes[8];};
"#,
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for (shape, width) in SHAPES {
        for (index, lanes) in patterns(width).iter().enumerate() {
            for mode in 0..3 {
                let name = format!("shuffle_{shape}_{index}_{}", mode);
                let indices = lanes
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let rhs = if mode == 1 { "v3" } else { "v4" };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vshuffle v3,{rhs},[{indices}]\n store align 1 v5,v2\n v6 = iconst.i64 32\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                cases += &format!("{{{name},{width},{},{{{indices}}}}},\n", mode);
            }
        }
    }
    c += &(cases + "};\n");
    c += r#"
static int check(const struct Case *c,const u8 *a,const u8 *b,u8 *o) {
 u8 aa[32],bb[32],expected[96];memcpy(aa,a,32);memcpy(bb,b,32);
 if(c->same==2)memcpy(bb,aa,32);
 for(unsigned i=0;i<32/c->width;i++) {unsigned l=c->lanes[i],n=32/c->width;
  memcpy(expected+i*c->width,(l<n||c->same?aa:bb)+(l%n)*c->width,c->width);}
 memcpy(expected+32,aa,32);memcpy(expected+64,bb,32);
 unsigned state=csr();c->run(a,b,o);
 return memcmp(o,expected,96)||csr()!=state;
}
int main(void) {
 // Raw bit patterns include signed zeros, quiet/signaling NaNs and infinities.
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
 puts("PASS AVX2 shuffles: bits, live sources, aliases, unaligned and guard boundaries, MXCSR");return 0;
}
"#;
    (ir, c)
}

fn directory(test: &str) -> PathBuf {
    std::env::var_os("KEEL_AVX2_SHUFFLE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-avx2-shuffle-{}-{test}", std::process::id()))
        })
}
fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(output.status.success(), "{c:?}: {output:?}");
}
fn text_bytes(object: &[u8]) -> Vec<u8> {
    object::File::parse(object)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn avx2_shuffle_encodings_and_semantic_oracles() {
    let (source, c) = fixture();
    let dir = directory("semantic");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("shuffle.c"), &c).unwrap();
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
            .arg(dir.join("shuffle.c")),
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
                ] {
                    let (text, _, object) = compile(&source, s, &a, p, features);
                    assert_fixture_selection(&text);
                    let stem = format!("shuffle-{level:?}-{s:?}-{a:?}-{pool}");
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
    if std::env::var_os("KEEL_AVX2_SHUFFLE_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn shuffle_immediate_encodings_match_clang() {
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    for imm in 0..256 {
        asm += &format!("vpshufd ymm15,ymm14,{imm}\n");
    }
    let dir = directory("encoding").join("encoding");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("all.s"), &asm).unwrap();
    command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(dir.join("all.s"))
            .arg("-o")
            .arg(dir.join("clang.o")),
    );
    let ours =
        keel_asm::x86::assemble_object_with_features(&asm, Features::for_level(CpuLevel::V3))
            .unwrap();
    assert_eq!(
        text_bytes(&ours),
        text_bytes(&fs::read(dir.join("clang.o")).unwrap())
    );
    if std::env::var_os("KEEL_AVX2_SHUFFLE_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Diagnostic comparison on an unchanged corpus; includes all function overhead.
#[test]
#[ignore = "code-generation measurement; run at both revisions with --nocapture"]
fn measure_avx2_shuffle_codegen() {
    let (source, _) = fixture();
    let dir = directory("measure").join("measure");
    fs::create_dir_all(&dir).unwrap();
    let reverse = "func @halfreverse32(i32x8)->i32x8 {\nentry(v0:i32x8):\n v1 = vshuffle v0,v0,[3,2,1,0,7,6,5,4]\n return v1\n}\n";
    let (text, program, object) = compile(
        reverse,
        SelectionTier::Full,
        &AllocationTier::Full,
        &keel_codegen::x86_64::REGISTERS,
        Features::for_level(CpuLevel::V3),
    );
    println!(
        "halfreverse32: instructions={} bytes={}",
        program.stats().instructions,
        text_bytes(&object).len()
    );
    fs::write(dir.join("halfreverse32.s"), text).unwrap();
    fs::write(dir.join("halfreverse32.o"), object).unwrap();
    fs::write(dir.join("corpus.keel"), &source).unwrap();
    for level in [CpuLevel::V3] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, p) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("spilled", &EMPTY),
            ] {
                let (text, program, object) = compile(
                    &source,
                    SelectionTier::Full,
                    &a,
                    p,
                    Features::for_level(level),
                );
                // Count emitted instructions touching spill/frame memory, separately
                // from explicit [rdi]/[rsi]/[rdx] application memory operations.
                let stack_accesses = text
                    .lines()
                    .filter(|l| {
                        l.contains("ptr [rbp") || l.contains("ptr [rsp") || l.contains("ptr [r15")
                    })
                    .count();
                let stem = format!("{level:?}-{a:?}-{pool}");
                println!(
                    "{stem}: instructions={} bytes={} stack_accesses={stack_accesses}",
                    program.stats().instructions,
                    text_bytes(&object).len()
                );
                fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                fs::write(dir.join(format!("{stem}.o")), object).unwrap();
            }
        }
    }
    if std::env::var_os("KEEL_AVX2_SHUFFLE_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn shuffle_feature_boundaries_and_exact_selection() {
    let reverse = "func @reverse(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n v2 = load.i32x8 align 1 v0\n v3 = vshuffle v2,v2,[3,2,1,0,7,6,5,4]\n store align 1 v3,v1\n return\n}\n";
    let m = keel_ir::parse(reverse).unwrap();
    for level in [CpuLevel::V1, CpuLevel::V2] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::for_level(level),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline
            )
            .is_err()
        );
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline
            )
            .is_err()
        );
    }
    let mut avx = Features::for_level(CpuLevel::V3);
    avx.avx2 = false;
    let (text, _, _) = compile(
        reverse,
        SelectionTier::Full,
        &AllocationTier::Full,
        &EMPTY,
        avx,
    );
    assert!(!text.contains("vpshufd"));

    let features = Features::for_level(CpuLevel::V3);
    for mask in ["3,2,1,0,4,5,6,7", "7,6,5,4,3,2,1,0"] {
        let source = reverse.replace("3,2,1,0,7,6,5,4", mask);
        let (text, _, _) = compile(
            &source,
            SelectionTier::Full,
            &AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            features,
        );
        assert!(!text.contains("vpshufd"));
        assert!(text.contains("vpshufb"));
    }
    for shape in ["i32x8", "f32x8"] {
        let mut source = String::new();
        for imm in 0..256 {
            let lanes = (0..8)
                .map(|i| (((imm >> (2 * (i % 4))) & 3) + i / 4 * 4).to_string())
                .collect::<Vec<_>>()
                .join(",");
            source += &reverse
                .replace("@reverse", &format!("@control_{imm}"))
                .replace("i32x8", shape)
                .replace("3,2,1,0,7,6,5,4", &lanes);
        }
        let (text, _, _) = compile(
            &source,
            SelectionTier::Full,
            &AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            features,
        );
        assert_eq!(text.matches("\tvpshufd\t").count(), 256);
        assert!(!text.contains("vpshufb"));
    }
}

struct Controlled(u8);
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-wide-shuffle"
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
            _ => unreachable!(),
        };
        a.spill_slots += 12;
        for (i, loc) in locs.into_iter().enumerate() {
            a.locs[0][i + 3] = Some(loc);
        }
        a
    }
}

#[test]
fn shuffle_controlled_register_aliases_and_spills() {
    let (mut source, c) = fixture();
    // Capture the original sources before the shuffle so their registers may
    // legally be reused by the result. The ordinary fixture keeps them live.
    let stores = " v6 = iconst.i64 32\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v4,v9\n";
    source = source
        .replace(stores, "")
        .replace(" v5 = vshuffle", &(stores.to_owned() + " v5 = vshuffle"));
    let dir = directory("controlled");
    fs::create_dir_all(&dir).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for layout in 0..5 {
            let (text, _, object) = compile(
                &source,
                s,
                &Controlled(layout),
                &keel_codegen::x86_64::REGISTERS,
                Features::for_level(CpuLevel::V3),
            );
            assert_fixture_selection(&text);
            let stem = format!("controlled-{s:?}-{layout}");
            fs::write(dir.join(format!("{stem}.o")), object).unwrap();
            fs::write(dir.join(format!("{stem}.s")), text).unwrap();
            fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
        }
    }
}

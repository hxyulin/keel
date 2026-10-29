//! Generic shuffles must preserve bits, live inputs and exact load/store widths.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use object::{Object, ObjectSection};
use std::{fs, path::PathBuf, process::Command};

const EMPTY: Pools = Pools {
    float_caller_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};
const SHAPES: [(&str, usize); 6] = [
    ("i8x16", 1),
    ("i16x8", 2),
    ("i32x4", 4),
    ("i64x2", 8),
    ("f32x4", 4),
    ("f64x2", 8),
];

fn compile(
    source: &str,
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> (String, keel_asm_ir::Program, Vec<u8>) {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
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

fn patterns(width: usize) -> Vec<Vec<usize>> {
    if width == 8 {
        return (0..16).map(|i| vec![i % 4, i / 4]).collect();
    }
    // Dword reverse, splat, second source, both half orders and repeated lanes.
    [
        [3, 2, 1, 0],
        [0, 0, 0, 0],
        [4, 5, 6, 7],
        [7, 6, 5, 4],
        [0, 3, 5, 6],
        [7, 4, 2, 1],
        [2, 2, 7, 7],
        [5, 6, 1, 0],
        [3, 4, 1, 6],
        [6, 1, 7, 0],
    ]
    .into_iter()
    .map(|dwords| {
        dwords
            .into_iter()
            .flat_map(|d| (0..4 / width).map(move |i| d * 4 / width + i))
            .collect()
    })
    .collect()
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
struct Case {void (*run)(const void*,const void*,void*);unsigned width,same;u8 lanes[16];};
"#,
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for (shape, width) in SHAPES {
        for (index, lanes) in patterns(width).iter().enumerate() {
            for same in [false, true] {
                let name = format!("shuffle_{shape}_{index}_{}", u8::from(same));
                let indices = lanes
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let rhs = if same { "v3" } else { "v4" };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vshuffle v3,{rhs},[{indices}]\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                cases += &format!("{{{name},{width},{},{{{indices}}}}},\n", u8::from(same));
            }
        }
    }
    c += &(cases + "};\n");
    c += r#"
static int check(const struct Case *c,const u8 *a,const u8 *b,u8 *o) {
 u8 aa[16],bb[16],expected[48];memcpy(aa,a,16);memcpy(bb,b,16);
 for(unsigned i=0;i<16/c->width;i++) {unsigned l=c->lanes[i],n=16/c->width;
  memcpy(expected+i*c->width,(l<n||c->same?aa:bb)+(l%n)*c->width,c->width);}
 memcpy(expected+16,aa,16);memcpy(expected+32,bb,16);
 unsigned state=csr();c->run(a,b,o);
 return memcmp(o,expected,48)||csr()!=state;
}
int main(void) {
 // Raw bit patterns include signed zeros, quiet/signaling NaNs and infinities.
 const u64 edges[]={0,0x8000000000000000ULL,0x7ff0123456789abcULL,0x7ff8123456789abcULL,
  0x800000007f812345ULL,0x7fc123457f800000ULL,~0ULL,0x0102030405060708ULL};
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++)
 for(unsigned pattern=0;pattern<264;pattern++)for(unsigned offset=0;offset<16;offset++)
 for(unsigned alias=0;alias<3;alias++) {
  u8 aa[96],bb[96],oo[96];memset(aa,0x5a,96);memset(bb,0x5a,96);memset(oo,0x5a,96);
  u8 *a=aa+16+offset,*b=bb+16+offset,*o=(alias==0?oo:alias==1?aa:bb)+16+offset;
  for(unsigned i=0;i<16;i++) {a[i]=(u8)(pattern+31*i);b[i]=(u8)(pattern*7+13*i);}
  if(pattern>=256)for(unsigned i=0;i<2;i++) {memcpy(a+8*i,&edges[(pattern+i)%8],8);memcpy(b+8*i,&edges[(pattern+i+3)%8],8);}
  if(check(&cases[k],a,b,o)||o[-1]!=0x5a||o[48]!=0x5a)return 1;
 }
 usize page=(usize)sysconf(30);u8 *maps[3];
 for(unsigned i=0;i<3;i++){maps[i]=mmap(0,3*page,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 2;}
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++)for(unsigned edge=0;edge<2;edge++) {
  u8 *a=maps[0]+page+(edge?page-16:0),*b=maps[1]+page+(edge?page-16:0),*o=maps[2]+page+(edge?page-48:0);
  for(unsigned i=0;i<16;i++){a[i]=(u8)(17*i);b[i]=(u8)(255-11*i);}
  if(check(&cases[k],a,b,o))return 3;
 }
 for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 4;
 puts("PASS generic shuffles: bits, live sources, aliases, unaligned and guard boundaries, MXCSR");return 0;
}
"#;
    (ir, c)
}

fn directory() -> PathBuf {
    std::env::var_os("KEEL_SIMD_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-generic-shuffle-{}", std::process::id()))
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
fn generic_shuffle_encodings_and_semantic_oracles() {
    let (source, c) = fixture();
    let dir = directory();
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
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
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
                    let (text, _, object) = compile(&source, s, a, p, features);
                    assert!(text.contains("pshufd"));
                    assert!(text.contains("shufps"));
                    // No scalarization lane buffers in normal allocation.
                    if pool == "normal" {
                        assert!(!text.contains("xmmword ptr [rbp"));
                        assert!(!text.contains(" = stack_addr"));
                    }
                    let stem = format!("shuffle-{level:?}-{s:?}-{a:?}-{pool}");
                    fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                    fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                    fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
                    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                    {
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
    if std::env::var_os("KEEL_SIMD_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn shuffle_immediate_encodings_match_clang() {
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    for imm in 0..256 {
        asm += &format!("pshufd xmm15,xmm14,{imm}\nshufps xmm15,xmm14,{imm}\n");
    }
    let dir = directory().join("encoding");
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
        keel_asm::x86::assemble_object_with_features(&asm, Features::for_level(CpuLevel::V1))
            .unwrap();
    assert_eq!(
        text_bytes(&ours),
        text_bytes(&fs::read(dir.join("clang.o")).unwrap())
    );
    if std::env::var_os("KEEL_SIMD_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

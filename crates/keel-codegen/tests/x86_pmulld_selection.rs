//! Effective-feature retention and selection of generic modular i32x4 multiply.
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
fn compile(
    source: &str,
    s: SelectionTier,
    a: &dyn RegisterAllocator,
    p: &Pools,
    f: Features,
) -> (String, keel_asm_ir::Program, Vec<u8>) {
    let mut m = keel_ir::parse(source).unwrap();
    for func in &mut m.functions {
        if func.name == "mul_2" {
            func.replace_all_uses(keel_ir::Value::new(4), keel_ir::Value::new(3));
        }
    }
    keel_ir::verify_module(&m).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline).unwrap();
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(&m, p, &pipeline, &mut |p| {
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
    let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_program_object_with_features(&chunks, f).unwrap()
    );
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    (text, program, object)
}
fn profiles() -> Vec<(&'static str, Features)> {
    [
        ("v1", CpuLevel::V1, None),
        ("v2", CpuLevel::V2, None),
        ("v3", CpuLevel::V3, None),
        ("v1+sse41", CpuLevel::V1, Some(true)),
        ("v2-sse41", CpuLevel::V2, Some(false)),
        ("v3-sse41", CpuLevel::V3, Some(false)),
    ]
    .into_iter()
    .map(|(n, l, modifier)| {
        let mut f = Features::for_level(l);
        if let Some(x) = modifier {
            f.sse41 = x;
        }
        (n, f)
    })
    .collect()
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        r#"
typedef unsigned char u8;
typedef unsigned u32;
typedef __SIZE_TYPE__ usize;
extern void *memcpy(void*,const void*,usize);
extern void *memset(void*,int,usize);
extern int memcmp(const void*,const void*,usize);
extern int puts(const char*);
extern void *mmap(void*,usize,int,int,int,long);
extern int mprotect(void*,usize,int);
extern int munmap(void*,usize);
extern long sysconf(int);
extern int fork(void);
extern int waitpid(int,int*,int);
extern void _exit(int);
extern void (*signal(int,void (*)(int)))(int);
static void fault(int sig){(void)sig;_exit(77);}
static unsigned csr(void){unsigned x;__asm__ volatile("stmxcsr %0":"=m"(x));return x;}
struct Case {void (*run)(const void*,const void*,void*);unsigned same;};
"#,
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for mode in 0..3 {
        let rhs = if mode == 1 { "v3" } else { "v4" };
        ir += &format!(
            "func @mul_{mode}(ptr,ptr,ptr) {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i32x4 align 1 v0\n v4 = load.i32x4 align 1 v1\n v5 = vimul v3,{rhs}\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
        );
        c += &format!("extern void mul_{mode}(const void*,const void*,void*);\n");
        cases += &format!("{{mul_{mode},{mode}}},\n");
    }
    c += &(cases + "};\n");
    c += r#"
static int check(const struct Case *c,const u8 *a,const u8 *b,u8 *o) {
 u32 aa[4],bb[4],expected[12];memcpy(aa,a,16);memcpy(bb,b,16);
 if(c->same==2)memcpy(bb,aa,16);
 for(unsigned i=0;i<4;i++)expected[i]=aa[i]*(c->same?aa[i]:bb[i]);
 memcpy(expected+4,aa,16);memcpy(expected+8,bb,16);
 unsigned state=csr();c->run(a,b,o);
 return memcmp(o,expected,48)||csr()!=state;
}
static u32 random32(u32 *s){*s^=*s<<13;*s^=*s>>17;*s^=*s<<5;return *s;}
int main(void) {
 const u32 edges[]={0,1,0xffffffffU,0x80000000U,0x7fffffffU,0x10000U,0xffffU,0xdeadbeefU};
 u32 seed=0x12345678U;
 for(unsigned k=0;k<3;k++)for(unsigned pattern=0;pattern<576;pattern++)
 for(unsigned offset=0;offset<16;offset++)for(unsigned alias=0;alias<3;alias++) {
  u8 aa[128],bb[128],oo[128];memset(aa,0x5a,128);memset(bb,0x5a,128);memset(oo,0x5a,128);
  u8 *a=aa+16+offset,*b=bb+16+offset,*o=(alias==0?oo:alias==1?aa:bb)+16+offset;
  for(unsigned i=0;i<4;i++) {u32 x=pattern<64?edges[(pattern/8+i)%8]:random32(&seed),y=pattern<64?edges[(pattern+i)%8]:random32(&seed);memcpy(a+4*i,&x,4);memcpy(b+4*i,&y,4);}
  if(check(&cases[k],a,b,o)||o[-1]!=0x5a||o[48]!=0x5a)return 1;
 }
 usize page=(usize)sysconf(30);u8 *maps[3];
 for(unsigned i=0;i<3;i++){maps[i]=mmap(0,3*page,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 2;}
 for(unsigned k=0;k<3;k++)for(unsigned edge=0;edge<2;edge++) {
  u8 *a=maps[0]+page+(edge?page-16:0),*b=maps[1]+page+(edge?page-16:0),*o=maps[2]+page+(edge?page-48:0);
  for(unsigned i=0;i<16;i++){a[i]=(u8)(17*i);b[i]=(u8)(255-11*i);}
  if(check(&cases[k],a,b,o))return 3;
 }
 // A crossing input must fault before any output store. Shared output lets
 // the parent check the interrupted call without relying on register state.
 u8 *shared=mmap(0,page,3,0x21,-1,0);if(shared==(void*)-1)return 4;
 for(unsigned input=0;input<2;input++)for(unsigned edge=0;edge<2;edge++) {
  memset(shared,0xa5,48);
  const u8 *a=maps[0]+page,*b=maps[1]+page;
  const u8 *bad=edge?maps[input]+2*page-15:maps[input]+page-1;
  if(input==0)a=bad;else b=bad;
  int pid=fork();if(pid<0)return 5;
  if(pid==0){signal(11,fault);signal(7,fault);cases[0].run(a,b,shared);_exit(1);}
  int status=0;if(waitpid(pid,&status,0)!=pid||status!=(77<<8))return 6;
  for(unsigned i=0;i<48;i++)if(shared[i]!=0xa5)return 7;
 }
 if(munmap(shared,page))return 8;
 for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 9;
 puts("PASS modular dword multiply: extrema/random, aliases, live sources, exact128 boundaries, MXCSR");return 0;
}
"#;
    (ir, c)
}
fn directory() -> PathBuf {
    std::env::var_os("KEEL_PMULLD_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("keel-pmulld-{}", std::process::id())))
}
fn command(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(out.status.success(), "{c:?}: {out:?}");
}
fn text_bytes(o: &[u8]) -> Vec<u8> {
    object::File::parse(o)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn export(dir: &std::path::Path, stem: &str, text: &str, object: &[u8], c: &str) {
    fs::write(dir.join(format!("{stem}.o")), object).unwrap();
    fs::write(dir.join(format!("{stem}.s")), text).unwrap();
    fs::write(dir.join(format!("{stem}.c")), c).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    if std::is_x86_feature_detected!("sse4.1") {
        command(
            Command::new("clang")
                .args(["-O2", "-march=x86-64", "-mno-avx"])
                .arg(dir.join(format!("{stem}.c")))
                .arg(dir.join(format!("{stem}.o")))
                .arg("-o")
                .arg(dir.join(stem)),
        );
        command(&mut Command::new(dir.join(stem)));
    }
}
#[test]
fn effective_feature_selection_and_oracles() {
    let (ir, c) = fixture();
    let dir = directory();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), &c).unwrap();
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
            .arg(dir.join("oracle.c")),
    );
    for (name, f) in profiles() {
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
                    let (text, _, object) = compile(&ir, s, &a, p, f);
                    assert_eq!(
                        text.matches("\tpmulld\t").count(),
                        if f.sse41 { 3 } else { 0 },
                        "{name} {s:?} {a:?} {pool}"
                    );
                    assert_eq!(
                        text.matches("\tpmuludq\t").count(),
                        if f.sse41 { 0 } else { 6 }
                    );
                    assert!(!text.contains(" = stack_addr"), "{text}");
                    export(
                        &dir,
                        &format!("{name}-{s:?}-{a:?}-{pool}"),
                        &text,
                        &object,
                        &c,
                    );
                }
            }
        }
    }
}
#[test]
fn independent_pmulld_register_encoding_oracle() {
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    for a in 0..16 {
        for b in 0..16 {
            asm += &format!("pmulld xmm{a},xmm{b}\n");
        }
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
        keel_asm::x86::assemble_object_with_features(&asm, Features::for_level(CpuLevel::V2))
            .unwrap();
    assert_eq!(
        text_bytes(&ours),
        text_bytes(&fs::read(dir.join("clang.o")).unwrap())
    );
}
struct Controlled(AllocationTier, u8);
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-pmulld"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = self.0.allocate(input);
        let r = |n| Loc::FReg(input.pools.float_caller_saved[n]);
        let base = a.spill_slots;
        let slots = [Loc::Stack(base), Loc::Stack(base + 4), Loc::Stack(base + 8)];
        let locs = match self.1 {
            0 => [r(0), r(1), r(0)],
            1 => [r(0), r(1), r(1)],
            2 => [slots[0], r(1), r(2)],
            3 => [r(0), slots[1], r(2)],
            4 => [r(0), r(1), slots[2]],
            5 => slots,
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
fn controlled_aliases_and_independent_spills() {
    let (mut ir, c) = fixture();
    // Capture live-source outputs before allowing legal destination reuse.
    let stores = " v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n";
    ir = ir
        .replace(stores, "")
        .replace(" v5 = vimul", &(stores.to_owned() + " v5 = vimul"));
    let dir = directory();
    fs::create_dir_all(&dir).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (profile, features) in [
                ("v1", Features::for_level(CpuLevel::V1)),
                ("v2", Features::for_level(CpuLevel::V2)),
            ] {
                for layout in 0..6 {
                    let (text, _, object) = compile(
                        &ir,
                        s,
                        &Controlled(a, layout),
                        &keel_codegen::x86_64::REGISTERS,
                        features,
                    );
                    assert_eq!(
                        text.matches("\tpmulld\t").count(),
                        if features.sse41 { 3 } else { 0 }
                    );
                    assert_eq!(
                        text.matches("\tpmuludq\t").count(),
                        if features.sse41 { 0 } else { 6 }
                    );
                    export(
                        &dir,
                        &format!("controlled-{profile}-{s:?}-{a:?}-{layout}"),
                        &text,
                        &object,
                        &c,
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "fixed-corpus codegen measurement; run at both revisions"]
fn measure_multiply_codegen() {
    let (ir, _) = fixture();
    let dir = directory().join("measure");
    fs::create_dir_all(&dir).unwrap();
    for (name, f) in profiles() {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, p) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("spilled", &EMPTY),
            ] {
                let (text, program, object) = compile(&ir, SelectionTier::Full, &a, p, f);
                let frame = text
                    .lines()
                    .filter(|l| {
                        l.contains("ptr [rbp") || l.contains("ptr [rsp") || l.contains("ptr [r15")
                    })
                    .count();
                println!(
                    "{name}-{a:?}-{pool}: instructions={} bytes={} frame_accesses={frame}",
                    program.stats().instructions,
                    text_bytes(&object).len()
                );
                fs::write(dir.join(format!("{name}-{a:?}-{pool}.s")), text).unwrap();
            }
        }
    }
}

/// Time only backend typed-program compilation, excluding parsing and assembly.
#[test]
#[ignore = "backend compile-time measurement; run release at both revisions"]
fn measure_multiply_backend_time() {
    let (ir, _) = fixture();
    let mut m = keel_ir::parse(&ir).unwrap();
    m.functions[2].replace_all_uses(keel_ir::Value::new(4), keel_ir::Value::new(3));
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::for_level(level),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let run = || {
            std::hint::black_box(
                keel_codegen::x86_64::compile_program_with_pipeline(
                    std::hint::black_box(&m),
                    &keel_codegen::x86_64::REGISTERS,
                    &pipeline,
                )
                .unwrap(),
            );
        };
        for _ in 0..100 {
            run();
        }
        for round in 0..7 {
            let start = std::time::Instant::now();
            for _ in 0..100 {
                run();
            }
            println!(
                "backend-{level:?}-{round}: ns_per_corpus={}",
                start.elapsed().as_nanos() / 100
            );
        }
    }
}

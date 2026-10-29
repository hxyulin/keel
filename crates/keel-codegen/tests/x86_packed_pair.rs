//! Exact signedness, saturation, rounding, product slices and SAD group sizes.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use std::{fs, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
// Name, input shape, output shape, bytes, required tier, independent formula.
const OPS: &[(&str, &str, &str, usize, u8, u8)] = &[
    ("x86_sse2_pmaddwd128", "i16x8", "i32x4", 16, 0, 0),
    ("x86_ssse3_pmaddubsw128", "i8x16", "i16x8", 16, 1, 1),
    ("x86_sse2_pmulhw128", "i16x8", "i16x8", 16, 0, 2),
    ("x86_sse2_pmulhuw128", "i16x8", "i16x8", 16, 0, 3),
    ("x86_ssse3_pmulhrsw128", "i16x8", "i16x8", 16, 1, 4),
    ("x86_sse41_pmuldq128", "i32x4", "i64x2", 16, 2, 5),
    ("x86_sse2_pmuludq128", "i32x4", "i64x2", 16, 0, 6),
    ("x86_sse2_psadbw128", "i8x16", "i64x2", 16, 0, 7),
    ("x86_avx_vpmaddwd128", "i16x8", "i32x4", 16, 3, 0),
    ("x86_avx_vpmaddubsw128", "i8x16", "i16x8", 16, 3, 1),
    ("x86_avx_vpmulhw128", "i16x8", "i16x8", 16, 3, 2),
    ("x86_avx_vpmulhuw128", "i16x8", "i16x8", 16, 3, 3),
    ("x86_avx_vpmulhrsw128", "i16x8", "i16x8", 16, 3, 4),
    ("x86_avx_vpmuldq128", "i32x4", "i64x2", 16, 3, 5),
    ("x86_avx_vpmuludq128", "i32x4", "i64x2", 16, 3, 6),
    ("x86_avx_vpsadbw128", "i8x16", "i64x2", 16, 3, 7),
    ("x86_avx2_vpmaddwd256", "i16x16", "i32x8", 32, 4, 0),
    ("x86_avx2_vpmaddubsw256", "i8x32", "i16x16", 32, 4, 1),
    ("x86_avx2_vpmulhw256", "i16x16", "i16x16", 32, 4, 2),
    ("x86_avx2_vpmulhuw256", "i16x16", "i16x16", 32, 4, 3),
    ("x86_avx2_vpmulhrsw256", "i16x16", "i16x16", 32, 4, 4),
    ("x86_avx2_vpmuldq256", "i32x8", "i64x4", 32, 4, 5),
    ("x86_avx2_vpmuludq256", "i32x8", "i64x4", 32, 4, 6),
    ("x86_avx2_vpsadbw256", "i8x32", "i64x4", 32, 4, 7),
];
fn supports(f: Features, t: u8) -> bool {
    match t {
        0 => true,
        1 => f.ssse3,
        2 => f.sse41,
        3 => f.avx,
        4 => f.avx && f.avx2,
        _ => false,
    }
}
fn feature_sets() -> Vec<(&'static str, Features, Vec<usize>)> {
    let mut ssse3 = Features::NONE;
    ssse3.ssse3 = true;
    let mut sse41 = Features::NONE;
    sse41.sse41 = true;
    let mut avx = Features::NONE;
    avx = avx.with_features("avx").unwrap();
    let mut avx2 = avx;
    avx2.avx2 = true;
    [
        ("strict-v1", Features::NONE),
        ("bare-ssse3", ssse3),
        ("bare-sse41", sse41),
        ("bare-avx", avx),
        ("bare-avx2", avx2),
        ("v2", Features::for_level(CpuLevel::V2)),
        ("v3", Features::for_level(CpuLevel::V3)),
    ]
    .into_iter()
    .map(|(label, f)| {
        (
            label,
            f,
            OPS.iter()
                .enumerate()
                .filter_map(|(i, r)| supports(f, r.4).then_some(i))
                .collect(),
        )
    })
    .collect()
}
fn source(indices: &[usize]) -> String {
    let mut s = String::new();
    for &i in indices {
        let (op, it, ot, n, _, _) = OPS[i];
        for alias in [false, true] {
            s += &format!(
                "func @pair_{i}_{}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{it} align 1 v0\n v4 = load.{it} align 1 v1\n v5 = intrinsic {op}(v3,{})\n store align 1 v5,v2\n v6 = iconst.i64 {n}\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
                u8::from(alias),
                if alias { "v3" } else { "v4" }
            );
            let _ = ot;
        }
    }
    s
}
fn compile(
    src: &str,
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    compile_module(&optimized(src), s, a, p, f)
}
fn optimized(src: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    m
}
fn compile_module(
    m: &keel_ir::Module,
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, p, &pipeline)?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, p, &pipeline)?;
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        obj,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((obj, text))
}
fn harness(indices: &[usize]) -> String {
    let mut c = String::from(
        "typedef unsigned char u8;typedef unsigned long long u64;typedef long long s64;extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern void*memcpy(void*,const void*,__SIZE_TYPE__);extern int puts(const char*);extern int printf(const char*,...);extern int getpagesize(void);extern void*mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);\n",
    );
    for &i in indices {
        for alias in 0..2 {
            c += &format!("extern void pair_{i}_{alias}(const void*,const void*,void*);\n");
        }
    }
    c += "struct Case{void(*fn)(const void*,const void*,void*);unsigned n,family,alias;};static const struct Case cases[]={\n";
    for &i in indices {
        let (_, _, _, n, _, family) = OPS[i];
        for alias in 0..2 {
            c += &format!("{{pair_{i}_{alias},{n},{family},{alias}}},\n");
        }
    }
    c += "};\n";
    c += r#"
static u64 read(const u8*p,unsigned n){u64 x=0;memcpy(&x,p,n);return x;}
static s64 sx(u64 x,unsigned bits){u64 m=1ULL<<(bits-1);return (s64)(x&(m-1))-(s64)(x&m);}
static unsigned csr(void){unsigned x;__asm__ volatile("stmxcsr %0":"=m"(x));return x;}
static void reference(const struct Case*c,const u8*a,const u8*b,u8*o){const u8*z=c->alias?a:b;unsigned step=c->family==0?4:c->family<=4?2:8;for(unsigned i=0;i<c->n;i+=step){u64 v=0;switch(c->family){case 0:v=(u64)(sx(read(a+i,2),16)*sx(read(z+i,2),16))+(u64)(sx(read(a+i+2,2),16)*sx(read(z+i+2,2),16));break;case 1:{s64 x=(s64)a[i]*sx(z[i],8)+(s64)a[i+1]*sx(z[i+1],8);if(x>32767)x=32767;if(x< -32768)x= -32768;v=(u64)x;break;}case 2:v=((u64)(sx(read(a+i,2),16)*sx(read(z+i,2),16)))>>16;break;case 3:v=(read(a+i,2)*read(z+i,2))>>16;break;case 4:v=((u64)(sx(read(a+i,2),16)*sx(read(z+i,2),16)+16384))>>15;break;case 5:v=(u64)(sx(read(a+i,4),32)*sx(read(z+i,4),32));break;case 6:v=read(a+i,4)*read(z+i,4);break;case 7:for(unsigned j=0;j<8;j++)v+=a[i+j]>z[i+j]?a[i+j]-z[i+j]:z[i+j]-a[i+j];break;}memcpy(o+i,&v,step);}memcpy(o+c->n,a,c->n);memcpy(o+2*c->n,b,c->n);}
static const unsigned edge[]={0,65535,32768,32767,16384,49152,1,65534,128,65408,255,256,21845,43690,254,32769};
int main(void){unsigned state=csr();u64 rng=0x9a714823;for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<512;pattern++)for(unsigned alias=0;alias<3;alias++){u8 aa[256],bb[256],oo[256],expected[96];memset(aa,0x5a,256);memset(bb,0x5a,256);memset(oo,0x5a,256);unsigned off=16+(pattern&15);u8*a=aa+off,*b=bb+off,*o=(alias==0?oo:alias==1?aa:bb)+off;for(unsigned i=0;i<c->n;i+=2){u64 x,y;if(pattern<256){x=edge[pattern>>4];y=edge[pattern&15];}else{rng=rng*6364136223846793005ULL+1;x=rng>>32;rng=rng*6364136223846793005ULL+1;y=rng>>32;}memcpy(a+i,&x,2);memcpy(b+i,&y,2);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n)||o[-1]!=0x5a||o[3*c->n]!=0x5a||csr()!=state){printf("FAIL packed pair case=%u family=%u pattern=%u alias=%u\n",ci,c->family,pattern,alias);return 10;}}
 unsigned long page=(unsigned)getpagesize();u8*maps[3];for(unsigned i=0;i<3;i++){maps[i]=mmap(0,page*3,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 20;}for(unsigned side=0;side<2;side++){u8*a=maps[0]+page+(side?page-c->n:0),*b=maps[1]+page+(side?page-c->n:0),*o=maps[2]+page+(side?page-3*c->n:0),expected[96];for(unsigned i=0;i<c->n;i++){a[i]=(u8)(i*7+1);b[i]=(u8)(255-i*11);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n))return 21;}for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 22;
 }puts("PASS packed pairs saturation rounding signedness wrapping groups aliases guards");return 0;}
"#;
    c
}
#[test]
fn packed_pair_features_types_and_encodings() {
    let all: Vec<_> = (0..OPS.len()).collect();
    for (i, (_, _, _, _, tier, _)) in OPS.iter().enumerate() {
        let src = source(&[i]);
        let m = keel_ir::parse(&src).unwrap();
        assert!(keel_codegen::aarch64::compile(&m).is_err());
        let id = keel_ir::Intrinsic::from_name(OPS[i].0).unwrap();
        assert!(id.info().pure && id.is_x86_target());
        if *tier != 0 {
            let mut missing = Features::for_level(CpuLevel::V3);
            match tier {
                1 => missing.ssse3 = false,
                2 => missing.sse41 = false,
                3 => missing.avx = false,
                4 => missing.avx2 = false,
                _ => unreachable!(),
            }
            assert!(
                compile(
                    &src,
                    SelectionTier::Full,
                    AllocationTier::Full,
                    &EMPTY,
                    missing
                )
                .is_err()
            );
        }
        if *tier == 4 {
            let mut noavx = Features::NONE;
            noavx.avx2 = true;
            assert!(
                compile(
                    &src,
                    SelectionTier::Full,
                    AllocationTier::Full,
                    &EMPTY,
                    noavx
                )
                .is_err()
            );
        }
    }
    for src in [
        "func @bad(i8x16,i16x8)->i16x8 {\nblock0(v0:i8x16,v1:i16x8):\n v2 = intrinsic x86_ssse3_pmaddubsw128(v0,v1)\n return v2\n}",
        "func @bad(i16x8)->i16x8 {\nblock0(v0:i16x8):\n v1 = intrinsic x86_sse2_pmaddwd128(v0,v0)\n return v1\n}",
    ] {
        assert!(keel_ir::parse(src).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    let unused = "func @unused(i16x8)->i16x8 {\nblock0(v0:i16x8):\n v1 = intrinsic x86_sse2_pmaddwd128(v0,v0)\n return v0\n}";
    let (_, text) = compile(
        unused,
        SelectionTier::Full,
        AllocationTier::Full,
        &EMPTY,
        Features::NONE,
    )
    .unwrap();
    assert!(
        !text
            .lines()
            .any(|l| l.split_whitespace().next() == Some("pmaddwd"))
    );
    let export = std::env::var_os("KEEL_X86_PACKED_PAIR_EXPORT_DIR").map(std::path::PathBuf::from);
    let d = export
        .clone()
        .unwrap_or_else(|| common::temp("packed-pairs"));
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), harness(&all)).unwrap();
    fs::write(d.join("test.ir"), source(&all)).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    for (label, f, indices) in feature_sets() {
        let m = optimized(&source(&indices));
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for (spill, p) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
                    let (o, t) = compile_module(&m, s, a, p, f).unwrap();
                    fs::write(d.join(format!("{label}-{s:?}-{a:?}-{spill}.o")), o).unwrap();
                    fs::write(d.join(format!("{label}-{s:?}-{a:?}-{spill}.s")), t).unwrap();
                }
            }
        }
    }
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    asm += "pmaddwd xmm13,xmm14\n";
    asm += "pmaddubsw xmm13,xmm14\n";
    asm += "pmulhw xmm13,xmm14\n";
    asm += "pmulhuw xmm13,xmm14\n";
    asm += "pmulhrsw xmm13,xmm14\n";
    asm += "pmuldq xmm13,xmm14\n";
    asm += "pmuludq xmm13,xmm14\n";
    asm += "psadbw xmm13,xmm14\n";
    asm += "vpmaddwd xmm13,xmm13,xmm14\n";
    asm += "vpmaddubsw xmm13,xmm13,xmm14\n";
    asm += "vpmulhw xmm13,xmm13,xmm14\n";
    asm += "vpmulhuw xmm13,xmm13,xmm14\n";
    asm += "vpmulhrsw xmm13,xmm13,xmm14\n";
    asm += "vpmuldq xmm13,xmm13,xmm14\n";
    asm += "vpmuludq xmm13,xmm13,xmm14\n";
    asm += "vpsadbw xmm13,xmm13,xmm14\n";
    asm += "vpmaddwd ymm13,ymm13,ymm14\n";
    asm += "vpmaddubsw ymm13,ymm13,ymm14\n";
    asm += "vpmulhw ymm13,ymm13,ymm14\n";
    asm += "vpmulhuw ymm13,ymm13,ymm14\n";
    asm += "vpmulhrsw ymm13,ymm13,ymm14\n";
    asm += "vpmuldq ymm13,ymm13,ymm14\n";
    asm += "vpmuludq ymm13,ymm13,ymm14\n";
    asm += "vpsadbw ymm13,ymm13,ymm14\n";

    fs::write(d.join("encoding.s"), &asm).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(d.join("encoding.s"))
            .arg("-o")
            .arg(d.join("encoding.o")),
    );
    use object::{Object, ObjectSection};
    let n = fs::read(d.join("encoding.o")).unwrap();
    let own = keel_asm::x86::assemble_object_with_features(&asm, Features::for_level(CpuLevel::V3))
        .unwrap();
    let n = object::File::parse(&*n).unwrap();
    let o = object::File::parse(&*own).unwrap();
    assert_eq!(
        n.section_by_name(".text").unwrap().data().unwrap(),
        o.section_by_name(".text").unwrap().data().unwrap()
    );
    if export.is_none() {
        fs::remove_dir_all(d).unwrap();
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_packed_pair_integer_reference_and_guards() {
    let d = common::temp("packed-pairs-native");
    fs::create_dir_all(&d).unwrap();
    let mut runs = 0;
    for (label, f, indices) in feature_sets() {
        let m = optimized(&source(&indices));
        fs::write(d.join("test.c"), harness(&indices)).unwrap();
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    let (o, _) = compile_module(&m, s, a, p, f).unwrap();
                    fs::write(d.join("test.o"), o).unwrap();
                    common::command(
                        Command::new("clang")
                            .args([
                                "-O2",
                                "-march=x86-64",
                                "-mno-avx",
                                "-fno-vectorize",
                                "-fno-slp-vectorize",
                            ])
                            .arg(d.join("test.c"))
                            .arg(d.join("test.o"))
                            .arg("-o")
                            .arg(d.join("test")),
                    );
                    common::command(&mut Command::new(d.join("test")));
                    runs += 1;
                    println!("PASS {label} {s:?} {a:?} config{runs}");
                }
            }
        }
    }
    assert_eq!(runs, 84);
    fs::remove_dir_all(d).unwrap();
}

// Valid custom plans make destination/input overlap and mixed storage explicit.
// The ordinary live-leaf corpus cannot coalesce its destination with either leaf.
struct PairLocations(u8, AllocationTier);
impl keel_codegen::regalloc::RegisterAllocator for PairLocations {
    fn name(&self) -> &'static str {
        "packed-pair-controlled"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_codegen::regalloc::{AllocationInput, Loc};
        let spill_input = AllocationInput {
            func: input.func,
            pools: &EMPTY,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        };
        let mut plan = self.1.allocate(&spill_input);
        let a = keel_ir::Value::new(3);
        let b = keel_ir::Value::new(4);
        let d = keel_ir::Value::new(5);
        assert!(matches!(plan.loc(a), Some(Loc::Stack(_))));
        if input.func.name.ends_with("_0") {
            assert!(matches!(plan.loc(b), Some(Loc::Stack(_))));
        }
        assert!(matches!(plan.loc(d), Some(Loc::Stack(_))));
        let r0 = Loc::FReg(0);
        let r1 = Loc::FReg(1);
        let assignments: &[(keel_ir::Value, Loc)] = match self.0 {
            0 => &[(a, r0), (b, r1), (d, r0)],
            1 => &[(a, r0), (b, r1), (d, r1)],
            2 => &[(b, r1), (d, r0)],
            3 => &[(a, r0), (b, r1)],
            4 => &[(a, r0), (d, r0)],
            5 => &[(b, r0), (d, r0)],
            _ => unreachable!(),
        };
        for &(v, loc) in assignments {
            if plan.loc(v).is_some() {
                plan.locs[0][v.index()] = Some(loc);
            }
        }
        plan.validate(input).unwrap();
        plan
    }
}
fn dying_source(indices: &[usize]) -> String {
    let mut s = String::new();
    for &i in indices {
        let (op, it, _, _, _, _) = OPS[i];
        for alias in [false, true] {
            s += &format!(
                "func @pair_{i}_{}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{it} align 1 v0\n v4 = load.{it} align 1 v1\n v5 = intrinsic {op}(v3,{})\n store align 1 v5,v2\n return\n}}\n",
                u8::from(alias),
                if alias { "v3" } else { "v4" }
            );
        }
    }
    s
}
fn controlled_pair(
    s: SelectionTier,
    a: AllocationTier,
    mode: u8,
    m: &keel_ir::Module,
    f: Features,
) -> (Vec<u8>, String) {
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let allocator = PairLocations(mode, a);
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocator,
    };
    let text =
        keel_codegen::x86_64::compile_with_pipeline(m, &keel_codegen::x86_64::REGISTERS, &pipeline)
            .unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        m,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    (object, text)
}
fn dying_harness(indices: &[usize]) -> String {
    harness(indices)
        .replace("memcpy(o+c->n,a,c->n);memcpy(o+2*c->n,b,c->n);", "")
        .replace("memcmp(o,expected,3*c->n)", "memcmp(o,expected,c->n)")
        .replace("o[3*c->n]", "o[c->n]")
        .replace("side?page-3*c->n:0", "side?page-c->n:0")
}
#[test]
fn packed_pair_controlled_destination_and_mixed_spill_exports() {
    let all = (0..OPS.len()).collect::<Vec<_>>();
    let src = dying_source(&all);
    let export =
        std::env::var_os("KEEL_X86_PACKED_PAIR_ALIAS_EXPORT_DIR").map(std::path::PathBuf::from);
    let dir = export
        .clone()
        .unwrap_or_else(|| common::temp("packed-pair-alias"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("test.c"), dying_harness(&all)).unwrap();
    fs::write(dir.join("test.ir"), &src).unwrap();
    let m = optimized(&src);
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(dir.join("test.c")),
    );
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for mode in 0..6 {
                let (o, t) = controlled_pair(s, a, mode, &m, Features::for_level(CpuLevel::V3));
                let stem = format!("{s:?}-{a:?}-{mode}");
                fs::write(dir.join(format!("{stem}.o")), o).unwrap();
                fs::write(dir.join(format!("{stem}.s")), t).unwrap();
            }
        }
    }
    if export.is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_packed_pair_controlled_register_reuse_and_mixed_spills() {
    let dir = common::temp("packed-pair-alias-native");
    fs::create_dir_all(&dir).unwrap();
    let mut runs = 0;
    for (_, f, indices) in feature_sets() {
        let m = optimized(&dying_source(&indices));
        fs::write(dir.join("test.c"), dying_harness(&indices)).unwrap();
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for mode in 0..6 {
                    let (o, _) = controlled_pair(s, a, mode, &m, f);
                    fs::write(dir.join("test.o"), o).unwrap();
                    common::command(
                        Command::new("clang")
                            .args([
                                "-O2",
                                "-march=x86-64",
                                "-mno-avx",
                                "-fno-vectorize",
                                "-fno-slp-vectorize",
                            ])
                            .arg(dir.join("test.c"))
                            .arg(dir.join("test.o"))
                            .arg("-o")
                            .arg(dir.join("test")),
                    );
                    common::command(&mut Command::new(dir.join("test")));
                    runs += 1;
                }
            }
        }
    }
    assert_eq!(runs, 252);
    fs::remove_dir_all(dir).unwrap();
}

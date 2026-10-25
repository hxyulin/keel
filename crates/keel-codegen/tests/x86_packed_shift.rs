//! Architectural count semantics, independent of portable modulo shifts.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
// name, data shape, lane bits, total bytes, per-lane count, feature tier, mode.
const OPS: &[(&str, &str, usize, usize, bool, u8, u8)] = &[
    ("x86_sse2_psllw128", "i16x8", 16, 16, false, 0, 0),
    ("x86_sse2_pslld128", "i32x4", 32, 16, false, 0, 0),
    ("x86_sse2_psllq128", "i64x2", 64, 16, false, 0, 0),
    ("x86_sse2_psrlw128", "i16x8", 16, 16, false, 0, 1),
    ("x86_sse2_psrld128", "i32x4", 32, 16, false, 0, 1),
    ("x86_sse2_psrlq128", "i64x2", 64, 16, false, 0, 1),
    ("x86_sse2_psraw128", "i16x8", 16, 16, false, 0, 2),
    ("x86_sse2_psrad128", "i32x4", 32, 16, false, 0, 2),
    ("x86_avx_vpsllw128", "i16x8", 16, 16, false, 1, 0),
    ("x86_avx_vpslld128", "i32x4", 32, 16, false, 1, 0),
    ("x86_avx_vpsllq128", "i64x2", 64, 16, false, 1, 0),
    ("x86_avx_vpsrlw128", "i16x8", 16, 16, false, 1, 1),
    ("x86_avx_vpsrld128", "i32x4", 32, 16, false, 1, 1),
    ("x86_avx_vpsrlq128", "i64x2", 64, 16, false, 1, 1),
    ("x86_avx_vpsraw128", "i16x8", 16, 16, false, 1, 2),
    ("x86_avx_vpsrad128", "i32x4", 32, 16, false, 1, 2),
    ("x86_avx2_vpsllw256", "i16x16", 16, 32, false, 2, 0),
    ("x86_avx2_vpslld256", "i32x8", 32, 32, false, 2, 0),
    ("x86_avx2_vpsllq256", "i64x4", 64, 32, false, 2, 0),
    ("x86_avx2_vpsrlw256", "i16x16", 16, 32, false, 2, 1),
    ("x86_avx2_vpsrld256", "i32x8", 32, 32, false, 2, 1),
    ("x86_avx2_vpsrlq256", "i64x4", 64, 32, false, 2, 1),
    ("x86_avx2_vpsraw256", "i16x16", 16, 32, false, 2, 2),
    ("x86_avx2_vpsrad256", "i32x8", 32, 32, false, 2, 2),
    ("x86_avx2_vpsllvd128", "i32x4", 32, 16, true, 2, 0),
    ("x86_avx2_vpsllvq128", "i64x2", 64, 16, true, 2, 0),
    ("x86_avx2_vpsrlvd128", "i32x4", 32, 16, true, 2, 1),
    ("x86_avx2_vpsrlvq128", "i64x2", 64, 16, true, 2, 1),
    ("x86_avx2_vpsravd128", "i32x4", 32, 16, true, 2, 2),
    ("x86_avx2_vpsllvd256", "i32x8", 32, 32, true, 2, 0),
    ("x86_avx2_vpsllvq256", "i64x4", 64, 32, true, 2, 0),
    ("x86_avx2_vpsrlvd256", "i32x8", 32, 32, true, 2, 1),
    ("x86_avx2_vpsrlvq256", "i64x4", 64, 32, true, 2, 1),
    ("x86_avx2_vpsravd256", "i32x8", 32, 32, true, 2, 2),
];
fn source(indices: &[usize]) -> String {
    let mut s = String::new();
    for &i in indices {
        let (op, t, _, n, per_lane, _, _) = OPS[i];
        let ct = if per_lane { t } else { "i64" };
        let count_bytes = if per_lane { n } else { 8 };
        for alias in [false, true] {
            s += &format!(
                "func @shift_{i}_{}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{t} align 1 v0\n v4 = load.{ct} align 1 v1\n v5 = intrinsic {op}(v3,{})\n store align 1 v5,v2\n v6 = iconst.i64 {n}\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
                u8::from(alias),
                if per_lane && alias { "v3" } else { "v4" }
            );
            assert!(count_bytes <= n);
        }
    }
    s
}
fn feature_sets() -> Vec<(&'static str, Features, Vec<usize>)> {
    let mut avx = Features::NONE;
    avx = avx.with_features("avx").unwrap();
    let mut avx2 = avx;
    avx2.avx2 = true;
    vec![
        (
            "strict-v1",
            Features::NONE,
            OPS.iter()
                .enumerate()
                .filter_map(|(i, r)| (r.5 == 0).then_some(i))
                .collect(),
        ),
        (
            "bare-avx",
            avx,
            OPS.iter()
                .enumerate()
                .filter_map(|(i, r)| (r.5 <= 1).then_some(i))
                .collect(),
        ),
        ("bare-avx2", avx2, (0..OPS.len()).collect()),
        (
            "v3",
            Features::for_level(CpuLevel::V3),
            (0..OPS.len()).collect(),
        ),
    ]
}
fn compile(
    src: &str,
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline)?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline)?;
    let o = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        o,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((o, text))
}
fn harness(indices: &[usize]) -> String {
    let mut c = String::from(
        "typedef unsigned char u8;typedef unsigned long long u64;extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern void*memcpy(void*,const void*,__SIZE_TYPE__);extern int puts(const char*);extern int printf(const char*,...);extern int getpagesize(void);extern void*mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);\n",
    );
    for &i in indices {
        for alias in 0..2 {
            c += &format!("extern void shift_{i}_{alias}(const void*,const void*,void*);\n");
        }
    }
    c += "struct Case{void(*fn)(const void*,const void*,void*);unsigned bits,n,per_lane,mode,alias;};static const struct Case cases[]={\n";
    for &i in indices {
        let (_, _, bits, n, per, _, mode) = OPS[i];
        for alias in 0..2 {
            c += &format!(
                "{{shift_{i}_{alias},{bits},{n},{},{mode},{alias}}},\n",
                u8::from(per)
            );
        }
    }
    c += "};\n";
    c += r#"
static u64 read(const u8*p,unsigned n){u64 x=0;memcpy(&x,p,n);return x;}
static unsigned csr(void){unsigned x;__asm__ volatile("stmxcsr %0":"=m"(x));return x;}
static void reference(const struct Case*c,const u8*a,const u8*b,u8*o){unsigned bytes=c->bits/8;u64 mask=c->bits==64?~0ULL:(1ULL<<c->bits)-1;for(unsigned i=0;i<c->n;i+=bytes){u64 x=read(a+i,bytes),count=c->per_lane?read((c->alias?a:b)+i,bytes):read(b,8),y;if(count>=c->bits)y=(c->mode==2&&(x>>(c->bits-1)))?mask:0;else if(!count)y=x;else if(c->mode==0)y=(x<<count)&mask;else if(c->mode==1)y=x>>count;else y=(x>>count)|((x>>(c->bits-1))?(mask^(mask>>count)):0);memcpy(o+i,&y,bytes);}memcpy(o+c->n,a,c->n);memcpy(o+2*c->n,b,c->per_lane?c->n:8);}
static const u64 counts[]={0,1,7,8,15,16,17,31,32,33,63,64,65,127,128,255,256,65535,65536,0xffffffffULL,0x100000000ULL,0x8000000000000000ULL,~0ULL};
int main(void){unsigned state=csr();u64 rng=0x714235f8;for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){const struct Case*c=&cases[ci];unsigned cb=c->per_lane?c->n:8,output=2*c->n+cb;for(unsigned pattern=0;pattern<256;pattern++)for(unsigned ck=0;ck<sizeof(counts)/sizeof(*counts);ck++)for(unsigned alias=0;alias<3;alias++){u8 aa[256],bb[256],oo[256],expected[96];memset(aa,0x5a,256);memset(bb,0x5a,256);memset(oo,0x5a,256);unsigned off=16+(pattern&15);u8*a=aa+off,*b=bb+off,*o=(alias==0?oo:alias==1?aa:bb)+off;for(unsigned i=0;i<c->n;i++){rng=rng*6364136223846793005ULL+1;a[i]=pattern==0?0:pattern==1?255:pattern==2?128:pattern==3?1:(u8)(rng>>32);}for(unsigned i=0;i<cb;i+=c->per_lane?c->bits/8:8){u64 count=counts[(ck+(c->per_lane?i:0))%(sizeof(counts)/sizeof(*counts))];memcpy(b+i,&count,c->per_lane?c->bits/8:8);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,output)||o[-1]!=0x5a||o[output]!=0x5a||csr()!=state){printf("FAIL packed shift case=%u pattern=%u count=%u alias=%u\n",ci,pattern,ck,alias);return 10;}}
 unsigned long page=(unsigned)getpagesize();u8*maps[3];for(unsigned i=0;i<3;i++){maps[i]=mmap(0,page*3,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 20;}for(unsigned edge=0;edge<2;edge++){u8*a=maps[0]+page+(edge?page-c->n:0),*b=maps[1]+page+(edge?page-cb:0),*o=maps[2]+page+(edge?page-output:0),expected[96];for(unsigned i=0;i<c->n;i++)a[i]=(u8)(7*i+1);memset(b,0xff,cb);reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,output))return 21;}for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 22;
 }puts("PASS packed shifts full unsigned counts aliases and exact footprints");return 0;}
"#;
    c
}
#[test]
fn packed_shift_features_semantics_and_encodings() {
    let all: Vec<_> = (0..OPS.len()).collect();
    for (op, _, _, _, _, _, _) in OPS {
        let id: keel_ir::Intrinsic = keel_ir::Intrinsic::from_name(op).unwrap();
        assert!(id.info().pure && id.is_x86_target());
    }
    for bad in [
        "func @bad(i16x8,i32)->i16x8 {\nblock0(v0:i16x8,v1:i32):\n v2 = intrinsic x86_sse2_psllw128(v0,v1)\n return v2\n}\n",
        "func @bad(i32x4,i64)->i32x4 {\nblock0(v0:i32x4,v1:i64):\n v2 = intrinsic x86_avx2_vpsllvd128(v0,v1)\n return v2\n}\n",
    ] {
        assert!(keel_ir::parse(bad).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    let unused = "func @unused(i16x8,i64)->i16x8 {\nblock0(v0:i16x8,v1:i64):\n v2 = intrinsic x86_sse2_psllw128(v0,v1)\n return v0\n}\n";
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
            .any(|l| l.split_whitespace().next() == Some("psllw"))
    );

    for (i, (_, _, _, _, _, tier, _)) in OPS.iter().enumerate() {
        let mut f = Features::NONE;
        f.avx = *tier != 0;
        f.avx2 = *tier == 2;
        f.xsave = f.avx;
        f.osxsave = f.avx;
        compile(
            &source(&[i]),
            SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY,
            f,
        )
        .unwrap();
        if *tier != 0 {
            assert!(
                compile(
                    &source(&[i]),
                    SelectionTier::Full,
                    AllocationTier::Full,
                    &EMPTY,
                    Features::NONE
                )
                .is_err()
            );
        }
        if *tier == 2 {
            f.avx2 = false;
            assert!(
                compile(
                    &source(&[i]),
                    SelectionTier::Full,
                    AllocationTier::Full,
                    &EMPTY,
                    f
                )
                .is_err()
            );
        }
    }
    assert!(keel_codegen::aarch64::compile(&keel_ir::parse(&source(&all)).unwrap()).is_err());
    let export = std::env::var_os("KEEL_X86_PACKED_SHIFT_EXPORT_DIR").map(std::path::PathBuf::from);
    let d = export
        .clone()
        .unwrap_or_else(|| common::temp("packed-shift"));
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("test.c"),
        harness(&(0..OPS.len()).collect::<Vec<_>>()),
    )
    .unwrap();
    fs::write(d.join("test.ir"), source(&all)).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (spill, p) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
                let (o, text) =
                    compile(&source(&all), s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join(format!("{s:?}-{a:?}-{spill}.o")), o).unwrap();
                fs::write(d.join(format!("{s:?}-{a:?}-{spill}.s")), text).unwrap();
            }
        }
    }
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    asm += "psllw xmm13,xmm14\n";
    asm += "pslld xmm13,xmm14\n";
    asm += "psllq xmm13,xmm14\n";
    asm += "psrlw xmm13,xmm14\n";
    asm += "psrld xmm13,xmm14\n";
    asm += "psrlq xmm13,xmm14\n";
    asm += "psraw xmm13,xmm14\n";
    asm += "psrad xmm13,xmm14\n";
    asm += "vpsllw xmm13,xmm13,xmm14\n";
    asm += "vpslld xmm13,xmm13,xmm14\n";
    asm += "vpsllq xmm13,xmm13,xmm14\n";
    asm += "vpsrlw xmm13,xmm13,xmm14\n";
    asm += "vpsrld xmm13,xmm13,xmm14\n";
    asm += "vpsrlq xmm13,xmm13,xmm14\n";
    asm += "vpsraw xmm13,xmm13,xmm14\n";
    asm += "vpsrad xmm13,xmm13,xmm14\n";
    asm += "vpsllw ymm13,ymm13,xmm14\n";
    asm += "vpslld ymm13,ymm13,xmm14\n";
    asm += "vpsllq ymm13,ymm13,xmm14\n";
    asm += "vpsrlw ymm13,ymm13,xmm14\n";
    asm += "vpsrld ymm13,ymm13,xmm14\n";
    asm += "vpsrlq ymm13,ymm13,xmm14\n";
    asm += "vpsraw ymm13,ymm13,xmm14\n";
    asm += "vpsrad ymm13,ymm13,xmm14\n";
    asm += "vpsllvd xmm13,xmm13,xmm14\n";
    asm += "vpsllvq xmm13,xmm13,xmm14\n";
    asm += "vpsrlvd xmm13,xmm13,xmm14\n";
    asm += "vpsrlvq xmm13,xmm13,xmm14\n";
    asm += "vpsravd xmm13,xmm13,xmm14\n";
    asm += "vpsllvd ymm13,ymm13,ymm14\n";
    asm += "vpsllvq ymm13,ymm13,ymm14\n";
    asm += "vpsrlvd ymm13,ymm13,ymm14\n";
    asm += "vpsrlvq ymm13,ymm13,ymm14\n";
    asm += "vpsravd ymm13,ymm13,ymm14\n";

    fs::write(d.join("encoding.s"), &asm).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(d.join("encoding.s"))
            .arg("-o")
            .arg(d.join("encoding.o")),
    );
    use object::{Object, ObjectSection};
    let native = fs::read(d.join("encoding.o")).unwrap();
    let own = keel_asm::x86::assemble_object_with_features(&asm, Features::for_level(CpuLevel::V3))
        .unwrap();
    let n = object::File::parse(&*native).unwrap();
    let o = object::File::parse(&*own).unwrap();
    assert_eq!(
        n.section_by_name(".text").unwrap().data().unwrap(),
        o.section_by_name(".text").unwrap().data().unwrap()
    );
    if export.is_none() {
        fs::remove_dir_all(d).unwrap();
    }
}
#[test]
fn strict_feature_matrices_and_each_foreign_operation() {
    for (i, (_, _, _, _, _, tier, _)) in OPS.iter().enumerate() {
        let module = keel_ir::parse(&source(&[i])).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(
            keel_codegen::aarch64::compile(&module).is_err(),
            "ARM must reject operation{i}"
        );
        assert!(
            keel_ir::Intrinsic::from_name(OPS[i].0)
                .unwrap()
                .is_x86_target()
        );
        if *tier != 0 {
            let mut missing = Features::NONE;
            missing.avx2 = true;
            assert!(
                compile(
                    &source(&[i]),
                    SelectionTier::Full,
                    AllocationTier::Full,
                    &EMPTY,
                    missing
                )
                .is_err()
            );
        }
    }
    for name in [
        "x86_ssse3_pshufb128",
        "x86_avx_vpshufb128",
        "x86_avx2_vpshufb256",
    ] {
        assert!(keel_ir::Intrinsic::from_name(name).unwrap().is_x86_target());
    }
    for (label, f, indices) in feature_sets() {
        let src = source(&indices);
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    compile(&src, s, a, p, f).unwrap_or_else(|e| panic!("{label}:{e:?}"));
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_packed_shift_counts_aliases_and_guard_pages() {
    let d = common::temp("shift-strict-native");
    fs::create_dir_all(&d).unwrap();
    let mut runs = 0;
    for (label, f, indices) in feature_sets() {
        let src = source(&indices);
        fs::write(d.join("test.c"), harness(&indices)).unwrap();
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    let (o, _) = compile(&src, s, a, p, f).unwrap();
                    fs::write(d.join("test.o"), o).unwrap();
                    common::command(
                        Command::new("clang")
                            .args(["-O2", "-march=x86-64", "-mno-avx"])
                            .arg(d.join("test.c"))
                            .arg(d.join("test.o"))
                            .arg("-o")
                            .arg(d.join("test")),
                    );
                    common::command(&mut Command::new(d.join("test")));
                    runs += 1;
                    println!("PASS {label} {s:?} {a:?} configuration{runs}");
                }
            }
        }
    }
    assert_eq!(runs, 48);
    fs::remove_dir_all(d).unwrap();
}

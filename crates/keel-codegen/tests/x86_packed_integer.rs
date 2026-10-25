//! Exact horizontal lane order, staged saturation, sign, abs and rounded average.
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
// Name, shape, bytes, feature tier, family, lane bits, arity.
const OPS: &[(&str, &str, usize, u8, u8, u8, u8)] = &[
    ("x86_ssse3_phaddw128", "i16x8", 16, 1, 0, 16, 2),
    ("x86_ssse3_phaddd128", "i32x4", 16, 1, 1, 32, 2),
    ("x86_ssse3_phaddsw128", "i16x8", 16, 1, 2, 16, 2),
    ("x86_ssse3_phsubw128", "i16x8", 16, 1, 3, 16, 2),
    ("x86_ssse3_phsubd128", "i32x4", 16, 1, 4, 32, 2),
    ("x86_ssse3_phsubsw128", "i16x8", 16, 1, 5, 16, 2),
    ("x86_sse2_paddsb128", "i8x16", 16, 0, 6, 8, 2),
    ("x86_sse2_paddsw128", "i16x8", 16, 0, 7, 16, 2),
    ("x86_sse2_paddusb128", "i8x16", 16, 0, 8, 8, 2),
    ("x86_sse2_paddusw128", "i16x8", 16, 0, 9, 16, 2),
    ("x86_sse2_psubsb128", "i8x16", 16, 0, 10, 8, 2),
    ("x86_sse2_psubsw128", "i16x8", 16, 0, 11, 16, 2),
    ("x86_sse2_psubusb128", "i8x16", 16, 0, 12, 8, 2),
    ("x86_sse2_psubusw128", "i16x8", 16, 0, 13, 16, 2),
    ("x86_sse2_pavgb128", "i8x16", 16, 0, 14, 8, 2),
    ("x86_sse2_pavgw128", "i16x8", 16, 0, 15, 16, 2),
    ("x86_ssse3_psignb128", "i8x16", 16, 1, 16, 8, 2),
    ("x86_ssse3_psignw128", "i16x8", 16, 1, 17, 16, 2),
    ("x86_ssse3_psignd128", "i32x4", 16, 1, 18, 32, 2),
    ("x86_ssse3_pabsb128", "i8x16", 16, 1, 19, 8, 1),
    ("x86_ssse3_pabsw128", "i16x8", 16, 1, 20, 16, 1),
    ("x86_ssse3_pabsd128", "i32x4", 16, 1, 21, 32, 1),
    ("x86_avx_vphaddw128", "i16x8", 16, 3, 0, 16, 2),
    ("x86_avx_vphaddd128", "i32x4", 16, 3, 1, 32, 2),
    ("x86_avx_vphaddsw128", "i16x8", 16, 3, 2, 16, 2),
    ("x86_avx_vphsubw128", "i16x8", 16, 3, 3, 16, 2),
    ("x86_avx_vphsubd128", "i32x4", 16, 3, 4, 32, 2),
    ("x86_avx_vphsubsw128", "i16x8", 16, 3, 5, 16, 2),
    ("x86_avx_vpaddsb128", "i8x16", 16, 3, 6, 8, 2),
    ("x86_avx_vpaddsw128", "i16x8", 16, 3, 7, 16, 2),
    ("x86_avx_vpaddusb128", "i8x16", 16, 3, 8, 8, 2),
    ("x86_avx_vpaddusw128", "i16x8", 16, 3, 9, 16, 2),
    ("x86_avx_vpsubsb128", "i8x16", 16, 3, 10, 8, 2),
    ("x86_avx_vpsubsw128", "i16x8", 16, 3, 11, 16, 2),
    ("x86_avx_vpsubusb128", "i8x16", 16, 3, 12, 8, 2),
    ("x86_avx_vpsubusw128", "i16x8", 16, 3, 13, 16, 2),
    ("x86_avx_vpavgb128", "i8x16", 16, 3, 14, 8, 2),
    ("x86_avx_vpavgw128", "i16x8", 16, 3, 15, 16, 2),
    ("x86_avx_vpsignb128", "i8x16", 16, 3, 16, 8, 2),
    ("x86_avx_vpsignw128", "i16x8", 16, 3, 17, 16, 2),
    ("x86_avx_vpsignd128", "i32x4", 16, 3, 18, 32, 2),
    ("x86_avx_vpabsb128", "i8x16", 16, 3, 19, 8, 1),
    ("x86_avx_vpabsw128", "i16x8", 16, 3, 20, 16, 1),
    ("x86_avx_vpabsd128", "i32x4", 16, 3, 21, 32, 1),
    ("x86_avx2_vphaddw256", "i16x16", 32, 4, 0, 16, 2),
    ("x86_avx2_vphaddd256", "i32x8", 32, 4, 1, 32, 2),
    ("x86_avx2_vphaddsw256", "i16x16", 32, 4, 2, 16, 2),
    ("x86_avx2_vphsubw256", "i16x16", 32, 4, 3, 16, 2),
    ("x86_avx2_vphsubd256", "i32x8", 32, 4, 4, 32, 2),
    ("x86_avx2_vphsubsw256", "i16x16", 32, 4, 5, 16, 2),
    ("x86_avx2_vpaddsb256", "i8x32", 32, 4, 6, 8, 2),
    ("x86_avx2_vpaddsw256", "i16x16", 32, 4, 7, 16, 2),
    ("x86_avx2_vpaddusb256", "i8x32", 32, 4, 8, 8, 2),
    ("x86_avx2_vpaddusw256", "i16x16", 32, 4, 9, 16, 2),
    ("x86_avx2_vpsubsb256", "i8x32", 32, 4, 10, 8, 2),
    ("x86_avx2_vpsubsw256", "i16x16", 32, 4, 11, 16, 2),
    ("x86_avx2_vpsubusb256", "i8x32", 32, 4, 12, 8, 2),
    ("x86_avx2_vpsubusw256", "i16x16", 32, 4, 13, 16, 2),
    ("x86_avx2_vpavgb256", "i8x32", 32, 4, 14, 8, 2),
    ("x86_avx2_vpavgw256", "i16x16", 32, 4, 15, 16, 2),
    ("x86_avx2_vpsignb256", "i8x32", 32, 4, 16, 8, 2),
    ("x86_avx2_vpsignw256", "i16x16", 32, 4, 17, 16, 2),
    ("x86_avx2_vpsignd256", "i32x8", 32, 4, 18, 32, 2),
    ("x86_avx2_vpabsb256", "i8x32", 32, 4, 19, 8, 1),
    ("x86_avx2_vpabsw256", "i16x16", 32, 4, 20, 16, 1),
    ("x86_avx2_vpabsd256", "i32x8", 32, 4, 21, 32, 1),
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
                .filter_map(|(i, r)| supports(f, r.3).then_some(i))
                .collect(),
        )
    })
    .collect()
}
fn source(indices: &[usize]) -> String {
    let mut s = String::new();
    for &i in indices {
        let (op, it, n, _, _, _, arity) = OPS[i];
        for alias in [false, true] {
            let args = if arity == 1 {
                "v3".into()
            } else {
                format!("v3,{}", if alias { "v3" } else { "v4" })
            };
            s += &format!(
                "func @pair_{i}_{}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{it} align 1 v0\n v4 = load.{it} align 1 v1\n v5 = intrinsic {op}({args})\n store align 1 v5,v2\n v6 = iconst.i64 {n}\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
                u8::from(alias)
            );
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
    c += "struct Case{void(*fn)(const void*,const void*,void*);unsigned n,family,alias,bits,arity;};static const struct Case cases[]={\n";
    for &i in indices {
        let (_, _, n, _, family, bits, arity) = OPS[i];
        for alias in 0..2 {
            c += &format!("{{pair_{i}_{alias},{n},{family},{alias},{bits},{arity}}},\n");
        }
    }
    c += "};\n";
    c += r#"
static u64 read(const u8*p,unsigned n){u64 x=0;memcpy(&x,p,n);return x;}
static s64 sx(u64 x,unsigned bits){u64 m=1ULL<<(bits-1);return (s64)(x&(m-1))-(s64)(x&m);}
static unsigned csr(void){unsigned x;__asm__ volatile("stmxcsr %0":"=m"(x));return x;}
static void reference(const struct Case*c,const u8*a,const u8*b,u8*o){
 const u8*z=c->alias?a:b;unsigned w=c->bits/8;u64 mask=(1ULL<<c->bits)-1;
 for(unsigned i=0;i<c->n;i+=w){u64 x=read(a+i,w),y=read(z+i,w),v=0; s64 sxv=sx(x,c->bits),sy=sx(y,c->bits),r=0;
  if(c->family<6){unsigned half=i/16, lane=(i%16)/w, pairs=8/w;const u8*p=lane<pairs?a:z;unsigned off=half*16+(lane%pairs)*2*w;
   r=sx(read(p+off,w),c->bits);s64 t=sx(read(p+off+w,w),c->bits);r=c->family<3?r+t:r-t;
   if(c->family==2||c->family==5){if(r>32767)r=32767;if(r< -32768)r= -32768;}v=(u64)r;
  }else if(c->family<14){unsigned sub=c->family>=10,uns=c->family==8||c->family==9||c->family==12||c->family==13;
   if(uns){if(sub)v=x<y?0:x-y;else{v=x+y;if(v>mask)v=mask;}}
   else{r=sub?sxv-sy:sxv+sy;s64 hi=(1LL<<(c->bits-1))-1,lo= -hi-1;if(r>hi)r=hi;if(r<lo)r=lo;v=(u64)r;}
  }else if(c->family<16){v=(x+y+1)>>1;}
  else if(c->family<19){v=sy<0?0-x:sy==0?0:x;}
  else{v=sxv<0?0-x:x;}
  v&=mask;memcpy(o+i,&v,w);
 }memcpy(o+c->n,a,c->n);memcpy(o+2*c->n,b,c->n);
}
static const unsigned edge[]={0,0xffffffffU,0x80000000U,0x7fffffffU,0x40000000U,0xc0000000U,1,0xfffffffeU,128,0xffffff80U,255,256,0x55555555U,0xaaaaaaaaU,254,0x80000001U};
int main(void){unsigned state=csr();u64 rng=0x9a714823;for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<528;pattern++)for(unsigned alias=0;alias<3;alias++){u8 aa[256],bb[256],oo[256],expected[96];memset(aa,0x5a,256);memset(bb,0x5a,256);memset(oo,0x5a,256);unsigned off=16+(pattern&15);u8*a=aa+off,*b=bb+off,*o=(alias==0?oo:alias==1?aa:bb)+off;for(unsigned i=0;i<c->n;i+=c->bits/8){u64 x,y;if(pattern<256){unsigned shift=32-c->bits;x=edge[pattern>>4]>>shift;y=edge[pattern&15]>>shift;}else if(pattern>=512){u64 top=1ULL<<(c->bits-1),max=top-1;unsigned lane=i/(c->bits/8);switch(pattern-512){case 0:x=lane&1?max:top;y=lane&1?top:max;break;case 1:x=max;y=max;break;case 2:x=lane&1?1:top;y=lane&1?top:1;break;case 3:x=lane&1?(top*2-1):max;y=lane&1?max:(top*2-1);break;default:x=(lane+1)*17+(i/16)*241+(pattern-512)*29;y=(lane+3)*31+(i/16)*503+(pattern-512)*37;break;}}else{rng=rng*6364136223846793005ULL+1;x=rng>>32;rng=rng*6364136223846793005ULL+1;y=rng>>32;}memcpy(a+i,&x,c->bits/8);memcpy(b+i,&y,c->bits/8);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n)||o[-1]!=0x5a||o[3*c->n]!=0x5a||csr()!=state){printf("FAIL packed pair case=%u family=%u pattern=%u alias=%u\n",ci,c->family,pattern,alias);return 10;}}
 unsigned long page=(unsigned)getpagesize();u8*maps[3];for(unsigned i=0;i<3;i++){maps[i]=mmap(0,page*3,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 20;}for(unsigned side=0;side<2;side++){u8*a=maps[0]+page+(side?page-c->n:0),*b=maps[1]+page+(side?page-c->n:0),*o=maps[2]+page+(side?page-3*c->n:0),expected[96];for(unsigned i=0;i<c->n;i++){a[i]=(u8)(i*7+1);b[i]=(u8)(255-i*11);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n))return 21;}for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 22;
 }puts("PASS packed integer horizontal saturation sign abs average aliases guards");return 0;}
"#;
    c
}
#[test]
fn packed_integer_features_types_and_encodings() {
    let all: Vec<_> = (0..OPS.len()).collect();
    for (i, (_, _, _, tier, _, _, _)) in OPS.iter().enumerate() {
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
                3 => missing = Features::NONE,
                4 => missing.avx2 = false,
                _ => unreachable!(),
            }
            missing.validate().unwrap();
            let error = compile(
                &src,
                SelectionTier::Full,
                AllocationTier::Full,
                &EMPTY,
                missing,
            )
            .unwrap_err();
            assert!(error.message.contains(OPS[i].0), "{error:?}");
            assert!(error.message.contains("requires its"), "{error:?}");
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
        "func @bad(i8x16,i16x8)->i16x8 {\nblock0(v0:i8x16,v1:i16x8):\n v2 = intrinsic x86_ssse3_phaddw128(v0,v1)\n return v2\n}",
        "func @bad(i32x4)->i32x4 {\nblock0(v0:i32x4):\n v1 = intrinsic x86_sse2_paddsw128(v0,v0)\n return v1\n}",
    ] {
        assert!(keel_ir::parse(src).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    for row in OPS {
        let id = keel_ir::Intrinsic::from_name(row.0).unwrap();
        let info = id.info();
        assert_eq!(info.params.len(), row.6 as usize);
        assert_eq!(info.returns.len(), 1);
        assert!(info.params.iter().all(|ty| *ty == info.returns[0]));
    }
    for text in [
        "func @bad(i8x16)->i8x16 {\nblock0(v0:i8x16):\n v1 = intrinsic x86_ssse3_pabsb128(v0,v0)\n return v1\n}",
        "func @bad(i16x8)->i16x8 {\nblock0(v0:i16x8):\n v1 = intrinsic x86_ssse3_pabsb128(v0)\n return v1\n}",
    ] {
        assert!(keel_ir::parse(text).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    let unused = "func @unused(i16x8)->i16x8 {\nblock0(v0:i16x8):\n v1 = intrinsic x86_sse2_paddsw128(v0,v0)\n return v0\n}";
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
            .any(|l| l.split_whitespace().next() == Some("paddsw"))
    );
    let export =
        std::env::var_os("KEEL_X86_PACKED_INTEGER_EXPORT_DIR").map(std::path::PathBuf::from);
    let d = export
        .clone()
        .unwrap_or_else(|| common::temp("packed-integer"));
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), harness(&all)).unwrap();
    fs::write(d.join("test.ir"), source(&all)).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    for (label, f, indices) in feature_sets() {
        let src = source(&indices);
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for (spill, p) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
                    let (o, t) = compile(&src, s, a, p, f).unwrap();
                    fs::write(d.join(format!("{label}-{s:?}-{a:?}-{spill}.o")), o).unwrap();
                    fs::write(d.join(format!("{label}-{s:?}-{a:?}-{spill}.s")), t).unwrap();
                }
            }
        }
    }
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    asm += "phaddw xmm13,xmm14\n";
    asm += "phaddd xmm13,xmm14\n";
    asm += "phaddsw xmm13,xmm14\n";
    asm += "phsubw xmm13,xmm14\n";
    asm += "phsubd xmm13,xmm14\n";
    asm += "phsubsw xmm13,xmm14\n";
    asm += "paddsb xmm13,xmm14\n";
    asm += "paddsw xmm13,xmm14\n";
    asm += "paddusb xmm13,xmm14\n";
    asm += "paddusw xmm13,xmm14\n";
    asm += "psubsb xmm13,xmm14\n";
    asm += "psubsw xmm13,xmm14\n";
    asm += "psubusb xmm13,xmm14\n";
    asm += "psubusw xmm13,xmm14\n";
    asm += "pavgb xmm13,xmm14\n";
    asm += "pavgw xmm13,xmm14\n";
    asm += "psignb xmm13,xmm14\n";
    asm += "psignw xmm13,xmm14\n";
    asm += "psignd xmm13,xmm14\n";
    asm += "pabsb xmm13,xmm14\n";
    asm += "pabsw xmm13,xmm14\n";
    asm += "pabsd xmm13,xmm14\n";
    asm += "vphaddw xmm13,xmm13,xmm14\n";
    asm += "vphaddd xmm13,xmm13,xmm14\n";
    asm += "vphaddsw xmm13,xmm13,xmm14\n";
    asm += "vphsubw xmm13,xmm13,xmm14\n";
    asm += "vphsubd xmm13,xmm13,xmm14\n";
    asm += "vphsubsw xmm13,xmm13,xmm14\n";
    asm += "vpaddsb xmm13,xmm13,xmm14\n";
    asm += "vpaddsw xmm13,xmm13,xmm14\n";
    asm += "vpaddusb xmm13,xmm13,xmm14\n";
    asm += "vpaddusw xmm13,xmm13,xmm14\n";
    asm += "vpsubsb xmm13,xmm13,xmm14\n";
    asm += "vpsubsw xmm13,xmm13,xmm14\n";
    asm += "vpsubusb xmm13,xmm13,xmm14\n";
    asm += "vpsubusw xmm13,xmm13,xmm14\n";
    asm += "vpavgb xmm13,xmm13,xmm14\n";
    asm += "vpavgw xmm13,xmm13,xmm14\n";
    asm += "vpsignb xmm13,xmm13,xmm14\n";
    asm += "vpsignw xmm13,xmm13,xmm14\n";
    asm += "vpsignd xmm13,xmm13,xmm14\n";
    asm += "vpabsb xmm13,xmm14\n";
    asm += "vpabsw xmm13,xmm14\n";
    asm += "vpabsd xmm13,xmm14\n";
    asm += "vphaddw ymm13,ymm13,ymm14\n";
    asm += "vphaddd ymm13,ymm13,ymm14\n";
    asm += "vphaddsw ymm13,ymm13,ymm14\n";
    asm += "vphsubw ymm13,ymm13,ymm14\n";
    asm += "vphsubd ymm13,ymm13,ymm14\n";
    asm += "vphsubsw ymm13,ymm13,ymm14\n";
    asm += "vpaddsb ymm13,ymm13,ymm14\n";
    asm += "vpaddsw ymm13,ymm13,ymm14\n";
    asm += "vpaddusb ymm13,ymm13,ymm14\n";
    asm += "vpaddusw ymm13,ymm13,ymm14\n";
    asm += "vpsubsb ymm13,ymm13,ymm14\n";
    asm += "vpsubsw ymm13,ymm13,ymm14\n";
    asm += "vpsubusb ymm13,ymm13,ymm14\n";
    asm += "vpsubusw ymm13,ymm13,ymm14\n";
    asm += "vpavgb ymm13,ymm13,ymm14\n";
    asm += "vpavgw ymm13,ymm13,ymm14\n";
    asm += "vpsignb ymm13,ymm13,ymm14\n";
    asm += "vpsignw ymm13,ymm13,ymm14\n";
    asm += "vpsignd ymm13,ymm13,ymm14\n";
    asm += "vpabsb ymm13,ymm14\n";
    asm += "vpabsw ymm13,ymm14\n";
    asm += "vpabsd ymm13,ymm14\n";

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
fn native_packed_integer_reference_and_guards() {
    let d = common::temp("packed-integer-native");
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

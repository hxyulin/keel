//! Architectural runtime byte controls: high-bit zeroing and per-128-bit indexing.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
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
const OPS: [(&str, &str, usize); 3] = [
    ("x86_ssse3_pshufb128", "i8x16", 16),
    ("x86_avx_vpshufb128", "i8x16", 16),
    ("x86_avx2_vpshufb256", "i8x32", 32),
];
fn source(cases: &[usize]) -> String {
    let mut s = String::new();
    for &i in cases {
        let (op, t, n) = OPS[i];
        for alias in [false, true] {
            s += &format!(
                "func @shuffle_{i}_{}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{t} align 1 v0\n v4 = load.{t} align 1 v1\n v5 = intrinsic {op}(v3,{})\n store align 1 v5,v2\n v6 = iconst.i64 {n}\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
                u8::from(alias),
                if alias { "v3" } else { "v4" }
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
fn c() -> String {
    let mut c = String::from(
        "typedef unsigned char u8;typedef unsigned long long u64;extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern void*memcpy(void*,const void*,__SIZE_TYPE__);extern int puts(const char*);extern int getpagesize(void);extern void*mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);\n",
    );
    for i in 0..3 {
        for alias in 0..2 {
            c += &format!("extern void shuffle_{i}_{alias}(const void*,const void*,void*);\n");
        }
    }
    c += "struct Case{void(*fn)(const void*,const void*,void*);unsigned n,alias;};static const struct Case cases[]={\n";
    for (i, (_, _, n)) in OPS.iter().enumerate() {
        for alias in 0..2 {
            c += &format!("{{shuffle_{i}_{alias},{n},{alias}}},\n");
        }
    }
    c += "};\n";
    c += r#"
static unsigned csr(void){unsigned x;__asm__ volatile("stmxcsr %0":"=m"(x));return x;}
static void reference(const struct Case*c,const u8*a,const u8*b,u8*o){const u8*control=c->alias?a:b;for(unsigned i=0;i<c->n;i++)o[i]=(control[i]&128)?0:a[(i&~15u)+(control[i]&15)];memcpy(o+c->n,a,c->n);memcpy(o+2*c->n,b,c->n);}
int main(void){unsigned state=csr();for(unsigned ci=0;ci<6;ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<256;pattern++)for(unsigned lane=0;lane<c->n;lane++)for(unsigned alias=0;alias<3;alias++){u8 aa[256],bb[256],oo[256],expected[96];memset(aa,0x5a,256);memset(bb,0x5a,256);memset(oo,0x5a,256);unsigned off=16+(pattern&15);u8*a=aa+off,*b=bb+off,*o=(alias==0?oo:alias==1?aa:bb)+off;for(unsigned i=0;i<c->n;i++){a[i]=(u8)(pattern*13+i*7);b[i]=(u8)(pattern+i*19);}b[lane]=(u8)pattern;reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n)||o[-1]!=0x5a||o[3*c->n]!=0x5a||csr()!=state)return 10;}
 unsigned long page=(unsigned)getpagesize();u8*maps[3];for(unsigned i=0;i<3;i++){maps[i]=mmap(0,page*3,0,0x22,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 20;}for(unsigned edge=0;edge<2;edge++){u8*a=maps[0]+page+(edge?page-c->n:0),*b=maps[1]+page+(edge?page-c->n:0),*o=maps[2]+page+(edge?page-3*c->n:0),expected[96];for(unsigned i=0;i<c->n;i++){a[i]=(u8)(7*i+1);b[i]=(u8)(255-11*i);}reference(c,a,b,expected);c->fn(a,b,o);if(memcmp(o,expected,3*c->n))return 21;}for(unsigned i=0;i<3;i++)if(munmap(maps[i],3*page))return 22;
 }puts("PASS byte shuffle all controls lanes aliases footprints");return 0;}
"#;
    c
}
#[test]
fn byte_shuffle_features_purity_and_encodings() {
    for i in 0..3 {
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
    let mut f = Features::NONE;
    f.ssse3 = true;
    compile(
        &source(&[0]),
        SelectionTier::Full,
        AllocationTier::Full,
        &EMPTY,
        f,
    )
    .unwrap();
    f = Features::NONE;
    f = f.with_features("avx").unwrap();
    f.xsave = true;
    f.osxsave = true;
    compile(
        &source(&[1]),
        SelectionTier::Full,
        AllocationTier::Full,
        &EMPTY,
        f,
    )
    .unwrap();
    assert!(
        compile(
            &source(&[2]),
            SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY,
            f
        )
        .is_err()
    );
    let unused = "func @unused(i8x16)->i8x16 {\nblock0(v0:i8x16):\n v1 = intrinsic x86_ssse3_pshufb128(v0,v0)\n return v0\n}";
    let (_, text) = compile(
        unused,
        SelectionTier::Full,
        AllocationTier::Full,
        &EMPTY,
        Features::NONE,
    )
    .unwrap();
    assert!(!text.contains("pshufb"));
    assert!(keel_codegen::aarch64::compile(&keel_ir::parse(&source(&[0, 1, 2])).unwrap()).is_err());
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                compile(
                    &source(&[0, 1, 2]),
                    s,
                    a,
                    p,
                    Features::for_level(CpuLevel::V3),
                )
                .unwrap();
            }
        }
    }
    let d = common::temp("byte-shuffle-oracle");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), c()).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    let asm = ".intel_syntax noprefix\n.text\npshufb xmm15,xmm14\nvpshufb xmm15,xmm15,xmm14\nvpshufb ymm15,ymm15,ymm14\n";
    fs::write(d.join("test.s"), asm).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(d.join("test.s"))
            .arg("-o")
            .arg(d.join("test.o")),
    );
    use object::{Object, ObjectSection};
    let native = fs::read(d.join("test.o")).unwrap();
    let own = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V3))
        .unwrap();
    let n = object::File::parse(&*native).unwrap();
    let o = object::File::parse(&*own).unwrap();
    assert_eq!(
        n.section_by_name(".text").unwrap().data().unwrap(),
        o.section_by_name(".text").unwrap().data().unwrap()
    );
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_byte_shuffle_controls_aliases_and_guards() {
    let d = common::temp("byte-shuffle-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), c()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let (obj, _) = compile(
                    &source(&[0, 1, 2]),
                    s,
                    a,
                    p,
                    Features::for_level(CpuLevel::V3),
                )
                .unwrap();
                fs::write(d.join("test.o"), obj).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("test.c"))
                        .arg(d.join("test.o"))
                        .arg("-o")
                        .arg(d.join("test")),
                );
                common::command(&mut Command::new(d.join("test")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

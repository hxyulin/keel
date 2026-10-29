//! The SSE2 unsigned-byte sign bias must be 0x80 in every byte lane.
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
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        r#"
typedef unsigned char u8;
extern void *memcpy(void*,const void*,__SIZE_TYPE__);
extern void *memset(void*,int,__SIZE_TYPE__);
extern int memcmp(const void*,const void*,__SIZE_TYPE__);
extern int puts(const char*);
struct Case {void (*run)(const void*,const void*,void*);unsigned cc;};
"#,
    );
    for cc in ["ult", "ule", "ugt", "uge"] {
        ir += &format!(
            "func @byte_{cc}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i8x16 align 1 v0\n v4 = load.i8x16 align 1 v1\n v5 = vicmp.{cc} v3,v4\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
        );
        c += &format!("extern void byte_{cc}(const void*,const void*,void*);\n");
    }
    c += r#"
static const struct Case cases[]={{byte_ult,0},{byte_ule,1},{byte_ugt,2},{byte_uge,3}};
int main(void) {
 for(unsigned cc=0;cc<4;cc++) for(unsigned x=0;x<256;x++) for(unsigned y=0;y<256;y++) {
  u8 aa[96],bb[96],oo[96],a[16],b[16],expected[48];
  unsigned off=(x+y)&15,alias=(x+y)%3;
  memset(aa,0x5a,96);memset(bb,0x5a,96);memset(oo,0x5a,96);
  for(unsigned l=0;l<16;l++) {
   // All 65,536 unsigned-byte pairs occur at EVERY lane position.
   a[l]=(u8)(x+31*l);b[l]=(u8)(y+17*l);
   unsigned truth=cc==0?a[l]<b[l]:cc==1?a[l]<=b[l]:cc==2?a[l]>b[l]:a[l]>=b[l];
   expected[l]=truth?255:0;
  }
  memcpy(expected+16,a,16);memcpy(expected+32,b,16);
  memcpy(aa+16+off,a,16);memcpy(bb+16+off,b,16);
  u8 *out=(alias==0?oo:alias==1?aa:bb)+16+off;
  cases[cc].run(aa+16+off,bb+16+off,out);
  if(memcmp(out,expected,48)||out[-1]!=0x5a||out[48]!=0x5a)return 1;
 }
 puts("PASS all unsigned-byte comparison pairs at every lane, live inputs, aliases, unaligned");return 0;
}
"#;
    (ir, c)
}
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
fn directory() -> PathBuf {
    std::env::var_os("KEEL_BYTE_COMPARE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-byte-compare-{}", std::process::id()))
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
fn byte_compare_recipe_and_all_pair_oracles() {
    let (source, c) = fixture();
    let dir = directory();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("test.c"), &c).unwrap();
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
            .arg(dir.join("test.c")),
    );
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
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
                    let (text, _, object) = compile(&source, s, a, p, Features::for_level(level));
                    assert_eq!(text.matches("packsswb\t").count(), 4);
                    assert!(!text.contains("punpck") && !text.contains("pshufd"));
                    let stem = format!("byte-{level:?}-{s:?}-{a:?}-{pool}");
                    fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                    fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
                    fs::write(dir.join(format!("{stem}.s")), text).unwrap();
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
    let asm =
        ".intel_syntax noprefix\n.text\npcmpeqd xmm15,xmm15\npsllw xmm15,7\npacksswb xmm15,xmm15\n";
    fs::write(dir.join("bias.s"), asm).unwrap();
    command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(dir.join("bias.s"))
            .arg("-o")
            .arg(dir.join("bias.o")),
    );
    let ours = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V1))
        .unwrap();
    assert_eq!(
        text_bytes(&ours),
        text_bytes(&fs::read(dir.join("bias.o")).unwrap())
    );
    if std::env::var_os("KEEL_BYTE_COMPARE_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

//! Architectural rounding/approximation/source-biased min/max and scalar flags.
//! Forced instruction oracles preserve unused-result MXCSR effects.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{Intrinsic, Type};
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Case {
    id: Intrinsic,
    mode: u8,
    alias: bool,
}
impl Case {
    fn name(&self) -> String {
        format!("misc_{}_{}_{}", self.id, self.mode, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for id in Intrinsic::ALL.into_iter().filter(|i| i.is_x86_fp_misc()) {
        for mode in 0..if id.has_x86_round_control() { 16 } else { 1 } {
            for alias in [false, true] {
                if alias && (id.has_x86_round_control() || id.info().params.len() == 1) {
                    continue;
                }
                v.push(Case { id, mode, alias });
            }
        }
    }
    v
}
fn source() -> String {
    let mut s = String::new();
    for c in cases() {
        let info = c.id.info();
        let ty = info.params[0];
        let bytes = info.returns[0].bytes().unwrap();
        let srcbytes = ty.bytes().unwrap();
        let name = c.name();
        for unused in [false, true] {
            s += &format!(
                "func @{}{name}(ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{ty} align 1 v0\n v4 = load.{ty} align 1 v1\n v5 = iconst.i32 {}\n v6 = intrinsic {}(v3{})\n",
                if unused { "unused_" } else { "" },
                c.mode,
                c.id,
                if c.id.has_x86_round_control() {
                    ",v5"
                } else if info.params.len() == 2 {
                    if c.alias { ",v3" } else { ",v4" }
                } else {
                    ""
                }
            );
            if !unused {
                s += &format!(
                    " store align 1 v6,v2\n v7 = iconst.i64 {bytes}\n v8 = ptradd v2,v7\n store align 1 v3,v8\n v9 = iconst.i64 {srcbytes}\n v10 = ptradd v8,v9\n store align 1 v4,v10\n"
                );
            }
            s += " return\n}\n";
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,const void*,void*);void(*unused)(const void*,const void*,void*);void(*ref)(const void*,const void*,void*);unsigned in,out,bits;const char*name;};static const struct Case cases[]={\n",
    );
    for case in cases() {
        let i = case.id.info();
        let ty = i.params[0];
        let name = case.name();
        let float32 = ty.vector().map_or(ty, |v| v.lane_type()) == Type::F32;
        let bits = if float32 { 32 } else { 64 };
        let ct = if float32 { "float" } else { "double" };
        let bytes = ty.bytes().unwrap();
        let n = bytes / (bits / 8);
        let vt = format!(
            "__m{}{}",
            if bytes == 32 { 256 } else { 128 },
            if float32 { "" } else { "d" }
        );
        let load = if ty.is_float() {
            if float32 {
                "_mm_load_ss"
            } else {
                "_mm_load_sd"
            }
        } else if bytes == 32 {
            if float32 {
                "_mm256_loadu_ps"
            } else {
                "_mm256_loadu_pd"
            }
        } else if float32 {
            "_mm_loadu_ps"
        } else {
            "_mm_loadu_pd"
        };
        c += &format!(
            "extern void {name}(const void*,const void*,void*);extern void unused_{name}(const void*,const void*,void*);__attribute__((noinline))static void ref_{name}(const void*p,const void*q,void*out){{{ct} aa[{n}],bb[{n}];memcpy(aa,p,{bytes});memcpy(bb,q,{bytes});{vt} a={load}(aa),b={load}({}),e;",
            if case.alias { "aa" } else { "bb" }
        );
        let op = case.id.x86_fp_misc_opcode().unwrap();
        if op.contains("comis") {
            c += &format!(
                "unsigned char cf,pf,zf;__asm__ volatile(\"{op} %4,%3;setb %0;setp %1;sete %2\":\"=qm\"(cf),\"=qm\"(pf),\"=qm\"(zf):\"x\"(a),\"x\"(b):\"cc\",\"memory\");uint32_t flags=cf|pf<<2|zf<<6;memcpy(out,&flags,4);}}\n"
            );
        } else {
            let asm = if case.id.has_x86_round_control() {
                format!(
                    "{op} ${}, %1, {}%0",
                    case.mode,
                    if ty.is_float() { "%1, " } else { "" }
                )
            } else if i.params.len() == 2 && !case.id.has_x86_round_control() {
                format!("{op} %2,%1,%0")
            } else {
                format!("{op} %1,{}%0", if ty.is_float() { "%1," } else { "" })
            };
            c += &format!(
                "__asm__ volatile(\"{asm}\":\"=x\"(e):\"x\"(a),\"x\"(b):\"memory\");memcpy(out,&e,{});}}\n",
                i.returns[0].bytes().unwrap()
            );
        }
        table += &format!(
            "{{{name},unused_{name},ref_{name},{bytes},{},{bits},\"{name}\"}},\n",
            i.returns[0].bytes().unwrap()
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
int main(void){unsigned saved=_mm_getcsr();const uint64_t e32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7fc00001,0x7f800001,0xffc12345,0xff800001,0x3fc00000,0xbfc00000,1,0x7f7fffff,0x00800000,0x80000001};const uint64_t e64[]={0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xbff0000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8000000000001ULL,0x7ff0000000000001ULL,0xfff812345678abcdULL,0xfff0000000000001ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,1,0x7fefffffffffffffULL,0x0010000000000000ULL,0x8000000000000001ULL};uint64_t rng=12345;unsigned offsets[]={0,1,7,15};
for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned state=0;state<16;state++)for(unsigned pattern=0;pattern<(state<4?512:32);pattern++)for(unsigned oi=0;oi<4;oi++)for(unsigned alias=0;alias<2;alias++){
 unsigned char in[128],inb[128],out[128],e[96];memset(in,0x5a,128);memset(inb,0x5a,128);memset(out,0x5a,128);unsigned char*p=in+16+offsets[oi],*q=inb+16+offsets[oi],*r=(alias?in:out)+16+offsets[oi];
 for(unsigned i=0;i<c->in*8/c->bits;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t a=pattern<256?(c->bits==32?e32[(pattern%16+3*i)%16]:e64[(pattern%16+3*i)%16]):rng;rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t b=pattern<256?(c->bits==32?e32[((state<4?pattern/16:pattern*7+pattern/16)+5*i)%16]:e64[((state<4?pattern/16:pattern*7+pattern/16)+5*i)%16]):rng;memcpy(p+i*c->bits/8,&a,c->bits/8);memcpy(q+i*c->bits/8,&b,c->bits/8);}
 memcpy(e+c->out,p,c->in);memcpy(e+c->out+c->in,q,c->in);unsigned csr=((saved|0x1f80)&~(0x8040|0x603f))|((state&3)<<13)|((state&4)?64:0)|((state&8)?0x8000:0);_mm_setcsr(csr);c->ref(p,q,e);unsigned ef=_mm_getcsr()&63,uf=ef;if(pattern<256&&oi==0&&!alias){_mm_setcsr(csr);c->unused(p,q,r);uf=_mm_getcsr()&63;}_mm_setcsr(csr);c->run(p,q,r);unsigned rf=_mm_getcsr()&63;if(memcmp(r,e,c->out+2*c->in)||ef!=rf||ef!=uf||r[-1]!=0x5a||r[c->out+2*c->in]!=0x5a){printf("failed %s pattern%u offset%u alias%u flags%u/%u/%u\n",c->name,pattern,offsets[oi],alias,ef,rf,uf);return 10;}
}}
_mm_setcsr(saved);return 0;}
"#;
    c
}
#[path = "common/matrix.rs"]
mod matrix;
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    static PREPARED: std::sync::OnceLock<matrix::PreparedModule> = std::sync::OnceLock::new();
    let m = PREPARED
        .get_or_init(|| matrix::PreparedModule::parse(&source()))
        .for_selector(s);
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if empty {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((b, text))
}
#[test]
fn round_approx_minmax_comi_typed_alias_spill_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("fp-misc.ir"), source()).unwrap();
        fs::write(d.join("fp-misc.c"), harness()).unwrap();
    }
    let d = common::temp("fp-misc-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("compare.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx",
                "-O2",
                "-c",
            ])
            .arg(d.join("compare.c"))
            .arg("-o")
            .arg(d.join("oracle.o")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                if let Some(d) = &export {
                    let name = format!("fp-misc-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn fp_misc_types_features_and_controls() {
    assert!(
        object(
            SelectionTier::Baseline,
            AllocationTier::Full,
            false,
            Features::NONE
        )
        .is_err()
    );
    let mut f = Features::NONE;
    f = f.with_features("avx").unwrap();
    object(SelectionTier::Full, AllocationTier::Full, true, f).unwrap();
    let bad = source().replace("iconst.i32 15", "iconst.i32 16");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let bad = source().replace("(v3,v5)", "(v3,v0)");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let arm_source = source();
    let narrow = arm_source
        .split("func @misc_x86_avx_vroundps256")
        .next()
        .unwrap();
    assert!(
        keel_codegen::aarch64::compile(&keel_ir::parse(narrow).unwrap())
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
    for c in cases() {
        assert!(!c.id.info().pure);
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_round_approx_minmax_comi_flags_and_aliases() {
    if !std::is_x86_feature_detected!("avx") {
        return;
    }
    let d = common::temp("fp-misc-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("compare.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("compare.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx", "-fno-fast-math", "-ffp-contract=off"])
                        .arg(d.join("compare.c"))
                        .arg(d.join("compare.o"))
                        .arg("-o")
                        .arg(d.join("compare")),
                );
                common::command(&mut Command::new(d.join("compare")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

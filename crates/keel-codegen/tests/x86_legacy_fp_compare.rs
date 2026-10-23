//! All32 AVX predicate controls, including signaling differences and unused-result
//! MXCSR effects. The oracle uses Clang's assembler to force the exact compare.
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
mod common;
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
        format!("cmp_{}_{}_{}", self.id, self.mode, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|i| i.is_x86_legacy_fp_compare())
    {
        for mode in 0..8 {
            for alias in [false, true] {
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
        let out = info.returns[0];
        let bytes = out.bytes().unwrap();
        let srcbytes = ty.bytes().unwrap();
        let name = c.name();
        for unused in [false, true] {
            s += &format!(
                "func @{}{name}(ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{ty} align 1 v0\n v4 = load.{ty} align 1 v1\n v5 = iconst.i32 {}\n v6 = intrinsic {}(v3,{},v5)\n",
                if unused { "unused_" } else { "" },
                c.mode,
                c.id,
                if c.alias { "v3" } else { "v4" }
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
            "extern void {name}(const void*,const void*,void*);extern void unused_{name}(const void*,const void*,void*);__attribute__((noinline))static void ref_{name}(const void*p,const void*q,void*out){{{ct} aa[{n}],bb[{n}];memcpy(aa,p,{bytes});memcpy(bb,q,{bytes});{vt} a={load}(aa),b={load}({}),e=a;__asm__ volatile(\"{} ${}, %1, %0\":\"+x\"(e):\"x\"(b):\"memory\");memcpy(out,&e,{});}}\n",
            if case.alias { "aa" } else { "bb" },
            case.id.x86_legacy_fp_compare_opcode().unwrap(),
            case.mode,
            i.returns[0].bytes().unwrap()
        );
        table += &format!(
            "{{{name},unused_{name},ref_{name},{bytes},{},{bits},\"{name}\"}},\n",
            i.returns[0].bytes().unwrap()
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
int main(void){unsigned saved=_mm_getcsr();const uint64_t e32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7fc00001,0x7f800001,0xffc12345,0xff800001,0x3fc00000,0xbfc00000,1,0x7f7fffff,0x00800000,0x80000001};const uint64_t e64[]={0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xbff0000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8000000000001ULL,0x7ff0000000000001ULL,0xfff812345678abcdULL,0xfff0000000000001ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,1,0x7fefffffffffffffULL,0x0010000000000000ULL,0x8000000000000001ULL};uint64_t rng=12345;unsigned offsets[]={0,1,7,15};
for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<512;pattern++)for(unsigned oi=0;oi<4;oi++)for(unsigned alias=0;alias<2;alias++){
 unsigned char in[128],inb[128],out[128],e[96];memset(in,0x5a,128);memset(inb,0x5a,128);memset(out,0x5a,128);unsigned char*p=in+16+offsets[oi],*q=inb+16+offsets[oi],*r=(alias?in:out)+16+offsets[oi];
 for(unsigned i=0;i<c->in*8/c->bits;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t a=pattern<256?(c->bits==32?e32[(pattern%16+3*i)%16]:e64[(pattern%16+3*i)%16]):rng;rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t b=pattern<256?(c->bits==32?e32[(pattern/16+5*i)%16]:e64[(pattern/16+5*i)%16]):rng;memcpy(p+i*c->bits/8,&a,c->bits/8);memcpy(q+i*c->bits/8,&b,c->bits/8);}
 memcpy(e+c->out,p,c->in);memcpy(e+c->out+c->in,q,c->in);unsigned csr=(saved|0x1f80)&~(0x8040|63);_mm_setcsr(csr);c->ref(p,q,e);unsigned ef=_mm_getcsr()&63,uf=ef;if(pattern<256&&oi==0&&!alias){_mm_setcsr(csr);c->unused(p,q,r);uf=_mm_getcsr()&63;}_mm_setcsr(csr);c->run(p,q,r);unsigned rf=_mm_getcsr()&63;if(memcmp(r,e,c->out+2*c->in)||ef!=rf||ef!=uf||r[-1]!=0x5a||r[c->out+2*c->in]!=0x5a){printf("failed %s pattern%u offset%u alias%u flags%u/%u/%u\n",c->name,pattern,offsets[oi],alias,ef,rf,uf);return 10;}
}}
_mm_setcsr(saved);return 0;}
"#;
    c
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
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
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((b, text))
}
#[test]
fn every_avx_compare_predicate_typed_alias_spill_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("legacy-compare8.ir"), source()).unwrap();
        fs::write(d.join("legacy-compare8.c"), harness()).unwrap();
    }
    let d = common::temp("legacy-compare8-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("compare.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-march=x86-64",
                "-mno-avx",
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
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                if let Some(d) = &export {
                    let name = format!("legacy-compare8-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn legacy_compare_controls_and_baseline_features() {
    object(
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::NONE,
    )
    .unwrap();
    let bad = source().replace("iconst.i32 7", "iconst.i32 8");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let bad = source().replace(",v5)", ",v0)");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    for f in m.functions.iter().filter(|f| f.name.starts_with("unused_")) {
        assert!(f.layout_insts().any(|(_,i)|matches!(f.inst_data(i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_legacy_fp_compare())));
    }
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_all8_predicates_masks_signaling_and_aliases() {
    let d = common::temp("legacy-compare8-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("compare.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                fs::write(d.join("compare.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args([
                            "-O2",
                            "-march=x86-64",
                            "-mno-avx",
                            "-fno-fast-math",
                            "-ffp-contract=off",
                        ])
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

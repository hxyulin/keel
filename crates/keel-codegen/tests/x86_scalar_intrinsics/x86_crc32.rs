//! SSE4.2 CRC32 uses a reflected Castagnoli scalar software reference.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Intrinsic;
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

struct Case {
    id: Intrinsic,
    alias: bool,
}
impl Case {
    fn name(&self) -> String {
        format!("crc_{}_{}", self.id, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for id in Intrinsic::ALL.into_iter().filter(|i| i.is_x86_crc32()) {
        v.push(Case { id, alias: false });
        if id.info().params[1] == keel_ir::Type::I32 {
            v.push(Case { id, alias: true });
        }
    }
    v
}
fn source() -> String {
    let mut s = String::new();
    for c in cases() {
        let ty = c.id.info().params[1];
        let name = c.name();
        s += &format!(
            "func @{name}(ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i32 align 1 v0\n v4 = load.{ty} align 1 v1\n v5 = intrinsic {}(v3,{})\n store align 1 v5,v2\n v6 = iconst.i64 4\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
            c.id,
            if c.alias { "v3" } else { "v4" }
        );
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void*memcpy(void*,const void*,__SIZE_TYPE__);extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,const void*,void*);unsigned bytes,alias;const char*name;};static const struct Case cases[]={\n",
    );
    for ca in cases() {
        let name = ca.name();
        c += &format!("extern void {name}(const void*,const void*,void*);\n");
        table += &format!(
            "{{{name},{},{},\"{name}\"}},\n",
            ca.id.info().params[1].bytes().unwrap(),
            u8::from(ca.alias)
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
static uint32_t reference(uint32_t seed,uint64_t data,unsigned n){for(unsigned i=0;i<n;i++){seed^=(unsigned)(data>>(8*i))&255;for(unsigned bit=0;bit<8;bit++)seed=(seed>>1)^(0x82f63b78u&-(seed&1));}return seed;}
int main(void){uint64_t rng=12345;for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<4096;pattern++)for(unsigned offset=0;offset<16;offset++)for(unsigned inplace=0;inplace<2;inplace++){
 unsigned char in[128],data[128],out[128],e[32];memset(in,0x5a,128);memset(data,0x5a,128);memset(out,0x5a,128);unsigned char*p=in+16+offset,*q=data+16+offset,*r=(inplace?in:out)+16+offset;
 rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint32_t seed=pattern==0?0:pattern==1?~0u:pattern<34?1u<<((pattern-2)&31):pattern<66?~(1u<<((pattern-34)&31)):(uint32_t)rng;rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t bits=pattern<64?1ULL<<(pattern&63):pattern<128?~(1ULL<<(pattern&63)):rng;memcpy(p,&seed,4);memcpy(q,&bits,c->bytes);uint32_t result=reference(seed,c->alias?seed:bits,c->bytes);memcpy(e,&result,4);memcpy(e+4,p,4);memcpy(e+8,q,c->bytes);c->run(p,q,r);if(memcmp(r,e,8+c->bytes)||r[-1]!=0x5a||r[8+c->bytes]!=0x5a){printf("crc failed %s pattern%u offset%u inplace%u\n",c->name,pattern,offset,inplace);return 10;}
}}return 0;}
"#;
    c
}
fn compile(
    m: &keel_ir::Module,
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
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
    compile(&m, s, a, empty, f)
}
#[test]
fn crc32_typed_alias_spill_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("crc32.ir"), source()).unwrap();
        fs::write(d.join("crc32.c"), harness()).unwrap();
    }
    let d = common::temp("crc32-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("crc.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("crc.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V2)).unwrap();
                if let Some(d) = &export {
                    let name = format!("crc32-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn crc32_types_features_and_purity() {
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
    f.sse42 = true;
    object(SelectionTier::Full, AllocationTier::Full, true, f).unwrap();
    let bad = source().replace("load.i8", "load.i16");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    for c in cases() {
        assert!(c.id.info().pure);
    }
    assert!(
        keel_codegen::aarch64::compile(&keel_ir::parse(&source()).unwrap())
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_crc32_castagnoli_seed_width_aliases() {
    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }
    let d = common::temp("crc32-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("crc.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V2)).unwrap();
                fs::write(d.join("crc.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("crc.c"))
                        .arg(d.join("crc.o"))
                        .arg("-o")
                        .arg(d.join("crc")),
                );
                common::command(&mut Command::new(d.join("crc")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

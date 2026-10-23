//! Runtime lane controls and explicit scalar bit/flag results, independent scalar oracle.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Intrinsic;
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
    alias: bool,
}
impl Case {
    fn name(&self) -> String {
        format!("bits_{}_{}", self.id, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|i| i.is_x86_legacy_sign_mask())
    {
        v.push(Case { id, alias: false });
        if id.info().params.len() == 2 {
            v.push(Case { id, alias: true });
        }
    }
    v
}
fn source() -> String {
    let mut s = String::new();
    for c in cases() {
        let info = c.id.info();
        let a = info.params[0];
        let b = info.params.get(1).copied();
        let o = info.returns[0];
        let name = c.name();
        let outbytes = o.bytes().unwrap();
        let abytes = a.bytes().unwrap();
        s += &format!(
            "func @{name}(ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{a} align 1 v0\n"
        );
        if let Some(b) = b {
            s += &format!(" v4 = load.{b} align 1 v1\n v5 = bitcast.{b} v3\n");
        }
        s += &format!(
            " v6 = intrinsic {}(v3{})\n store align 1 v6,v2\n v7 = iconst.i64 {outbytes}\n v8 = ptradd v2,v7\n store align 1 v3,v8\n",
            c.id,
            if b.is_some() {
                if c.alias { ",v5" } else { ",v4" }
            } else {
                ""
            }
        );
        if b.is_some() {
            s +=
                &format!(" v9 = iconst.i64 {abytes}\n v10 = ptradd v8,v9\n store align 1 v4,v10\n");
        }
        s += " return\n}\n";
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <immintrin.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,const void*,void*);unsigned bits,bytes,out,arity,kind,alias;const char*name;};static const struct Case cases[]={\n",
    );
    for case in cases() {
        let i = case.id.info();
        let a = i.params[0].vector().unwrap();
        let op = case.id.x86_legacy_sign_mask_opcode().unwrap();
        let kind = match op {
            "vpermilps" => 0,
            "vpermilpd" => 1,
            "vpermd" | "vpermps" => 2,
            "movmskps" | "movmskpd" | "pmovmskb" => 3,
            "vtestps" | "vtestpd" => 4,
            "vptest" => 5,
            _ => unreachable!(),
        };
        let name = case.name();
        c += &format!("extern void {name}(const void*,const void*,void*);\n");
        table += &format!(
            "{{{name},{},{},{},{},{kind},{},\"{name}\"}},\n",
            a.lane_type().bytes().unwrap() * 8,
            a.bytes(),
            i.returns[0].bytes().unwrap(),
            i.params.len(),
            u8::from(case.alias)
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
static uint64_t get(const unsigned char*p,unsigned bits){uint64_t r=0;memcpy(&r,p,bits/8);return r;}
static void reference(const struct Case*c,const unsigned char*a,const unsigned char*b,unsigned char*out){const unsigned char*control=c->alias?a:b;memset(out,0,96);unsigned n=c->bytes*8/c->bits;
 if(c->kind<3){for(unsigned i=0;i<n;i++){uint64_t ctrl=get(control+i*c->bits/8,c->bits);unsigned lane=c->kind==2?(unsigned)ctrl&7:c->kind==1?(i/2)*2+((ctrl>>1)&1):(i/4)*4+(ctrl&3);memcpy(out+i*c->bits/8,a+lane*c->bits/8,c->bits/8);}}
 else{uint32_t value=0;if(c->kind==3){for(unsigned i=0;i<n;i++)value|=(uint32_t)(get(a+i*c->bits/8,c->bits)>>(c->bits-1))<<i;}
 else{uint64_t z=0,cy=0;if(c->kind==5){for(unsigned i=0;i<c->bytes;i++){z|=a[i]&control[i];cy|=(unsigned char)~a[i]&control[i];}}else{for(unsigned i=0;i<n;i++){unsigned sa=get(a+i*c->bits/8,c->bits)>>(c->bits-1),sb=get(control+i*c->bits/8,c->bits)>>(c->bits-1);z|=sa&sb;cy|=(!sa)&sb;}}value=(!cy)|((!z)<<6);}memcpy(out,&value,4);}
 memcpy(out+c->out,a,c->bytes);if(c->arity==2)memcpy(out+c->out+c->bytes,b,c->bytes);
}
int main(void){unsigned saved=_mm_getcsr();unsigned offsets[]={0,1,7,15};uint64_t rng=12345;for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<1024;pattern++)for(unsigned oi=0;oi<4;oi++)for(unsigned alias=0;alias<2;alias++){
 unsigned char in[128],inb[128],out[128],e[96];memset(in,0x5a,128);memset(inb,0x5a,128);memset(out,0x5a,128);unsigned char*a=in+16+offsets[oi],*b=inb+16+offsets[oi],*r=(alias?in:out)+16+offsets[oi];for(unsigned i=0;i<c->bytes;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;a[i]=rng>>32;b[i]=(pattern+3*i)^((rng>>19)&~255u);}
 if(c->kind<3){for(unsigned i=0;i<c->bytes*8/c->bits;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint64_t ctrl=(rng&~7ULL)|((pattern+3*i)&7);memcpy(b+i*c->bits/8,&ctrl,c->bits/8);}}
 if(c->kind>=4){switch(pattern%8){case 0:memset(b,0,c->bytes);break;case 1:memset(a,0,c->bytes);break;case 2:memset(a,255,c->bytes);break;case 3:memcpy(b,a,c->bytes);break;case 4:for(unsigned i=0;i<c->bytes;i++)b[i]=~a[i];break;}}
 if(c->kind==3&&pattern<c->bytes*8/c->bits+3){unsigned n=c->bytes*8/c->bits;for(unsigned i=0;i<n;i++){unsigned char*p=a+i*c->bits/8+c->bits/8-1;*p&=127;if(pattern==i||pattern==n+1)*p|=128;}}
 reference(c,a,b,e);unsigned csr=(saved|0x1f80)&~63u;_mm_setcsr(csr);c->run(a,b,r);unsigned flags=_mm_getcsr()&63;unsigned size=c->out+c->bytes*c->arity;if(memcmp(r,e,size)||flags||r[-1]!=0x5a||r[size]!=0x5a){printf("failed %s pattern%u off%u alias%u flags%u\n",c->name,pattern,offsets[oi],alias,flags);return 10;}
 }}_mm_setcsr(saved);return 0;}
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
fn runtime_permute_masks_tests_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("legacy-sign-mask.ir"), source()).unwrap();
        fs::write(d.join("legacy-sign-mask.c"), harness()).unwrap();
    }
    let d = common::temp("legacy-sign-mask-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("bits.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("bits.c")),
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
                    let name = format!("legacy-sign-mask-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn strict_v1_sign_mask_types_and_purity() {
    object(
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::NONE,
    )
    .unwrap();
    let bad = source().replace("load.f32x4", "load.i32x4");
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
fn native_runtime_permutations_masks_and_test_flags() {
    let d = common::temp("legacy-sign-mask-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("bits.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                fs::write(d.join("bits.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("bits.c"))
                        .arg(d.join("bits.o"))
                        .arg("-o")
                        .arg(d.join("bits")),
                );
                common::command(&mut Command::new(d.join("bits")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

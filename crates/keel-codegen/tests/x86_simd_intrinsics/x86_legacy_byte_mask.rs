//! Runtime lane controls and explicit scalar bit/flag results, independent scalar oracle.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source() -> String {
    let mut s = String::new();
    for alias in [false, true] {
        s += &format!(
            "func @byte_mask_{alias}(ptr,ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.i8x16 align 1 v1\n v5 = load.i8x16 align 1 v2\n intrinsic x86_sse_maskmovdqu(v0,v4,{})\n v6 = load.i8x16 align 1 v0\n store align 1 v6,v3\n v7 = iconst.i64 16\n v8 = ptradd v3,v7\n store align 1 v4,v8\n v9 = ptradd v8,v7\n store align 1 v5,v9\n return\n}}\n",
            if alias { "v4" } else { "v5" }
        );
    }
    s
}
fn harness() -> String {
    String::from(
        r#"
#include <stdint.h>
extern void*memcpy(void*,const void*,__SIZE_TYPE__);extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);
extern void byte_mask_false(void*,const void*,const void*,void*);extern void byte_mask_true(void*,const void*,const void*,void*);
int main(void){for(unsigned mask=0;mask<65536;mask++)for(unsigned offset=0;offset<16;offset++)for(unsigned alias=0;alias<2;alias++)for(unsigned inplace=0;inplace<2;inplace++){
 unsigned char data[48],masks[48],dest[80],out[80],expected[80],e[48];memset(data,0x5a,48);memset(masks,0x5a,48);memset(dest,0x5a,80);memset(out,0x5a,80);
 unsigned char*p=data+offset,*q=masks+offset,*d=dest+16+offset,*r=out+16;
 for(unsigned i=0;i<16;i++){p[i]=(unsigned char)((mask+i*17)&127)|((mask>>((i+5)%16)&1)<<7);q[i]=(unsigned char)((mask+i*31)&127)|((mask>>i&1)<<7);}
 if(inplace)memcpy(d,p,16);memcpy(expected,dest,80);memcpy(e+16,inplace?d:p,16);memcpy(e+32,q,16);
 for(unsigned i=0;i<16;i++){unsigned char a=e[16+i],m=alias?a:q[i];if(m&128)expected[16+offset+i]=a;}memcpy(e,expected+16+offset,16);
 (alias?byte_mask_true:byte_mask_false)(d,inplace?d:p,q,r);
 if(memcmp(dest,expected,80)||memcmp(r,e,48)||r[-1]!=0x5a||r[48]!=0x5a){printf("byte mask failed mask%u offset%u alias%u inplace%u\n",mask,offset,alias,inplace);return 10;}
}return 0;}
"#,
    )
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
fn byte_mask_typed_alias_spill_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("legacy-byte-mask.ir"), source()).unwrap();
        fs::write(d.join("legacy-byte-mask.c"), harness()).unwrap();
    }
    let d = common::temp("legacy-byte-mask-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("byte.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("byte.c")),
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
                    let name = format!("legacy-byte-mask-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn byte_mask_types_features_and_target() {
    object(
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::NONE,
    )
    .unwrap();
    let bad = source().replace("load.i8x16", "load.i16x8");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    assert!(
        keel_codegen::aarch64::compile(&keel_ir::parse(&source()).unwrap())
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_all_byte_masks_aliases_and_preserved_inputs() {
    let d = common::temp("legacy-byte-mask-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("byte.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V1)).unwrap();
                fs::write(d.join("byte.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("byte.c"))
                        .arg(d.join("byte.o"))
                        .arg("-o")
                        .arg(d.join("byte")),
                );
                common::command(&mut Command::new(d.join("byte")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

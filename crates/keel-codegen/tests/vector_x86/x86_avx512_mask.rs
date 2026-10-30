//! Scalar-mask semantics, merge preservation, allocator pressure and fault suppression.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::from("decl @mask_clobber()\n");
    for (suffix, shape, mask) in [("d", "i32x16", "i16"), ("q", "i64x8", "i8")] {
        for signed in ["", "u"] {
            for control in 0..8 {
                s += &format!(
                    "func @cmp_{signed}{suffix}_{control}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = iconst.i32 {control}\n v6 = intrinsic x86_avx512_cmp{signed}{suffix}(v3,v4,v5)\n store align 1 v6,v2\n return\n}}\n"
                );
            }
        }
        s += &format!(
            "func @cmp_chain_{suffix}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = iconst.i32 1\n v6 = intrinsic x86_avx512_cmpu{suffix}(v3,v4,v5)\n call @mask_clobber()\n v7 = intrinsic x86_avx512_maskadd{suffix}(v3,v6,v3,v4)\n store align 1 v7,v2\n return\n}}\n"
        );
        for operation in ["add", "sub"] {
            s += &format!(
                "func @masked_{operation}_{suffix}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:{mask}):\n v5 = load.{shape} align 1 v0\n v6 = load.{shape} align 1 v1\n v7 = load.{shape} align 1 v2\n v8 = intrinsic x86_avx512_mask{operation}{suffix}(v5,v4,v6,v7)\n store align 1 v8,v3\n v9 = iconst.i64 64\n v10 = ptradd v3,v9\n store align 1 v5,v10\n v11 = ptradd v10,v9\n store align 1 v6,v11\n v12 = ptradd v11,v9\n store align 1 v7,v12\n return\n}}\n"
            );
        }
        s += &format!(
            "func @masked_load_{suffix}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v1\n v5 = intrinsic x86_avx512_maskload{suffix}(v0,v4,v3)\n store align 1 v5,v2\n return\n}}\nfunc @masked_store_{suffix}(ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:{mask}):\n v3 = load.{shape} align 1 v1\n intrinsic x86_avx512_maskstore{suffix}(v0,v3,v2)\n return\n}}\n"
        );
    }
    s
}
fn harness() -> String {
    let mut h = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\nvoid mask_clobber(void){__asm__ volatile(\"kxnorw %k1,%k1,%k1\");}\n",
    );
    let mut checks = String::new();
    for (suffix, scalar, lanes, mask) in [("d", "32", 16, "uint16_t"), ("q", "64", 8, "uint8_t")] {
        for signed in ["", "u"] {
            for control in 0..8 {
                h += &format!("extern void cmp_{signed}{suffix}_{control}(void*,void*,void*);\n");
                let ctype = if signed.is_empty() {
                    format!("int{scalar}_t")
                } else {
                    format!("uint{scalar}_t")
                };
                let expression = match control {
                    0 => "a==b",
                    1 => "a<b",
                    2 => "a<=b",
                    3 => "0",
                    4 => "a!=b",
                    5 => "a>=b",
                    6 => "a>b",
                    _ => "1",
                };
                checks += &format!(
                    "{{ {mask} got=0,expected=0;for(unsigned i=0;i<{lanes};i++){{{ctype} a=({ctype})x[i],b=({ctype})y[i];if({expression})expected|=1u<<i;}}cmp_{signed}{suffix}_{control}(x,y,&got);if(got!=expected)return 10;}}\n"
                );
            }
        }
        h += &format!("extern void cmp_chain_{suffix}(void*,void*,void*);\n");
        checks += &format!(
            "cmp_chain_{suffix}(x,y,out);for(unsigned i=0;i<{lanes};i++)if(out[i]!=(x[i]<y[i]?(uint{scalar}_t)(x[i]+y[i]):x[i]))return 11;\n"
        );
        for (operation, symbol) in [("add", "+"), ("sub", "-")] {
            h += &format!(
                "extern void masked_{operation}_{suffix}(void*,void*,void*,void*,{mask});\n"
            );
            checks += &format!(
                "for(unsigned pattern=0;pattern<5;pattern++){{{mask} k=pattern==0?0:pattern==1?({mask})~0u:pattern==2?({mask})0x5555:pattern==3?1:1u<<({lanes}-1);masked_{operation}_{suffix}(z,x,y,out,k);for(unsigned i=0;i<{lanes};i++)if(out[i]!=((k>>i)&1?(uint{scalar}_t)(x[i]{symbol}y[i]):z[i])||out[i+{lanes}]!=z[i]||out[i+2*{lanes}]!=x[i]||out[i+3*{lanes}]!=y[i])return 20;}}\n"
            );
        }
        h += &format!(
            "extern void masked_load_{suffix}(void*,void*,void*,{mask});\nextern void masked_store_{suffix}(void*,void*,{mask});\n"
        );
        checks += &format!(
            "{{size_t page=sysconf(_SC_PAGESIZE);char*map=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(map==MAP_FAILED)return 30;if(mprotect(map+page,page,PROT_NONE))return 31;for(unsigned n=0;n<={lanes};n++){{uint{scalar}_t*p=(void*)(map+page-n*sizeof(*p));{mask} k=n=={lanes}?({mask})~0u:({mask})((1u<<n)-1);for(unsigned i=0;i<n;i++)p[i]=x[i];masked_load_{suffix}(p,z,out,k);for(unsigned i=0;i<{lanes};i++)if(out[i]!=(i<n?x[i]:z[i]))return 32;masked_store_{suffix}(p,y,k);for(unsigned i=0;i<n;i++)if(p[i]!=y[i])return 33;}}munmap(map,page*2);}}\n"
        );
        // Per-shape arrays keep the C oracle's lane widths independent of IR lowering.
        let body = format!(
            "{{uint{scalar}_t x[{lanes}],y[{lanes}],z[{lanes}],out[{lanes}*4];for(unsigned i=0;i<{lanes};i++){{x[i]=(uint{scalar}_t)(0x8123456789abcdefULL+i*17);y[i]=i%3==0?x[i]:(uint{scalar}_t)(i*19);z[i]=(uint{scalar}_t)(0x76543210fedcba98ULL+i);}}\n{checks}}}\n"
        );
        checks.clear();
        h += &format!("static int check_{suffix}(void){{{body}return 0;}}\n");
    }
    h += "int main(void){int r=check_d();return r?r:check_q();}\n";
    h
}
#[test]
fn avx512_scalar_masks_merge_and_guard_pages() {
    let module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = Features::NONE.with_features("avx512f").unwrap();
    let dir = std::env::temp_dir().join(format!("keel-avx512-mask-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness()).unwrap();
    for (tier, allocator, pools, opt) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
    ] {
        let mut m = module.clone();
        if opt {
            keel_opt::optimize(&mut m).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features: f,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &allocator,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &p).unwrap();
        let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
        let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
        );
        fs::write(dir.join("mask.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                continue;
            }
            let output = Command::new("cc")
                .arg(dir.join("harness.c"))
                .arg(dir.join("mask.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(dir.join("run")).status().unwrap().success());
        }
    }
}
#[test]
fn avx512_mask_contracts_and_feature_rejection() {
    let m = keel_ir::parse(&source()).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
    for predicate in ["-1", "8"] {
        let bad = source().replace("iconst.i32 0", &format!("iconst.i32 {predicate}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    for (id, effect) in [
        (Intrinsic::X86Avx512MaskLoadd, MemoryEffects::Read),
        (Intrinsic::X86Avx512MaskLoadq, MemoryEffects::Read),
        (Intrinsic::X86Avx512MaskStored, MemoryEffects::Write),
        (Intrinsic::X86Avx512MaskStoreq, MemoryEffects::Write),
    ] {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(data.memory_effects(), effect);
        assert!(!id.info().pure);
        assert!(data.may_trap());
    }
}

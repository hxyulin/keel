//! Signed VSIB indices, every scale, immutable operands and inactive fault suppression.
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
const FORMS: [(&str, &str, &str, &str, &str, usize, usize); 8] = [
    (
        "dd",
        "vpgatherdd",
        "vpscatterdd",
        "i32x16",
        "i32x16",
        32,
        16,
    ),
    ("dq", "vpgatherdq", "vpscatterdq", "i32x8", "i64x8", 32, 8),
    ("qd", "vpgatherqd", "vpscatterqd", "i64x8", "i32x8", 64, 8),
    ("qq", "vpgatherqq", "vpscatterqq", "i64x8", "i64x8", 64, 8),
    (
        "dps",
        "vgatherdps",
        "vscatterdps",
        "i32x16",
        "f32x16",
        32,
        16,
    ),
    ("dpd", "vgatherdpd", "vscatterdpd", "i32x8", "f64x8", 32, 8),
    ("qps", "vgatherqps", "vscatterqps", "i64x8", "f32x8", 64, 8),
    ("qpd", "vgatherqpd", "vscatterqpd", "i64x8", "f64x8", 64, 8),
];
fn sources() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdlib.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for (tag, gather, scatter, index, data, ibits, lanes) in FORMS {
        let (mask, cmask) = if lanes == 16 {
            ("i16", "uint16_t")
        } else {
            ("i8", "uint8_t")
        };
        let bytes = if data.starts_with("i64") || data.starts_with("f64") {
            8
        } else {
            4
        };
        let ibytes = ibits / 8 * lanes;
        let dbytes = bytes * lanes;
        for scale in [1, 2, 4, 8] {
            for (prefix, opcode, is_scatter) in [("g", gather, false), ("s", scatter, true)] {
                let call = if is_scatter {
                    format!(" intrinsic x86_avx512_{opcode}(v0,v5,v6,v4,v7)\n")
                } else {
                    format!(
                        " v8 = intrinsic x86_avx512_{opcode}(v0,v5,v6,v4,v7)\n store align 1 v8,v3\n"
                    )
                };
                ir += &format!(
                    "func @{prefix}_{tag}_{scale}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:{mask}):\n v5 = load.{index} align 1 v1\n v6 = load.{data} align 1 v2\n v7 = iconst.i32 {scale}\n{call} v9 = iconst.i64 64\n v10 = ptradd v3,v9\n store align 1 v5,v10\n v11 = ptradd v10,v9\n store align 1 v6,v11\n v12 = ptradd v11,v9\n store align 1 v4,v12\n return\n}}\n"
                );
                c += &format!(
                    "extern void {prefix}_{tag}_{scale}(void*,void*,void*,void*,{cmask});\n"
                );
            }
            cases += &format!(
                "{{int{ibits}_t idx[{lanes}];unsigned char src[{dbytes}],out[208],wanted[{dbytes}];for(unsigned i=0;i<{dbytes};i++)src[i]=(unsigned char)(i*71+13);for(unsigned n=0;n<={lanes};n++){{{cmask} k=n=={lanes}?({cmask})~0u:({cmask})((1u<<n)-1);for(unsigned i=0;i<{lanes};i++)idx[i]=i<n?-(int{ibits}_t)((i+1)*8):(int{ibits}_t)(page/{scale});memcpy(wanted,src,{dbytes});for(unsigned i=0;i<n;i++)memcpy(wanted+i*{bytes},base+(int64_t)idx[i]*{scale},{bytes});memset(out,0xa5,sizeof(out));g_{tag}_{scale}(base,idx,src,out,k);if(memcmp(out,wanted,{dbytes})||memcmp(out+64,idx,{ibytes})||memcmp(out+128,src,{dbytes})||memcmp(out+192,&k,sizeof(k)))return 10;for(unsigned i={dbytes};i<64;i++)if(out[i]!=0xa5)return 11;memcpy(expected,map,page*2);for(unsigned i=0;i<n;i++)memcpy(expected+page+(int64_t)idx[i]*{scale},src+i*{bytes},{bytes});s_{tag}_{scale}(base,idx,src,out,k);if(memcmp(map,expected,page*2)||memcmp(out+64,idx,{ibytes})||memcmp(out+128,src,{dbytes})||memcmp(out+192,&k,sizeof(k)))return 12;}}g_{tag}_{scale}((void*)1,idx,src,out,0);if(memcmp(out,src,{dbytes}))return 13;s_{tag}_{scale}((void*)1,idx,src,out,0);}}\n"
            );
        }
    }
    for (tag, shape, bits, lanes, mask, cmask) in [
        ("dd", "i32x16", 32, 16, "i16", "uint16_t"),
        ("qq", "i64x8", 64, 8, "i8", "uint8_t"),
    ] {
        ir += &format!(
            "func @g_alias_{tag}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{shape} align 1 v1\n v5 = iconst.i32 1\n v6 = intrinsic x86_avx512_vpgather{tag}(v0,v4,v4,v3,v5)\n store align 1 v6,v2\n v7 = iconst.i64 64\n v8 = ptradd v2,v7\n store align 1 v4,v8\n return\n}}\n"
        );
        c += &format!("extern void g_alias_{tag}(void*,void*,void*,{cmask});\n");
        cases += &format!(
            "{{int{bits}_t idx[{lanes}];unsigned char out[128],wanted[64];for(unsigned i=0;i<{lanes};i++)idx[i]=i%2?-(int{bits}_t)((i+1)*8):(int{bits}_t)page;memcpy(wanted,idx,64);for(unsigned i=1;i<{lanes};i+=2)memcpy(wanted+i*sizeof(*idx),base+(int64_t)idx[i],sizeof(*idx));g_alias_{tag}(base,idx,out,({cmask})0xaaaa);if(memcmp(out,wanted,64)||memcmp(out+64,idx,64))return 14;}}\n"
        );
    }
    c += &format!(
        "int main(void){{size_t page=sysconf(_SC_PAGESIZE);unsigned char*map=mmap(0,page*3,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(map==MAP_FAILED)return 1;if(mprotect(map+page*2,page,PROT_NONE))return 2;unsigned char*base=map+page,*expected=malloc(page*2);if(!expected)return 3;for(unsigned i=0;i<page*2;i++)map[i]=(unsigned char)(i*37+11);{cases}free(expected);munmap(map,page*3);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn avx512_gather_scatter_signed_indices_and_fault_suppression() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2);
    let dir = std::env::temp_dir().join(format!("keel-avx512-indexed-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for (tier, alloc, pools, opt) in [
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
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &p).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("indexed.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                continue;
            }
            let output = Command::new("cc")
                .arg(dir.join("harness.c"))
                .arg(dir.join("indexed.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "indexed oracle: {status}");
        }
    }
}
#[test]
fn avx512_indexed_contracts_features_and_scale() {
    let (source, _) = sources();
    let m = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
    for scale in ["0", "3", "16", "-1"] {
        let bad = source.replace("iconst.i32 1", &format!("iconst.i32 {scale}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_indexed())
    {
        assert!(!id.info().pure);
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(data.is_pinned());
        assert!(data.may_trap());
        assert_eq!(
            data.memory_effects(),
            if id.is_x86_avx512_scatter() {
                MemoryEffects::Write
            } else {
                MemoryEffects::Read
            }
        );
    }
}

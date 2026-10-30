//! DQ lane values preserve every bit across widths, masks and allocation paths.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
// Opcode stem, kind, short input, result, mask, encoded width, maximum index.
const FORMS: &[(&str, &str, &str, &str, &str, u16, u16)] = &[
    ("broadcasti32x2", "Register", "i32x4", "i32x4", "i8", 128, 0),
    ("broadcastf32x2", "Register", "f32x4", "f32x8", "i8", 256, 0),
    ("broadcastf64x2", "Memory", "f64x2", "f64x4", "i8", 256, 0),
    ("broadcasti32x2", "Register", "i32x4", "i32x8", "i8", 256, 0),
    ("broadcasti64x2", "Memory", "i64x2", "i64x4", "i8", 256, 0),
    ("extractf64x2", "Extract", "f64x4", "f64x2", "i8", 256, 1),
    ("extracti64x2", "Extract", "i64x4", "i64x2", "i8", 256, 1),
    ("insertf64x2", "Insert", "f64x2", "f64x4", "i8", 256, 1),
    ("inserti64x2", "Insert", "i64x2", "i64x4", "i8", 256, 1),
    (
        "broadcastf32x2",
        "Register",
        "f32x4",
        "f32x16",
        "i16",
        512,
        0,
    ),
    ("broadcastf32x8", "Memory", "f32x8", "f32x16", "i16", 512, 0),
    ("broadcastf64x2", "Memory", "f64x2", "f64x8", "i8", 512, 0),
    (
        "broadcasti32x2",
        "Register",
        "i32x4",
        "i32x16",
        "i16",
        512,
        0,
    ),
    ("broadcasti32x8", "Memory", "i32x8", "i32x16", "i16", 512, 0),
    ("broadcasti64x2", "Memory", "i64x2", "i64x8", "i8", 512, 0),
    ("extractf32x8", "Extract", "f32x16", "f32x8", "i8", 512, 1),
    ("extractf64x2", "Extract", "f64x8", "f64x2", "i8", 512, 3),
    ("extracti32x8", "Extract", "i32x16", "i32x8", "i8", 512, 1),
    ("extracti64x2", "Extract", "i64x8", "i64x2", "i8", 512, 3),
    ("insertf32x8", "Insert", "f32x8", "f32x16", "i16", 512, 1),
    ("insertf64x2", "Insert", "f64x2", "f64x8", "i8", 512, 3),
    ("inserti32x8", "Insert", "i32x8", "i32x16", "i16", 512, 1),
    ("inserti64x2", "Insert", "i64x2", "i64x8", "i8", 512, 3),
];
fn bytes(shape: &str) -> usize {
    let (bits, lanes) = shape[1..].split_once('x').unwrap();
    bits.parse::<usize>().unwrap() * lanes.parse::<usize>().unwrap() / 8
}
fn fixtures() -> (String, String) {
    let mut source = String::new();
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for &(op, kind, input, output, mask, width, max) in FORMS {
        let insert = kind == "Insert";
        let extract = kind == "Extract";
        let whole = if insert { output } else { input };
        let rbytes = bytes(output);
        let abytes = bytes(whole);
        let bbytes = if insert { bytes(input) } else { 0 };
        let bits = if output.starts_with("f32") || output.starts_with("i32") {
            32
        } else {
            64
        };
        let lanes = rbytes * 8 / bits;
        let alanes = abytes * 8 / bits;
        let blanes = if insert { bbytes * 8 / bits } else { 1 };
        let uint = if bits == 32 { "uint32_t" } else { "uint64_t" };
        let cmask = if mask == "i16" { "uint16_t" } else { "uint8_t" };
        let mbytes = if mask == "i16" { 2 } else { 1 };
        let block = if insert {
            blanes
        } else if extract {
            lanes
        } else {
            op.rsplit_once('x').unwrap().1.parse::<usize>().unwrap()
        };
        for control in 0..=max {
            let name = format!("{op}_{width}_{control}");
            let load_b = if insert {
                format!(" v7 = load.{input} align 1 v2\n")
            } else {
                String::new()
            };
            let args = if insert {
                "v5,v4,v6,v7,v8"
            } else if extract {
                "v5,v4,v6,v8"
            } else {
                "v5,v4,v6"
            };
            let store_b = if insert {
                " store align 1 v7,v13\n"
            } else {
                ""
            };
            source += &format!(
                "func @{name}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:{mask}):\n v5 = load.{output} align 1 v0\n v6 = load.{whole} align 1 v1\n{load_b} v8 = iconst.i32 {control}\n v9 = intrinsic x86_avx512dq_{op}_{width}({args})\n store align 1 v9,v3\n v10 = iconst.i64 64\n v11 = ptradd v3,v10\n store align 1 v5,v11\n v12 = ptradd v11,v10\n store align 1 v6,v12\n v13 = ptradd v12,v10\n{store_b} v14 = ptradd v13,v10\n store align 1 v4,v14\n return\n}}\n"
            );
            harness += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,{cmask});\n"
            );
            let select = if insert {
                format!("(i/{block}=={control}?b[i%{block}]:a[i])")
            } else if extract {
                format!("a[{control}*{block}+i]")
            } else {
                format!("a[i%{block}]")
            };
            let partfill = if insert {
                format!(
                    "for(unsigned i=0;i<{blanes};i++)b[i]=({uint})edges[(i*3+5+sample)%8];memcpy(gb,b,{bbytes});"
                )
            } else {
                String::new()
            };
            let partcheck = if insert {
                format!("if(memcmp(out+192,b,{bbytes}))return 4;")
            } else {
                String::new()
            };
            let bpointer = if insert { "alias==3?out:gb" } else { "0" };
            cases += &format!(
                "{{{uint} a[{alanes}],b[{blanes}],s[{lanes}],expected[{lanes}];unsigned char out[288];unsigned char*gs=g[0]+page-{rbytes},*ga=g[1]+page-{abytes},*gb=g[2]+page-{bbytes};{cmask} masks[]={{0,({cmask})~0u,1,({cmask})0xaaaa,({cmask})(1u<<({lanes}-1)),({cmask})~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<2;sample++){{for(unsigned i=0;i<{lanes};i++)s[i]=({uint})edges[(i*5+2+sample)%8];for(unsigned i=0;i<{alanes};i++)a[i]=({uint})edges[(i+sample)%8]^(sample?0:({uint})(i*17));{partfill}memcpy(gs,s,{rbytes});memcpy(ga,a,{abytes});for(unsigned mi=0;mi<6;mi++)for(unsigned alias=0;alias<{};alias++){{{cmask} k=masks[mi];for(unsigned i=0;i<{lanes};i++)expected[i]=(k&(1u<<i))?{select}:s[i];memset(out,0xa5,sizeof out);if(alias==1)memcpy(out,a,{abytes});if(alias==2)memcpy(out,s,{rbytes});if(alias==3)memcpy(out,b,{bbytes});unsigned csr=((saved|63)&~0x80u)^(sample?0x8040:0);_mm_setcsr(csr);{name}(alias==2?out:gs,alias==1?out:ga,{bpointer},out,k);if(_mm_getcsr()!=csr)return 1;if(memcmp(out,expected,{rbytes})){{fprintf(stderr,\"{name} mask%u alias%u\\n\",mi,alias);return 2;}}if(memcmp(out+64,s,{rbytes})||memcmp(out+128,a,{abytes})||memcmp(out+256,&k,{mbytes}))return 3;{partcheck}for(unsigned i={rbytes};i<64;i++)if(out[i]!=(alias==1&&i<{abytes}?((unsigned char*)a)[i]:0xa5)||out[i+64]!=0xa5)return 5;for(unsigned i={abytes};i<64;i++)if(out[i+128]!=0xa5)return 6;for(unsigned i={bbytes};i<64;i++)if(out[i+192]!=0xa5)return 7;for(unsigned i={mbytes};i<32;i++)if(out[i+256]!=0xa5)return 8;_mm_setcsr(saved);}}}}}}\n",
                if insert { 4 } else { 3 }
            );
        }
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*g[3];for(unsigned i=0;i<3;i++){{g[i]=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(g[i]==MAP_FAILED)return 90;if(mprotect(g[i]+page,page,PROT_NONE))return 91;}}uint64_t edges[8]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x7ff000007f800001),UINT64_C(0xfff00000ff800001),UINT64_C(0x8000000080000000)}};{cases}for(unsigned i=0;i<3;i++)munmap(g[i],page*2);_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
#[test]
fn dq_lane_values_masks_guards_aliases_and_spills() {
    let (source, harness) = fixtures();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE
        .with_features("avx512f,avx512dq,avx512vl")
        .unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-dq-lane-{}", std::process::id()));
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
        (SelectionTier::Full, AllocationTier::Full, &ONE, true),
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
        for opcode in [
            "vbroadcasti32x2",
            "vbroadcastf32x2",
            "vbroadcastf32x8",
            "vbroadcastf64x2",
            "vbroadcasti32x8",
            "vbroadcasti64x2",
            "vextractf32x8",
            "vextractf64x2",
            "vextracti32x8",
            "vextracti64x2",
            "vinsertf32x8",
            "vinsertf64x2",
            "vinserti32x8",
            "vinserti64x2",
        ] {
            assert!(text.contains(opcode), "missing {opcode}");
        }
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("convert.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512dq") {
                continue;
            }
            let output = Command::new("cc")
                .args([
                    "-O1",
                    "-mavx512f",
                    "-mavx512vl",
                    "-mavx512dq",
                    "-frounding-math",
                ])
                .arg(dir.join("harness.c"))
                .arg(dir.join("convert.o"))
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
            assert!(status.success(), "conversion oracle: {status}");
        }
    }
}
#[test]
fn dq_lane_contracts_and_controls() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_dq_lane().is_some())
        .collect();
    assert_eq!(ids.len(), 23);
    for id in ids {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure);
        assert!(!data.is_pinned() && !data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::None);
    }
    let (source, _) = fixtures();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for flags in ["avx512f,avx512vl", "avx512f,avx512dq"] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features(flags).unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                .is_err()
        );
    }
    for value in [-1, 4] {
        let bad = source.replace("iconst.i32 0", &format!("iconst.i32 {value}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let wide: String = source
        .split("}\n")
        .filter(|body| body.contains("_512_"))
        .map(|body| format!("{body}}}\n"))
        .collect();
    let wide = keel_ir::parse(&wide).unwrap();
    let features = Features::NONE.with_features("avx512f,avx512dq").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&wide, &EMPTY, &pipeline).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
}

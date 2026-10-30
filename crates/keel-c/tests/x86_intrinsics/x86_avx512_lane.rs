//! C lane wrappers preserve operand order, short widths and single evaluation.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Api {
    api: String,
    kind: u8,
    full: &'static str,
    small: &'static str,
    bits: u32,
    max: i32,
    bytes: usize,
}
fn apis() -> Vec<Api> {
    let mut result = Vec::new();
    for bits in [32, 64] {
        for op in ["slli", "srli", "srai", "sll", "srl", "sra"] {
            result.push(Api {
                api: format!("{op}_epi{bits}"),
                kind: if op.ends_with('i') { 0 } else { 1 },
                full: "i512",
                small: "i128",
                bits,
                max: 255,
                bytes: 64,
            });
        }
    }
    for (api, full, small, bits) in [
        ("broadcastd_epi32", "i512", "i128", 32),
        ("broadcastq_epi64", "i512", "i128", 64),
        ("broadcastss_ps", "ps512", "ps128", 32),
        ("broadcastsd_pd", "pd512", "pd128", 64),
    ] {
        result.push(Api {
            api: api.into(),
            kind: 2,
            full,
            small,
            bits,
            max: 0,
            bytes: 64,
        });
    }
    for bits in [32, 64] {
        result.push(Api {
            api: format!("set1_epi{bits}"),
            kind: 6,
            full: "i512",
            small: if bits == 32 { "s32" } else { "s64" },
            bits,
            max: 0,
            bytes: 64,
        });
    }
    for (block, suffix, full, small, bits, max, bytes) in [
        ("i32x4", "epi32", "i512", "i128", 32, 3, 16),
        ("i64x4", "epi64", "i512", "i256", 64, 1, 32),
        ("f32x4", "ps", "ps512", "ps128", 32, 3, 16),
        ("f64x4", "pd", "pd512", "pd256", 64, 1, 32),
    ] {
        result.push(Api {
            api: format!("broadcast_{block}"),
            kind: 3,
            full,
            small,
            bits,
            max: 0,
            bytes: 64,
        });
        result.push(Api {
            api: format!("extract{block}_{suffix}"),
            kind: 4,
            full,
            small,
            bits,
            max,
            bytes,
        });
        result.push(Api {
            api: format!("insert{block}"),
            kind: 5,
            full,
            small,
            bits,
            max,
            bytes: 64,
        });
    }
    result
}
fn ty(shape: &str) -> &'static str {
    match shape {
        "i512" => "__m512i",
        "i256" => "__m256i",
        "i128" => "__m128i",
        "ps512" => "__m512",
        "pd512" => "__m512d",
        "ps128" => "__m128",
        "pd128" => "__m128d",
        "pd256" => "__m256d",
        "s32" => "int",
        "s64" => "long long",
        _ => unreachable!(),
    }
}
fn size(shape: &str) -> usize {
    match shape {
        "i512" | "ps512" | "pd512" => 64,
        "i256" | "pd256" => 32,
        "i128" | "ps128" | "pd128" => 16,
        "s32" => 4,
        "s64" => 8,
        _ => unreachable!(),
    }
}
fn source() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(vector_count),FN(mask_count),FN(scalar_count),FN(pointer_count);\n",
    );
    for shape in [
        "i512", "i256", "i128", "ps512", "pd512", "ps128", "pd128", "pd256", "s32", "s64",
    ] {
        let ctype = ty(shape);
        let bytes = size(shape);
        let counter = if shape.starts_with('s') {
            "scalar_count"
        } else {
            "vector_count"
        };
        source += &format!(
            "static __inline__ __attribute__((always_inline)) {ctype} take_{shape}(const void*p){{{ctype} result;FN({counter})++;__builtin_memcpy(&result,p,{bytes});return result;}}\n"
        );
    }
    source += "static __inline__ __attribute__((always_inline)) unsigned take_mask(unsigned k){FN(mask_count)++;return k;}\nstatic __inline__ __attribute__((always_inline)) const void*take_pointer(const void*p){FN(pointer_count)++;return p;}\n";
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nextern int vector_count,mask_count,scalar_count,pointer_count,reference_vector_count,reference_mask_count,reference_scalar_count,reference_pointer_count;\ntypedef void(*Fn)(const void*,const void*,const void*,void*,unsigned);\nstruct Case{Fn actual,reference;unsigned bytes,vectors,masks,scalars,pointers,kind,capacity;};\n",
    );
    let mut cases = String::new();
    let mut number = 0;
    for api in apis() {
        let modes: &[&str] = if api.kind == 6 {
            &["mask_", "maskz_"]
        } else {
            &["", "mask_", "maskz_"]
        };
        let controls: Vec<i32> = if api.kind == 0 {
            vec![0, api.bits as i32, 255]
        } else if matches!(api.kind, 4 | 5) {
            vec![0, api.max]
        } else {
            vec![0]
        };
        for control in controls {
            for &mode in modes {
                let result = if api.kind == 4 {
                    ty(api.small)
                } else {
                    ty(api.full)
                };
                let merge = if api.kind == 4 { api.small } else { api.full };
                let mut args = match api.kind {
                    0 => vec![format!("take_{}(a)", api.full), control.to_string()],
                    1 => vec![format!("take_{}(a)", api.full), "take_i128(b)".into()],
                    2 | 3 => vec![format!("take_{}(a)", api.small)],
                    4 => vec![format!("take_{}(a)", api.full), control.to_string()],
                    5 => vec![
                        format!("take_{}(a)", api.full),
                        format!("take_{}(b)", api.small),
                        control.to_string(),
                    ],
                    6 => vec![format!("take_{}(a)", api.small)],
                    _ => unreachable!(),
                };
                match mode {
                    "mask_" => {
                        args.insert(0, "take_mask(k)".into());
                        args.insert(0, format!("take_{merge}(m)"));
                    }
                    "maskz_" => args.insert(0, "take_mask(k)".into()),
                    _ => {}
                }
                let vectors = args
                    .iter()
                    .filter(|arg| {
                        arg.starts_with("take_")
                            && !arg.starts_with("take_mask")
                            && !arg.starts_with("take_s")
                    })
                    .count();
                let scalars = args.iter().filter(|arg| arg.starts_with("take_s")).count();
                let masks = usize::from(!mode.is_empty());
                source += &format!(
                    "void FN(case_{number})(const void*m,const void*a,const void*b,void*out,unsigned k){{{result} result=_mm512_{mode}{}({});__builtin_memcpy(out,&result,{});}}\n",
                    api.api,
                    args.join(","),
                    api.bytes
                );
                harness += &format!(
                    "extern void case_{number}(const void*,const void*,const void*,void*,unsigned),reference_case_{number}(const void*,const void*,const void*,void*,unsigned);\n"
                );
                cases += &format!(
                    "{{case_{number},reference_case_{number},{},{vectors},{masks},{scalars},0,0,0}},\n",
                    api.bytes
                );
                number += 1;
            }
        }
    }
    // Pointer-only forms use compiler-owned names. Reference helpers model safe
    // complete tuples and zero masks; backend tests cover partial tuple faults.
    source += "#ifdef REFERENCE\n";
    for (api, full, small, scalar, capacity) in [
        ("broadcastd_epi32", "i512", "i128", "int", 4),
        ("broadcastq_epi64", "i512", "i128", "long long", 8),
        ("broadcastss_ps", "ps512", "ps128", "float", 4),
        ("broadcastsd_pd", "pd512", "pd128", "double", 8),
    ] {
        let result = ty(full);
        let short = ty(small);
        let mask = if capacity == 4 {
            "__mmask16"
        } else {
            "__mmask8"
        };
        source += &format!(
            "static __inline__ {result} ref_load_{api}({result} src,{mask} k,const void*p){{if(!k)return src;{short} a=({short}){{0}};__builtin_memcpy(&a,p,{capacity});return _mm512_mask_{api}(src,k,a);}}\n#define __keel_mm512_mask_load_{api}(s,k,p) ref_load_{api}((s),(k),(p))\n"
        );
        let _ = scalar;
    }
    for (block, suffix, full, small, width, capacity) in [
        ("i32x4", "epi32", "i512", "i128", 4, 16),
        ("i64x4", "epi64", "i512", "i256", 8, 32),
        ("f32x4", "ps", "ps512", "ps128", 4, 16),
        ("f64x4", "pd", "pd512", "pd256", 8, 32),
    ] {
        let result = ty(full);
        let short = ty(small);
        let mask = if width == 4 { "__mmask16" } else { "__mmask8" };
        source += &format!(
            "static __inline__ {result} ref_load_broadcast_{block}({result} src,{mask} k,const void*p){{if(!k)return src;{short} a;__builtin_memcpy(&a,p,{capacity});return _mm512_mask_broadcast_{block}(src,k,a);}}\n#define __keel_mm512_mask_load_broadcast_{block}(s,k,p) ref_load_broadcast_{block}((s),(k),(p))\n#define __keel_mm512_mask_load_insert{block}(s,k,a,p,lane) ({{{result} src=(s),base=(a),result;{mask} mask=(k);const void*ptr=(p);if(!mask)result=src;else{{{short} value;__builtin_memcpy(&value,ptr,{capacity});result=_mm512_mask_insert{block}(src,mask,base,value,(lane));}}result;}})\n#define __keel_mm512_mask_store_extract{block}_{suffix}(p,k,a,lane) ({{void*ptr=(void*)(p);__mmask8 mask=(k);{short} value=_mm512_extract{block}_{suffix}((a),(lane));for(unsigned i=0;i<4;i++)if((mask>>i)&1)__builtin_memcpy((char*)ptr+i*{width},(char*)&value+i*{width},{width});}})\n"
        );
    }
    source += "#endif\n";
    let mut private = Vec::new();
    for (api, full, capacity) in [
        ("broadcastd_epi32", "i512", 4),
        ("broadcastq_epi64", "i512", 8),
        ("broadcastss_ps", "ps512", 4),
        ("broadcastsd_pd", "pd512", 8),
    ] {
        private.push((format!("load_{api}"), full, 0, capacity, 0));
    }
    for (block, suffix, full, width, capacity, max) in [
        ("i32x4", "epi32", "i512", 4, 16, 3),
        ("i64x4", "epi64", "i512", 8, 32, 1),
        ("f32x4", "ps", "ps512", 4, 16, 3),
        ("f64x4", "pd", "pd512", 8, 32, 1),
    ] {
        private.push((format!("load_broadcast_{block}"), full, 0, capacity, 0));
        private.push((format!("load_insert{block}"), full, 1, capacity, max));
        private.push((
            format!("store_extract{block}_{suffix}"),
            full,
            2,
            capacity,
            max,
        ));
        let _ = width;
    }
    for (api, full, kind, capacity, control) in private {
        let result = ty(full);
        let args = if kind == 2 {
            format!("take_pointer(a),take_mask(k),take_{full}(m),{control}")
        } else if kind == 1 {
            format!("take_{full}(m),take_mask(k),take_{full}(b),take_pointer(a),{control}")
        } else {
            format!("take_{full}(m),take_mask(k),take_pointer(a)")
        };
        let expression = format!("__keel_mm512_mask_{api}({args})");
        let statement = if kind == 2 {
            format!("{expression};")
        } else {
            format!("{result} result={expression};__builtin_memcpy(out,&result,64);")
        };
        source += &format!(
            "void FN(case_{number})(const void*m,const void*a,const void*b,void*out,unsigned k){{{statement}}}\n"
        );
        harness += &format!(
            "extern void case_{number}(const void*,const void*,const void*,void*,unsigned),reference_case_{number}(const void*,const void*,const void*,void*,unsigned);\n"
        );
        cases += &format!(
            "{{case_{number},reference_case_{number},64,{},1,0,1,{},{} }},\n",
            if kind == 1 { 2 } else { 1 },
            kind + 1,
            capacity
        );
        number += 1;
    }
    harness += &format!(
        r#"
static const struct Case cases[]={{{cases}}};
int main(void){{
 long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;
 unsigned char a[64],b[64],m[64],out[64],expected[64],saved[64];
 for(unsigned i=0;i<64;i++){{a[i]=(unsigned char)(0xf1-i*13);b[i]=(unsigned char)(0x51+i*73);m[i]=(unsigned char)(0x81+i*37);}}
 uint64_t counts[]={{0,31,32,63,64,UINT64_MAX}};
 for(unsigned count=0;count<6;count++){{memcpy(b,&counts[count],8);uint64_t upper=UINT64_MAX;memcpy(b+8,&upper,8);
 for(unsigned pattern=0;pattern<5;pattern++){{unsigned k=(unsigned[]){{0,0xffff,0xa55a,0x80,0xf0}}[pattern];
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){{const struct Case*c=&cases[ci];vector_count=mask_count=scalar_count=pointer_count=reference_vector_count=reference_mask_count=reference_scalar_count=reference_pointer_count=0;memset(out,0xa5,64);memset(expected,0xa5,64);
  if(!c->kind){{c->actual(m,a,b,out,k);c->reference(m,a,b,expected,k);}}
  else{{unsigned active=k&(c->kind==3?15:(c->capacity==4||c->capacity==16?65535:255));unsigned char*p=(active||c->kind==2||c->kind==3)?guard+2*page-c->capacity:(void*)1;memset(guard+page,0xa5,page);if(c->kind!=3&&active)memcpy(p,a,c->capacity);c->actual(m,p,b,out,k);if(c->kind==3)memcpy(saved,guard+2*page-c->capacity,c->capacity);memset(guard+page,0xa5,page);if(c->kind!=3&&active)memcpy(p,a,c->capacity);c->reference(m,p,b,expected,k);if(c->kind==3&&memcmp(saved,guard+2*page-c->capacity,c->capacity))return 3;}}
  if(memcmp(out,expected,64)||vector_count!=c->vectors||reference_vector_count!=c->vectors||mask_count!=c->masks||reference_mask_count!=c->masks||scalar_count!=c->scalars||reference_scalar_count!=c->scalars||pointer_count!=c->pointers||reference_pointer_count!=c->pointers){{fprintf(stderr,"lane C case%u count%u mask%u resultbytes%u counters %d/%d %d/%d %d/%d %d/%d\n",ci,count,pattern,c->bytes,vector_count,reference_vector_count,mask_count,reference_mask_count,scalar_count,reference_scalar_count,pointer_count,reference_pointer_count);return 4;}}
 }}}}}}
 return 0;
}}
"#
    );
    (source, harness)
}
fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_c_lane_controls_and_all_typed_routes() {
    for api in apis()
        .into_iter()
        .filter(|api| matches!(api.kind, 0 | 4 | 5))
    {
        let result = if api.kind == 4 {
            ty(api.small)
        } else {
            ty(api.full)
        };
        for control in ["-1".to_owned(), (api.max + 1).to_string(), "n".into()] {
            let args = if api.kind == 5 {
                format!("a,b,{control}")
            } else {
                format!("a,{control}")
            };
            let text = format!(
                "#include <immintrin.h>\n{result} f({} a,{} b,int n){{return _mm512_{}({args});}}\n",
                ty(api.full),
                ty(api.small),
                api.api
            );
            assert!(
                parse(&text, Target::X86_64LinuxGnu).is_err(),
                "{} {control}",
                api.api
            );
        }
    }
    let (source, _) = source();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let covered: std::collections::HashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| match f.inst_data(i) {
                keel_ir::InstData::Intrinsic { id, .. } if id.is_x86_avx512_lane() => Some(*id),
                _ => None,
            })
        })
        .collect();
    assert_eq!(covered.len(), 44);
}
#[test]
fn avx512_c_lane_native_wrappers_and_single_evaluation() {
    let (source, harness) = source();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-lane-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    };
    if native {
        command(
            Command::new("cc")
                .args([
                    "-mavx512f",
                    "-mno-avx512dq",
                    "-mno-avx512bw",
                    "-O1",
                    "-DREFERENCE",
                    "-c",
                ])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-c"])
                .arg(dir.join("harness.c"))
                .arg("-o")
                .arg(dir.join("harness.o")),
        );
    }
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
        let mut module = original.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(dir.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(dir.join("actual.o"))
                    .arg(dir.join("reference.o"))
                    .arg(dir.join("harness.o"))
                    .arg("-o")
                    .arg(dir.join("run")),
            );
            command(&mut Command::new(dir.join("run")));
        }
    }
}

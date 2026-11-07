//! Native integer lane extraction retains explicit signed extension.
#[path = "../arm_narrow/oracle.rs"]
mod support;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};

#[test]
fn packed_half_concatenations_preserve_bits_aliases_and_spills() {
    for high in [false, true] {
        let mut ir = String::new();
        let mut c = String::from(include_str!("../vector_shuffle/header.c"));
        let mut cases = String::from("static const struct Case cases[]={\n");
        for (shape, lane, count) in [
            ("i8x16", "unsigned char", 16),
            ("i16x8", "unsigned short", 8),
            ("i32x4", "unsigned", 4),
            ("f32x4", "float", 4),
        ] {
            c += &format!("typedef {lane} V_{shape} __attribute__((vector_size(16)));\n");
            let start = if high { count / 2 } else { 0 };
            let lanes = (start..start + count / 2)
                .chain(count + start..count + start + count / 2)
                .map(|lane| lane.to_string())
                .collect::<Vec<_>>()
                .join(",");
            for layout in super::LAYOUTS {
                let name = format!("concat_{shape}_{layout}");
                let same = layout.starts_with("same");
                let b = if same { "v0" } else { "v1" };
                ir += &format!(
                    "func @{name}({shape},{shape})->{shape} {{\nblock0(v0:{shape},v1:{shape}):\n v2=vshuffle v0,{b},[{lanes}]\n return v2\n}}\n"
                );
                c += &format!(
                    "extern V_{shape} {name}(V_{shape},V_{shape});\nstatic void call_{name}(const void*p,const void*q,void*o){{V_{shape} a,b,r;memcpy(&a,p,16);memcpy(&b,q,16);r={name}(a,b);memcpy(o,&r,16);}}\n"
                );
                cases += &format!("{{\"{name}\",call_{name},{},0}},\n", u8::from(same));
            }
        }
        let offset = if high { 8 } else { 0 };
        let checks = include_str!("../vector_shuffle/oracle.c").replace(
            "memcpy(expected,x+8,8);memcpy(expected+8,c->same?x:y,8);",
            &format!("memcpy(expected,x+{offset},8);memcpy(expected+8,(c->same?x:y)+{offset},8);"),
        );
        c += &(cases + "};\n" + &checks);
        let module = keel_ir::parse(&ir).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut oracle = support::Oracle::new("half-concat");
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            for forced in [false, true] {
                let allocator: &dyn keel_codegen::regalloc::RegisterAllocator = if forced {
                    &super::Controlled
                } else {
                    &AllocationTier::Full
                };
                let pools = if forced {
                    &keel_codegen::aarch64::REGISTERS
                } else {
                    &support::EMPTY
                };
                let (text, obj) = support::compile(&module, &sel, allocator, pools);
                assert_eq!(
                    text.matches(if high { "zip2\t" } else { "zip1\t" }).count(),
                    36,
                    "{text}"
                );
                assert!(!text.contains("ins\t"), "{text}");
                oracle.check(&format!("{high}-{sel:?}-{forced}"), &text, &obj, &c);
            }
        }
    }
}

#[test]
fn every_integer_lane_typed_text_clang_and_native() {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\ntypedef int32_t V __attribute__((vector_size(16)));\n",
    );
    for lane in 0..4 {
        ir += &format!(
            "func @extract{lane}(i32x4) -> i64 {{\nblock0(v0:i32x4):\n v1 = vextract v0,{lane}\n v2 = sext.i64 v1\n return v2\n}}\n"
        );
        c += &format!("extern int64_t extract{lane}(V);\n");
    }
    c += "int main(void){const int32_t edges[]={INT32_MIN,INT32_MAX,-1,0,1,0x12345678}; for(unsigned p=0;p<1000;p++){V v; for(unsigned l=0;l<4;l++)v[l]=edges[(p+l)%6];";
    for lane in 0..4 {
        c += &format!(
            "if(extract{lane}(v)!=(int64_t)v[{lane}])return {};",
            lane + 1
        );
    }
    c += "}return 0;}\n";
    let m = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut oracle = support::Oracle::new("extract");
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &support::EMPTY] {
                let (text, obj) = support::compile(&m, &sel, &alloc, pools);
                assert_eq!(text.matches("umov\tw").count(), 4, "{text}");
                for lane in 0..4 {
                    assert!(text.contains(&format!(".s[{lane}]")));
                }
                oracle.check(
                    &format!(
                        "extract-{sel:?}-{alloc:?}-{}",
                        pools.float_caller_saved.len()
                    ),
                    &text,
                    &obj,
                    &c,
                );
            }
        }
    }
}

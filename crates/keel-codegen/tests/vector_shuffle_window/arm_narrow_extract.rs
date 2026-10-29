//! Native integer lane extraction retains explicit signed extension.
#[path = "../arm_narrow/oracle.rs"]
mod support;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
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

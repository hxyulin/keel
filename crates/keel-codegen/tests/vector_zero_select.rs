//! Shared canonical masks and zero arms, including arbitrary-mask boundaries.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use std::fs;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::process::Command;
const SPILLED: Pools = Pools {
    float_caller_saved: &[],
    float_callee_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n");
    let mut main = String::from(
        "int main(void){const uint64_t edges[]={0,1,2,0x80,0xffff,0x80000000ULL,0x8000000000000000ULL,~0ULL,0x7ff8000012345678ULL};\n",
    );
    for (bits, float) in [
        (8, false),
        (16, false),
        (32, false),
        (64, false),
        (32, true),
        (64, true),
    ] {
        for canonical in [false, true] {
            for negative in [false, true] {
                if negative && !float {
                    continue;
                }
                let lanes = 128 / bits;
                let mask_shape = format!("i{bits}x{lanes}");
                let shape = format!("{}{bits}x{lanes}", if float { "f" } else { "i" });
                let name = format!("zero_{bits}_{float}_{canonical}_{negative}");
                let mask = if canonical {
                    " v6 = vicmp.slt v3,v5\n".to_string()
                } else {
                    format!(" v6 = load.{mask_shape} align 1 v1\n")
                };
                let scalar_zero = if float {
                    format!("fconst.f{bits} {}0.0", if negative { "-" } else { "" })
                } else {
                    format!("iconst.i{bits} 0")
                };
                let cast = if float {
                    format!(" v7 = bitcast.{shape} v3\n")
                } else {
                    format!(" v7 = load.{shape} align 1 v0\n")
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{mask_shape} align 1 v0\n v4 = iconst.i{bits} 0\n v5 = vsplat.{mask_shape} v4\n{mask}{cast} v8 = {scalar_zero}\n v9 = vsplat.{shape} v8\n v10 = vselect v6,v9,v7\n v11 = vselect v6,v7,v9\n store align 1 v10,v2\n v12 = iconst.i64 16\n v13 = ptradd v2,v12\n store align 1 v11,v13\n v14 = iconst.i64 32\n v15 = ptradd v2,v14\n store align 1 v6,v15\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                let zero = if negative {
                    format!("((uint{bits}_t)1<<{})", bits - 1)
                } else {
                    "0".into()
                };
                main += &format!(
                    "for(unsigned k=0;k<9;k++)for(unsigned j=0;j<9;j++){{unsigned char a[16],b[16],expected[48],aa[80],bb[80],out[80];for(unsigned z=0;z<16;z++){{a[z]=(unsigned char)(edges[(k+z/8)%9]>>(8*(z%8)));b[z]=(unsigned char)(edges[(j+z/8)%9]>>(8*(z%8)));}}for(unsigned l=0;l<{lanes};l++){{uint{bits}_t x,m,t,f;int{bits}_t sx;memcpy(&x,a+l*sizeof(x),sizeof(x));memcpy(&sx,&x,sizeof(x));"
                );
                main += &if canonical {
                    format!("m=sx<0?(uint{bits}_t)~(uint{bits}_t)0:0;")
                } else {
                    "memcpy(&m,b+l*sizeof(m),sizeof(m));".to_string()
                };
                main += &format!(
                    "t=m?{zero}:x;f=m?x:{zero};memcpy(expected+l*sizeof(t),&t,sizeof(t));memcpy(expected+16+l*sizeof(f),&f,sizeof(f));memcpy(expected+32+l*sizeof(m),&m,sizeof(m));}}for(unsigned alias=0;alias<3;alias++){{memcpy(aa+1,a,16);memcpy(bb+1,b,16);unsigned char *r=alias==1?aa+1:alias==2?bb+1:out+1;{name}(aa+1,bb+1,r);if(memcmp(r,expected,48)){{fprintf(stderr,\"{name} k=%u j=%u alias=%u\\n\",k,j,alias);return 1;}}}}}}\n"
                );
            }
        }
    }
    main += "puts(\"3888 zero-select checks passed\");return 0;}\n";
    (ir, c + &main)
}
#[test]
fn zero_arm_selection_preserves_shared_masks_spills_and_exact_bits() {
    let (ir, c) = fixture();
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("zero-select");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocation,
                };
                let pools = if spilled {
                    &SPILLED
                } else {
                    &keel_codegen::x86_64::REGISTERS
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                let typed =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let object = keel_asm::x86::assemble_object(&text).unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_program_object(&typed).unwrap()
                );
                for f in text.split(".globl").skip(1) {
                    let name = f.lines().next().unwrap();
                    let fast = name.trim().trim_matches('"').ends_with("_true_false"); // canonical mask, positive zero
                    if fast {
                        assert!(f.contains("andnps\t"), "{f}");
                        assert!(
                            !f.split(" = vselect ").nth(1).unwrap().contains("xorps\t"),
                            "{f}"
                        );
                    }
                    if name.trim().trim_matches('"').ends_with("_true_true") {
                        assert!(
                            f.contains("xorps\t"),
                            "negative zero must retain its bits: {f}"
                        );
                    }
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                {
                    fs::write(dir.join("kernels.o"), object).unwrap();
                    let output = Command::new("clang")
                        .current_dir(&dir)
                        .args([
                            "-O2",
                            "-fno-vectorize",
                            "-fno-slp-vectorize",
                            "oracle.c",
                            "kernels.o",
                            "-o",
                            "oracle",
                        ])
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let output = Command::new(dir.join("oracle")).output().unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(
                        String::from_utf8(output.stdout)
                            .unwrap()
                            .contains("3888 zero-select checks passed")
                    );
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

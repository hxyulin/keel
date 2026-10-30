//! Every integer reduction sees all sixteen dwords or eight qwords.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[test]
fn avx512_integer_reductions_all_lanes_and_spills() {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut checks = String::new();
    for (shape, bits, lanes) in [("i32x16", 32, 16), ("i64x8", 64, 8)] {
        let mut body = String::new();
        for operation in ["add", "and", "or", "xor", "smin", "smax", "umin", "umax"] {
            ir += &format!(
                "func @reduce_{bits}_{operation}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n v3 = vreduce_{operation} v2\n store align 1 v3,v1\n v4 = iconst.i64 64\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\n"
            );
            c += &format!("extern void reduce_{bits}_{operation}(void*,void*);\n");
            let expression = match operation {
                "add" => format!("expected=(uint{bits}_t)(expected+x[i]);"),
                "and" => "expected&=x[i];".into(),
                "or" => "expected|=x[i];".into(),
                "xor" => "expected^=x[i];".into(),
                "smin" => format!("if((int{bits}_t)x[i]<(int{bits}_t)expected)expected=x[i];"),
                "smax" => format!("if((int{bits}_t)x[i]>(int{bits}_t)expected)expected=x[i];"),
                "umin" => "if(x[i]<expected)expected=x[i];".into(),
                _ => "if(x[i]>expected)expected=x[i];".into(),
            };
            body += &format!(
                "{{uint{bits}_t expected=x[0],got;for(unsigned i=1;i<{lanes};i++){{{expression}}}memset(out,0xa5,sizeof(out));reduce_{bits}_{operation}(x,out);memcpy(&got,out,sizeof(got));if(got!=expected||memcmp(out+64,x,64))return 10;for(unsigned i=sizeof(got);i<64;i++)if(out[i]!=0xa5)return 11;}}\n"
            );
        }
        checks += &format!(
            "{{uint{bits}_t x[{lanes}];unsigned char out[128];for(unsigned pattern=0;pattern<8;pattern++)for(unsigned lane=0;lane<{lanes};lane++){{for(unsigned i=0;i<{lanes};i++)x[i]=pattern==0?0:pattern==1?~(uint{bits}_t)0:pattern==2?1:pattern==3?((uint{bits}_t)1<<({bits}-1)):pattern==4?i+1:pattern==5?((uint{bits}_t)1<<(i%{bits})):pattern==6?(uint{bits}_t)(0x9e3779b97f4a7c15ULL*(i+1)):(uint{bits}_t)(0x8000000000000000ULL+i);x[lane]^=pattern%2?((uint{bits}_t)1<<({bits}-1)):~(uint{bits}_t)0;{body}}}}}\n"
        );
    }
    c += &format!("int main(void){{{checks}return 0;}}\n");
    let m = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2);
    let dir = std::env::temp_dir().join(format!("keel-avx512-reduce-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), c).unwrap();
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
        for opcode in [
            "vshufi32x4",
            "vpshufd",
            "vpaddd",
            "vpaddq",
            "vpminsq",
            "vpmaxuq",
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
        fs::write(dir.join("reduce.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                continue;
            }
            let output = Command::new("cc")
                .arg(dir.join("harness.c"))
                .arg(dir.join("reduce.o"))
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

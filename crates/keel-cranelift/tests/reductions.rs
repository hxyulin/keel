//! Integer reductions preserve lane-width wrapping and signed extrema.
mod common;
use common::*;
use std::process::Command;

#[test]
fn integer_reductions_match_unsigned_c_oracle() {
    if !host_ok() {
        return;
    }
    let dir = workdir("reductions");
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\nint main(void) {\n");
    for (shape, bits, lanes) in [
        ("i8x16", 8, 16),
        ("i16x8", 16, 8),
        ("i32x4", 32, 4),
        ("i64x2", 64, 2),
    ] {
        for op in ["add", "and", "or", "xor", "smin", "smax", "umin", "umax"] {
            let name = format!("r_{shape}_{op}");
            let extend = if bits == 64 {
                "return v2".to_string()
            } else {
                "v3 = uext.i64 v2\n return v3".to_string()
            };
            ir.push_str(&format!("func @{name}(ptr) -> i64 {{\nblock0(v0: ptr):\n v1 = load.{shape} v0\n v2 = vreduce_{op} v1\n {extend}\n}}\n"));
            let mask = if bits == 64 {
                "UINT64_MAX".to_string()
            } else {
                format!("((UINT64_C(1)<<{bits})-1)")
            };
            let update = match op {
                "add" => "a=(a+b)&mask",
                "and" => "a&=b",
                "or" => "a|=b",
                "xor" => "a^=b",
                "smin" => "if ((a^sign)>(b^sign)) a=b",
                "smax" => "if ((a^sign)<(b^sign)) a=b",
                "umin" => "if (a>b) a=b",
                _ => "if (a<b) a=b",
            };
            c.push_str(&format!("{{ extern uint64_t {name}(const void*);\n const uint64_t mask={mask}, sign=UINT64_C(1)<<{};\n (void)sign;\n for (unsigned trial=0;trial<40;trial++) {{\n uint8_t bytes[16];\n for(unsigned j=0;j<16;j++) bytes[j]=(uint8_t)(trial*73+j*31);\n uint64_t a=0; memcpy(&a,bytes,{});\n for(unsigned lane=1;lane<{lanes};lane++) {{ uint64_t b=0; memcpy(&b,bytes+lane*{},{}); {update}; }}\n if ({name}(bytes)!=(a&mask)) return 1;\n }} }}\n", bits-1,bits/8,bits/8,bits/8));
        }
    }
    c.push_str("return 0; }\n");
    let driver = write(&dir, "driver.c", c.as_bytes());
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for config in CONFIGS {
        let obj = write(&dir, &format!("{config}.o"), &object(&module, config));
        let exe = link(&dir, config, &driver, &["-O2"], &[&obj]);
        run(&mut Command::new(exe));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

mod common;
use common::*;
use std::process::Command;

fn fixture() -> (keel_ir::Module, String) {
    let mut ir = String::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    for from in [8, 16, 32] {
        for to in [16, 32, 64] {
            if from >= to {
                continue;
            }
            for signed in [false, true] {
                let sign = if signed { "s" } else { "u" };
                let op = if signed { "swiden_low" } else { "uwiden_low" };
                let name = format!("widen_{sign}{from}_{to}");
                ir += &format!(
                    "func @{name}(ptr, ptr) {{\nblock0(v0: ptr, v1: ptr):\n v2 = load.i{from}x{} align 1 v0\n v3 = {op}.i{to}x{} v2\n store align 1 v3, v1\n return\n}}\n",
                    128 / from,
                    128 / to
                );
                declarations += &format!("extern void {name}(const void*,void*);\n");
                let cfrom = format!("{}int{from}_t", if signed { "" } else { "u" });
                let cto = format!("{}int{to}_t", if signed { "" } else { "u" });
                checks += &format!(
                    "{{ unsigned char input[17],output[17]; {cfrom} a[{}]; {cto} expected[{}]; for(unsigned k=0;k<257;k++){{ for(unsigned j=0;j<{};j++) a[j]=({cfrom})(k*100003ULL+j*1073741823ULL); for(unsigned j=0;j<{};j++) expected[j]=({cto})a[j]; memcpy(input+1,a,16); {name}(input+1,output+1); assert(!memcmp(output+1,expected,16)); }} }}\n",
                    128 / from,
                    128 / to,
                    128 / from,
                    128 / to
                );
            }
        }
    }
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let driver = format!(
        "#include <stdint.h>\n#include <assert.h>\n#include <string.h>\n{declarations}\nint main(void){{{checks}return 0;}}\n"
    );
    (module, driver)
}

#[test]
fn all_low_lane_widths_translate_with_verified_clif() {
    let (module, _) = fixture();
    for opt in [
        keel_cranelift::OptLevel::None,
        keel_cranelift::OptLevel::Speed,
    ] {
        keel_cranelift::compile_object(
            &module,
            &keel_cranelift::Options {
                target: keel_cranelift::TargetProfile::AppleArm64,
                opt,
                verify_clif: true,
            },
        )
        .unwrap();
    }
}

#[test]
fn native_and_clif_preserve_low_lanes_signedness_and_unaligned_buffers() {
    if !host_ok() {
        return;
    }
    let (module, driver) = fixture();
    let dir = workdir("widening");
    let c = write(&dir, "driver.c", driver.as_bytes());
    for config in CONFIGS {
        let obj = write(&dir, &format!("{config}.o"), &object(&module, config));
        let exe = link(&dir, config, &c, &["-O2"], &[&obj]);
        run(&mut Command::new(exe));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

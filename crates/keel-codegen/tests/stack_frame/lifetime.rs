//! Lifetime events constrain optimization and add no native instruction sequence.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn fixed_slot_lifetime_events_emit_no_native_instructions() {
    let source = "func @f() -> i64 {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    intrinsic lifetime_start(v0)
    v1 = iconst.i64 37
    store v1, v0
    v2 = load.i64 v0
    intrinsic lifetime_end(v0)
    return v2
}";
    let marked = keel_ir::parse(source).unwrap();
    let plain = keel_ir::parse(
        &source
            .replace("    intrinsic lifetime_start(v0)\n", "")
            .replace("    intrinsic lifetime_end(v0)\n", ""),
    )
    .unwrap();
    keel_ir::verify_module(&marked).unwrap();
    for target in Target::NATIVE {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let compile = |m| {
                keel_codegen::compile_with_pipeline(m, target, &pipeline)
                    .unwrap()
                    .lines()
                    .filter(|line| {
                        !line.trim_start().starts_with(';')
                            && !line.trim_start().starts_with('#')
                            && !line.trim_start().starts_with("//")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(compile(&marked), compile(&plain), "{target:?} {selector:?}");
        }
    }
}

/// Two slots used one after the other in a loop share bytes; a third, live
/// across the whole loop, keeps its own. The C callee checks each value and
/// then overwrites it, so sharing bytes with a live slot would show.
#[test]
fn disjoint_lifetimes_share_frame_bytes() {
    use keel_codegen::backend_profile::{Counter, capture};
    let source = "decl @see(ptr, i64) -> i64
func @sequential(i64) -> i64 {
    ss0 = stack_slot 8, align 8
    ss1 = stack_slot 8, align 8
    ss2 = stack_slot 8, align 8
block0(v0: i64):
    v1 = stack_addr ss0
    v2 = stack_addr ss1
    v3 = stack_addr ss2
    intrinsic lifetime_start(v3)
    v4 = iconst.i64 77
    store v4, v3
    v5 = iconst.i64 0
    jump block1(v5, v5)
block1(v6: i64, v7: i64):
    intrinsic lifetime_start(v1)
    store v6, v1
    v8 = call @see(v1, v6)
    intrinsic lifetime_end(v1)
    intrinsic lifetime_start(v2)
    v9 = iconst.i64 1000
    v10 = iadd v6, v9
    store v10, v2
    v11 = call @see(v2, v10)
    intrinsic lifetime_end(v2)
    v12 = iadd v7, v8
    v13 = iadd v12, v11
    v14 = iconst.i64 1
    v15 = iadd v6, v14
    v16 = icmp.ult v15, v0
    brif v16, block1(v15, v13), block2(v13)
block2(v17: i64):
    v18 = call @see(v3, v4)
    intrinsic lifetime_end(v3)
    v19 = iadd v17, v18
    return v19
}";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in Target::NATIVE {
        for separate in [false, true] {
            let selector = SelectionTier::Full;
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let mut emission = keel_codegen::EmissionOptions::default();
            emission.separate_slots = separate;
            let request = keel_codegen::Request::new(target, pipeline).emission(emission);
            let (asm, report) = capture(|| request.text(&module).unwrap());
            let bytes = report.counters[Counter::SlotBytes as usize];
            assert_eq!(report.counters[Counter::UnsharedSlotBytes as usize], 24);
            assert_eq!(bytes, if separate { 24 } else { 16 }, "{target:?}");
            keel_asm::assemble_source_for(target, &asm).unwrap();
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            if target == Target::Aarch64AppleDarwin {
                run_against_c(&asm, separate);
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn run_against_c(asm: &str, separate: bool) {
    let driver = r#"
#include <stdio.h>
long sequential(long);
/* 1 unless *p holds want; then clobbers the object, as its owner may. */
long see(long *p, long want) {
    long bad = *p != want;
    *p = -1;
    return bad;
}
int main(void) {
    long failures = sequential(10);
    if (failures) printf("%ld failures\n", failures);
    return failures != 0;
}
"#;
    let dir = std::env::temp_dir().join(format!(
        "keel-slot-sharing-{}-{separate}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("driver.c"), driver).unwrap();
    std::fs::write(dir.join("frame.s"), asm).unwrap();
    let status = std::process::Command::new("clang")
        .arg(dir.join("driver.c"))
        .arg(dir.join("frame.s"))
        .arg("-o")
        .arg(dir.join("frame"))
        .status()
        .unwrap();
    assert!(status.success());
    let run = std::process::Command::new(dir.join("frame"))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

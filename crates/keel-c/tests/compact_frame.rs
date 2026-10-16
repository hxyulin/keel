//! Frontend declarations left behind by optimization consume no normal frame.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};
use keel_ir::InstData;

#[test]
fn optimized_c_dead_declarations_do_not_change_generated_code() {
    for target in Target::ALL {
        let compile_c = |count| {
            let source =
                format!("int f(int x) {{ int dead[{count}]; dead[0] = x; return x + 1; }}");
            let mut module = keel_c::compile(
                &source,
                "dead.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap();
            keel_opt::optimize(&mut module).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let f = &module.functions[0];
            assert!(
                f.num_stack_slots() >= 1,
                "frontend must retain declarations"
            );
            assert!(
                !f.layout_insts()
                    .any(|(_, inst)| matches!(f.inst_data(inst), InstData::StackAddr { .. })),
                "optimized fixture must have no executing slot references"
            );
            module
        };
        let small = compile_c(1);
        let large = compile_c(8192);
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                assert_eq!(
                    keel_codegen::compile_with_pipeline(&small, target, &pipeline).unwrap(),
                    keel_codegen::compile_with_pipeline(&large, target, &pipeline).unwrap(),
                );
                if Target::host() == Some(target) {
                    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join(format!("compact-c-{allocator:?}-{selector:?}"));
                    std::fs::create_dir_all(&dir).unwrap();
                    let asm =
                        keel_codegen::compile_with_pipeline(&large, target, &pipeline).unwrap();
                    std::fs::write(dir.join("f.s"), asm).unwrap();
                    std::fs::write(
                        dir.join("main.c"),
                        "extern int f(int); int main(void) { return f(41) != 42; }",
                    )
                    .unwrap();
                    let exe = dir.join("test");
                    let output = std::process::Command::new("clang")
                        .arg(dir.join("f.s"))
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(std::process::Command::new(exe).status().unwrap().success());
                }
            }
        }
    }
}

//! Size emission shares restoration only after moving results into ABI registers.
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target,
    regalloc::AllocationTier,
};
use object::{Object, ObjectSection};

const SOURCE: &str = r#"
decl @side(i64) -> i64
func @choose(i64, i64) -> i64 {
    ss0 = stack_slot 64, align 64
block0(v0:i64, v1:i64):
    v2 = stack_addr ss0
    store v0, v2
    v3 = iconst.i64 32
    v4 = intrinsic stack_alloc(v3)
    store v0, v4
    v5 = call @side(v0)
    brif v1, block1, block2
block1:
    v6 = load.i64 v2
    v7 = iadd v5, v6
    return v7
block2:
    v8 = load.i64 v4
    v9 = isub v8, v5
    return v9
}
func @wide(i128, i64) -> i128 {
block0(v0:i128, v1:i64):
    v2 = call @side(v1)
    brif v2, block1, block2
block1:
    return v0
block2:
    v3 = iconst.i128 0
    v4 = isub v3, v0
    return v4
}
func @floating(f64, f64, i64) -> f64 {
block0(v0:f64, v1:f64, v2:i64):
    v3 = call @side(v2)
    brif v3, block1, block2
block1:
    v4 = fadd v0, v1
    return v4
block2:
    v5 = fsub v0, v1
    return v5
}
func @multiple(i64, i64) -> (i64, i64) {
block0(v0:i64, v1:i64):
    v2 = call @side(v0)
    brif v2, block1, block2
block1:
    return v0, v1
block2:
    return v1, v0
}
"#;

#[test]
fn shared_restoration_preserves_transports_and_return_values() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-shared-returns-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
uint64_t choose(uint64_t, uint64_t);
__int128 wide(__int128, uint64_t);
double floating(double, double, uint64_t);
struct Pair { uint64_t first, second; };
struct Pair multiple(uint64_t, uint64_t);
uint64_t side(uint64_t x) { return x; }
int main(void) {
    const uint64_t values[] = {0, 1, 17, UINT64_MAX, UINT64_C(1) << 63};
    for (unsigned i = 0; i < sizeof(values)/sizeof(values[0]); ++i) {
        uint64_t x = values[i];
        if (choose(x, 0) != 0 || choose(x, 1) != x + x) return 1;
        __int128 big = ((__int128)x << 32) + 7;
        if (wide(big, 0) != -big || wide(big, 1) != big) return 2;
    }
    if (floating(7.5, 2.25, 0) != 5.25 || floating(7.5, 2.25, 1) != 9.75) return 3;
    struct Pair a = multiple(0, 7), b = multiple(5, 7);
    if (a.first != 7 || a.second != 0 || b.first != 5 || b.second != 7) return 4;
    return 0;
}
"#,
    )
    .unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocation,
            };
            let mut sizes = Vec::new();
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let mut options = EmissionOptions::default();
                options.goal = goal;
                let request = Request::new(target, pipeline).emission(options);
                let text = request.text(&module).unwrap();
                assert_eq!(
                    text.matches("\tret\n").count(),
                    if goal == CodegenGoal::Size { 4 } else { 8 },
                    "{allocation:?}: {text}"
                );
                let bytes = keel_asm::assemble_source_for(target, &text).unwrap();
                let program = request.program(&module).unwrap();
                assert_eq!(
                    bytes,
                    keel_asm::assemble_program_for(target, &program).unwrap()
                );
                let mut chunks = keel_asm_ir::Program::new();
                request
                    .program_chunks(&module, &mut |p| chunks.append(p))
                    .unwrap();
                assert_eq!(
                    bytes,
                    keel_asm::assemble_program_for(target, &chunks).unwrap()
                );
                let object = object::File::parse(bytes.as_slice()).unwrap();
                sizes.push(
                    object
                        .section_by_name(if target == Target::Aarch64AppleDarwin {
                            "__text"
                        } else {
                            ".text"
                        })
                        .unwrap()
                        .size(),
                );
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                if target == Target::Aarch64AppleDarwin && goal == CodegenGoal::Size {
                    // The two transports are byte-identical; execute one image.
                    std::fs::write(dir.join("returns.o"), &bytes).unwrap();
                    let status = std::process::Command::new("clang")
                        .arg(dir.join("oracle.c"))
                        .arg(dir.join("returns.o"))
                        .arg("-o")
                        .arg(dir.join("returns"))
                        .status()
                        .unwrap();
                    assert!(status.success());
                    let run = std::process::Command::new(dir.join("returns"))
                        .output()
                        .unwrap();
                    assert!(run.status.success(), "{allocation:?}: {run:?}");
                }
            }
            assert!(sizes[1] < sizes[0], "{target:?}/{allocation:?}: {sizes:?}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn return_sharing_respects_function_policy_debug_and_tiny_frames() {
    for (attrs, goal, debug, shared) in [
        ("", CodegenGoal::Speed, false, false),
        ("", CodegenGoal::Size, false, true),
        ("cold", CodegenGoal::Speed, false, true),
        ("optimize(\"Os\")", CodegenGoal::Speed, false, true),
        ("optimize(2)", CodegenGoal::Size, false, false),
        ("optnone", CodegenGoal::Size, false, false),
        ("", CodegenGoal::Size, true, false),
    ] {
        let source = SOURCE.replace("-> i64 {", &format!("-> i64 {attrs} {{"));
        let module = keel_ir::parse(&format!(
            "{}{source}",
            if debug {
                "file f0 = \"returns.c\"\n"
            } else {
                ""
            }
        ))
        .unwrap();
        let mut options = EmissionOptions::default();
        options.goal = goal;
        let text = Request::new(
            Target::Aarch64LinuxGnu,
            CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
        )
        .emission(options)
        .text(&module)
        .unwrap();
        assert_eq!(
            text.contains("Lchoose_return_epilogue:"),
            shared,
            "{attrs}, {debug}: {text}"
        );
    }
    // A frame record alone needs two instructions; a used slot adds the
    // third instruction that makes replacing another epilogue worthwhile.
    for (slot, store, shared) in [
        ("", "", false),
        (
            "ss0 = stack_slot 8, align 8",
            "v2 = stack_addr ss0\n store v0, v2",
            true,
        ),
    ] {
        let module = keel_ir::parse(&format!("func @tiny(i64) -> i64 {{\n{slot}\nblock0(v0:i64):\n{store}\n brif v0, block1, block2\nblock1:\n return v0\nblock2:\n v1 = iconst.i64 3\n return v1\n}}")).unwrap();
        let mut options = EmissionOptions::default();
        options.goal = CodegenGoal::Size;
        let text = Request::new(
            Target::Aarch64LinuxGnu,
            CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
        )
        .emission(options)
        .text(&module)
        .unwrap();
        assert_eq!(text.contains("return_epilogue"), shared, "{text}");
        assert_eq!(
            text.matches("\tret\n").count(),
            if shared { 1 } else { 2 },
            "{text}"
        );
    }
}

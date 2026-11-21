//! Size emission preserves the SysV ABI and agrees across assembly transports.
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, SelectionTier, Target, regalloc::AllocationTier,
};
use object::{Object, ObjectSection, ObjectSymbol};

const SOURCE: &str = r#"
decl @side(i64) -> i64
func @leaf(i64) -> i64 {
block0(v0:i64):
 v1 = iconst.i64 3
 v2 = iadd v0, v1
 return v2
}
func @call_only(i64) -> i64 {
block0(v0:i64):
 v1 = call @side(v0)
 return v1
}
func @choose(i64, i64) -> i64 {
block0(v0:i64, v1:i64):
 v2 = call @side(v0)
 brif v2, block1, block2
block1:
 v3 = iadd v0, v2
 return v3
block2:
 v4 = isub v1, v2
 return v4
}
func @word_loop(ptr, ptr) -> i64 {
block0(v0:ptr, v1:ptr):
 jump block1
block1:
 v2 = load.i64 v0
 v3 = load.i64 v1
 v4 = xor v2, v3
 brif v4, block1, block2(v4)
block2(v5:i64):
 return v5
}
"#;

fn options(goal: CodegenGoal) -> EmissionOptions {
    let mut options = EmissionOptions::default();
    options.goal = goal;
    options
}

fn function<'a>(text: &'a str, name: &str) -> &'a str {
    text.split_once(&format!("\"{name}\":\n"))
        .unwrap()
        .1
        .split_once(&format!(".L{name}_end:"))
        .unwrap()
        .0
}

#[test]
fn size_fuses_register_compare_across_lifetimes_and_keeps_memory_compare_in_place() {
    let source = r#"
func @lifetime_cmp(i64, i64) -> i64 cc(c) {
 ss0 = stack_slot 8, align 8
block0(v0: i64, v1: i64):
 v2 = stack_addr ss0
 intrinsic lifetime_start(v2)
 v3 = icmp.ult v0, v1
 intrinsic lifetime_end(v2)
 brif v3, block1(v1, v0), block1(v0, v1)
block1(v4: i64, v5: i64):
 v6 = isub v5, v4
 return v6
}
"#;
    let memory = source
        .replace("@lifetime_cmp(", "@lifetime_mem_cmp(")
        .replace(
            "v3 = icmp.ult v0, v1",
            "store v1, v2\n v7 = load.i64 v2\n v3 = icmp.ult v0, v7",
        );
    let narrow = source
        .replace("@lifetime_cmp(", "@lifetime_narrow_cmp(")
        .replace("i64", "i32");
    let framed = narrow
        .replace("@lifetime_narrow_cmp(", "@lifetime_framed_cmp(")
        .replace("cc(c) {", "cc(c) frame_pointer {");
    let module = keel_ir::parse(&format!("{source}\n{memory}\n{narrow}\n{framed}")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let target = Target::X86_64LinuxGnu;
    let request = keel_codegen::Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &AllocationTier::Full,
        },
    );
    let speed = request.text(&module).unwrap();
    assert!(
        function(&speed, "lifetime_cmp").contains("\tset"),
        "{speed}"
    );

    let request = request.emission(options(CodegenGoal::Size));
    let text = request.text(&module).unwrap();
    let register = function(&text, "lifetime_cmp");
    assert!(register.contains("\tcmp\t"), "{register}");
    assert!(!register.contains("\tset"), "{register}");
    let memory = function(&text, "lifetime_mem_cmp");
    assert!(
        memory
            .lines()
            .any(|line| line.contains("\tcmp\t") && line.contains("qword ptr")),
        "{memory}"
    );
    assert!(memory.contains("\tset"), "{memory}");
    assert!(
        function(&text, "lifetime_narrow_cmp").contains("\tset"),
        "{text}"
    );
    assert!(
        !function(&text, "lifetime_framed_cmp").contains("\tset"),
        "{text}"
    );
    let object =
        keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_source_for(target, &text).unwrap()
    );
    keel_test_support::native_program!("x86-compare-lifetime")
        .file("cmp.o", object)
        .file(
            "oracle.c",
            r#"
#include <stdint.h>
uint64_t lifetime_cmp(uint64_t, uint64_t);
uint64_t lifetime_mem_cmp(uint64_t, uint64_t);
uint32_t lifetime_framed_cmp(uint32_t, uint32_t);
int main(void) {
    const uint64_t values[] = {0, 1, 19, UINT64_C(1) << 63, UINT64_MAX};
    for (unsigned i = 0; i < sizeof(values) / sizeof(*values); ++i)
        for (unsigned j = 0; j < sizeof(values) / sizeof(*values); ++j) {
            uint64_t a = values[i], b = values[j];
            uint64_t want = a < b ? a - b : b - a;
            if (lifetime_cmp(a, b) != want || lifetime_mem_cmp(a, b) != want) return 1;
            uint32_t x = a, y = b;
            if (lifetime_framed_cmp(x, y) != (x < y ? x - y : y - x)) return 2;
        }
    return 0;
}
"#,
        )
        .link(["oracle.c", "cmp.o"])
        .check();
}

#[test]
fn size_uses_short_tests_for_small_switches_with_shared_edges() {
    let module = keel_ir::parse(
        r#"
func @grouped_switch(i32) -> i64 cc(c) {
block0(v0: i32):
 v1 = iconst.i64 41
 v2 = iconst.i64 57
 v3 = iconst.i64 89
 jump block1(v0, v1)
block1(v4: i32, v5: i64):
 switch v4, block3(v5), [3: block2, 2: block3(v2), 1: block3(v2), 0: block3(v5)]
block2:
 v6 = iconst.i32 4
 jump block1(v6, v3)
block3(v7: i64):
 return v7
}
func @distinct_edges(i32) -> i64 cc(c) {
block0(v0: i32):
 v1 = iconst.i64 41
 v2 = iconst.i64 57
 v3 = iconst.i64 89
 v4 = iconst.i64 97
 jump block1(v0, v1)
block1(v5: i32, v6: i64):
 switch v5, block3(v6), [3: block2, 2: block3(v4), 1: block3(v2), 0: block3(v6)]
block2:
 v7 = iconst.i32 4
 jump block1(v7, v3)
block3(v8: i64):
 return v8
}
func @acyclic_switch(i32) -> i64 cc(c) {
block0(v0: i32):
 v1 = iconst.i64 41
 v2 = iconst.i64 57
 v3 = iconst.i64 89
 switch v0, block1(v1), [3: block1(v3), 2: block1(v2), 1: block1(v2), 0: block1(v1)]
block1(v4: i64):
 return v4
}
"#,
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let target = Target::X86_64LinuxGnu;
    let request = keel_codegen::Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &AllocationTier::Compact,
        },
    );
    let speed = request.text(&module).unwrap();
    assert!(function(&speed, "grouped_switch").contains("\tjmp\trax"));
    let request = request.emission(options(CodegenGoal::Size));
    let text = request.text(&module).unwrap();
    let grouped = function(&text, "grouped_switch");
    assert!(!grouped.contains("\tjmp\trax"), "{grouped}");
    assert!(grouped.contains("\tjbe\t"), "{grouped}");
    assert!(
        function(&text, "distinct_edges").contains("\tjmp\trax"),
        "{text}"
    );
    assert!(
        function(&text, "acyclic_switch").contains("\tjmp\trax"),
        "{text}"
    );
    let object =
        keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_source_for(target, &text).unwrap()
    );
    keel_test_support::native_program!("x86-grouped-switch")
        .file("switch.o", object)
        .file(
            "oracle.c",
            r#"
#include <stdint.h>
uint64_t grouped_switch(uint32_t);
uint64_t distinct_edges(uint32_t);
int main(void) {
    const uint32_t inputs[] = {0, 1, 2, 3, 4, 127, UINT32_MAX};
    const uint64_t want[] = {41, 57, 57, 89, 41, 41, 41};
    for (unsigned i = 0; i < sizeof(inputs) / sizeof(*inputs); ++i) {
        if (grouped_switch(inputs[i]) != want[i]) return 1;
        if (distinct_edges(inputs[i]) != (i == 2 ? 97 : want[i])) return 2;
    }
    return 0;
}
"#,
        )
        .link(["oracle.c", "switch.o"])
        .check();
}

#[test]
fn smaller_layout_frames_and_shared_returns_preserve_transport_parity() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let mut objects = Vec::new();
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(options(goal))
                    .text(&module)
                    .unwrap();
                let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(options(goal))
                    .program(&module)
                    .unwrap();
                let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
                let mut chunks = keel_asm_ir::Program::new();
                keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(options(goal))
                    .program_chunks(&module, &mut |p| chunks.append(p))
                    .unwrap();
                assert_eq!(
                    bytes,
                    keel_asm::x86::assemble_program_object(&chunks).unwrap()
                );
                if goal == CodegenGoal::Size {
                    assert!(!text.contains(".p2align\t4"), "{text}");
                    assert!(!text.contains(".p2align\t5"), "{text}");
                    assert!(!function(&text, "leaf").contains("rbp"), "{text}");
                    assert!(function(&text, "call_only").contains("pop\trbp"), "{text}");
                    let shared = function(&text, "choose");
                    assert_eq!(shared.matches("\tret\n").count(), 1, "{text}");
                    assert!(shared.contains(".Lchoose_return"), "{text}");
                } else {
                    assert!(function(&text, "leaf").contains("push\trbp"), "{text}");
                    assert!(text.contains(".p2align\t5"), "{text}");
                    assert_eq!(
                        text,
                        keel_codegen::compile_with_pipeline(
                            &module,
                            Target::X86_64LinuxGnu,
                            &pipeline,
                        )
                        .unwrap()
                    );
                }
                objects.push(bytes);
            }
            let code_size = |bytes: &[u8]| {
                object::File::parse(bytes)
                    .unwrap()
                    .section_by_name(".text")
                    .unwrap()
                    .size()
            };
            assert!(code_size(&objects[1]) < code_size(&objects[0]));
        }
    }
}

#[test]
fn explicit_alignment_and_stack_observers_keep_their_contracts() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    module.functions[0].attrs.align = Some(64);
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
        .emission(options(CodegenGoal::Size))
        .program(&module)
        .unwrap();
    let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.section_by_name(".text").unwrap().align(), 64);
    assert_eq!(
        file.symbols()
            .find(|s| s.name().ok() == Some("leaf"))
            .unwrap()
            .address()
            % 64,
        0
    );
    for source in [
        "func @observe() -> ptr {\nblock0:\n v0 = intrinsic frame_address()\n return v0\n}\n",
        "func @observe(i64, i64, i64, i64, i64, i64, i64) -> i64 {\nblock0(v0:i64, v1:i64, v2:i64, v3:i64, v4:i64, v5:i64, v6:i64):\n return v6\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options(CodegenGoal::Size))
            .text(&module)
            .unwrap();
        assert!(text.contains("push\trbp"), "{text}");
    }
}

#[test]
fn sharing_saves_bytes_even_when_returns_need_near_jumps() {
    // Put calls before both returns, so the earlier return crosses more than
    // 127 bytes regardless of the allocator's block order.
    let mut source = SOURCE.to_owned();
    for (block, start) in [("block1", 100), ("block2", 300)] {
        let calls: String = (start..start + 64)
            .map(|i| format!(" v{i} = call @side(v0)\n"))
            .collect();
        source = source.replacen(&format!("{block}:\n"), &format!("{block}:\n{calls}"), 1);
    }
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let mut sizes = Vec::new();
    for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
        let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options(goal))
            .text(&module)
            .unwrap();
        let bytes = keel_asm::x86::assemble_object(&text).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let symbol = file
            .symbols()
            .find(|s| s.name().ok() == Some("choose"))
            .unwrap();
        sizes.push(symbol.size());
        if goal == CodegenGoal::Size {
            let section = file
                .section_by_index(symbol.section_index().unwrap())
                .unwrap();
            let body =
                &section.data().unwrap()[symbol.address() as usize..][..symbol.size() as usize];
            assert!(
                body.contains(&0xe9),
                "expected a near jump to the shared return"
            );
            assert_eq!(function(&text, "choose").matches("\tret\n").count(), 1);
        }
    }
    assert!(
        sizes[1] < sizes[0],
        "near jump sharing grew the function: {sizes:?}"
    );
}

struct RbxOnly;
impl keel_codegen::regalloc::RegisterAllocator for RbxOnly {
    fn name(&self) -> &'static str {
        "rbx-only-size-test"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let pools = keel_codegen::regalloc::Pools {
            caller_saved: &[],
            callee_saved: &[3],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        AllocationTier::Full.allocate(&keel_codegen::regalloc::AllocationInput {
            func: input.func,
            pools: &pools,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        })
    }
}

#[test]
fn register_only_leaf_drops_unused_call_alignment_padding() {
    let module = keel_ir::parse("func @padding_leaf(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 3\n v2 = iadd v0, v1\n return v2\n}\n").unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &RbxOnly,
    };
    for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
        let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options(goal))
            .text(&module)
            .unwrap();
        assert!(text.contains("push\trbx"), "{text}");
        assert_eq!(
            text.contains("sub\trsp, 8"),
            goal == CodegenGoal::Speed,
            "{text}"
        );
        let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options(goal))
            .program(&module)
            .unwrap();
        let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
        assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
    }
}

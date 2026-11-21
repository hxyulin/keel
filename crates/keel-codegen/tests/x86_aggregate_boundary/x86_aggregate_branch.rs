use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

#[test]
fn single_integer_field_keeps_the_c_abi_without_stack_homes() {
    use keel_codegen::{CodegenGoal, EmissionOptions, Request, Target};
    use keel_test_support::native_program;

    let module = keel_ir::parse(
        r#"
data @invalid = align 4 { i32 0 }
data @identity = align 4 { i32 0 }
func @map_word(ptr sret {i32}, ptr, ptr, ptr byval {i32}) cc(c) {
    ss0 = stack_slot 4, align 4
    dscope ds0
block0(v0: ptr, v1: ptr, v2: ptr, v3: ptr):
    v4 = symbol_addr @identity
    v5 = icmp.eq v1, v4
    brif v5, block1, block2
block1:
    v6 = load.i32 v3
    store v6, v0
    return
block2:
    v7 = symbol_addr @invalid
    v8 = icmp.eq v1, v7
    brif v8, block3, block4
block3:
    v9 = iconst.i32 -1
    store v9, v0
    return
block4:
    v10 = icmp.eq v2, v4
    brif v10, block5, block6
block5:
    v11 = load.i32 v3
    jump block7(v11)
block6:
    v12 = load.i32 v3
    jump block7(v12)
block7(v13: i32):
    store v13, v0
    return
}
"#,
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let target = Target::X86_64LinuxGnu;
    let mut options = EmissionOptions::default();
    options.goal = CodegenGoal::Size;
    let request = Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &AllocationTier::Compact,
        },
    )
    .emission(options);
    let text = request.text(&module).unwrap();
    assert!(
        !text.contains("ptr [rbp") && !text.contains("ptr [rsp"),
        "{text}"
    );
    let object =
        keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_source_for(target, &text).unwrap()
    );
    native_program!("single-integer-field")
        .file("map.o", object)
        .file(
            "oracle.c",
            "\
typedef struct { unsigned bits; } Word;
extern int invalid;
extern int identity;
extern Word map_word(const int *, const int *, Word);
int main(void) {
    int other = 0;
    unsigned payloads[] = {0, 1, 0x80000000u, 0xffffffffu};
    for (unsigned i = 0; i < 4; ++i) {
        Word w = {payloads[i]};
        if (map_word(&invalid, &other, w).bits != 0xffffffffu ||
            map_word(&identity, &other, w).bits != w.bits ||
            map_word(&other, &identity, w).bits != w.bits ||
            map_word(&other, &other, w).bits != w.bits) return 1;
    }
    return 0;
}",
        )
        .link(["oracle.c", "map.o"])
        .check();
}

#[test]
fn branching_aggregate_keeps_native_parity_and_c_abi() {
    let source = include_str!("../fixtures/aggregate-branch.keel");
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &empty] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
                #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                execute(&bytes);
            }
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn execute(bytes: &[u8]) {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-aggregate-branch-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("branch.o"), bytes).unwrap();
    fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <sys/prctl.h>
#include <unistd.h>
typedef struct { uint64_t lo, hi; } P;
extern P aggregate_dup(void *, P);
int main(void) {
    struct rlimit no_core = {0,0};
    if (setrlimit(RLIMIT_CORE, &no_core)) abort();
    // Ubuntu's apport ignores RLIMIT_CORE; non-dumpable processes skip it.
    if (prctl(PR_SET_DUMPABLE, 0)) abort();
    uint64_t tags[] = {0,1,UINT64_MAX-11,UINT64_MAX-10,UINT64_MAX,0x12345678fffffffbULL};
    for (unsigned i=0; i<sizeof(tags)/sizeof(tags[0]); ++i) {
        uint32_t count = 41;
        P v = {(uintptr_t)&count, tags[i]};
        P r = aggregate_dup((void *)(uintptr_t)1, v);
        if (r.lo!=v.lo || r.hi!=v.hi) abort();
        if (count != 41 + ((uint32_t)v.hi >= (uint32_t)-11)) abort();
    }
    /* Invalid pointees on the untaken branch must remain unobserved. */
    P r = aggregate_dup(0, (P){1,0});
    if (r.lo!=1 || r.hi!=0) abort();
    pid_t child = fork();
    if (child==0) { aggregate_dup(0, (P){1,UINT64_MAX}); _exit(2); }
    int status;
    if (child<0 || waitpid(child,&status,0)!=child || !WIFSIGNALED(status) ||
        (WTERMSIG(status)!=SIGSEGV && WTERMSIG(status)!=SIGBUS)) abort();
    return 0;
}
"#,
    )
    .unwrap();
    let obj = dir.join("branch.o");
    let built = Command::new("clang")
        .arg(dir.join("oracle.c"))
        .arg(&obj)
        .arg("-o")
        .arg(dir.join("oracle"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(Command::new(dir.join("oracle")).status().unwrap().success());
    fs::remove_dir_all(dir).unwrap();
}

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

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

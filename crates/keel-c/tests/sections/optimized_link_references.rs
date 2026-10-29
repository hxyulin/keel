use keel_c::{FunctionBinding, Options};
use keel_ir::Target;

#[test]
fn optimization_removes_references_from_newly_dead_bodies_and_empty_calls() {
    for source in [
        "extern void missing(void); static int dead(void){missing();return 1;} int live(void){if(0)return dead();return 0;}",
        "extern void missing(void); static int dead(int flag){if(flag)missing();return 1;} int live(void){return dead(0);}",
        "extern int missing; __attribute__((noinline)) void empty(void *p){} void live(void){empty(&missing);}",
        "extern void missing(void); int live(void){if ((unsigned long)((void *)0x100 + 0xdead000000000000UL) & 1) missing(); return 0;}",
        "extern void missing(void); int live(unsigned x){if (((x ? 4u : 0u) | 0x133u) & 0x18000u) missing(); return 0;}",
        "extern void missing(void); int live(unsigned x){unsigned long flags=x & 3; if (flags > 3) missing(); return 0;}",
    ] {
        let mut module = keel_c::compile_with_binding(
            source,
            "link-references.c",
            Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
            FunctionBinding::NoSemanticInterposition,
        )
        .unwrap();
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let assembly = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
        assert!(!assembly.contains("missing"), "{source}\n{assembly}");
    }
}

#[test]
fn empty_call_removal_keeps_side_effectful_argument_evaluation() {
    let source = "extern void *effect(void); __attribute__((noinline)) static void empty(void *p){} void live(void){empty(effect());}";
    let mut module = keel_c::compile(
        source,
        "call-effects.c",
        Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let assembly = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
    assert!(assembly.contains("effect"), "{assembly}");
    assert!(!assembly.contains("empty"), "{assembly}");
}

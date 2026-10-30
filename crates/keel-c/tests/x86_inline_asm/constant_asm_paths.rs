use keel_ir::Target;
use std::{fs, process::Command};

const SOURCE: &str = r#"
static int calls;
static long *once(long *p) { ++calls; return p; }
long load(long *p) {
    long result = 0;
    switch (sizeof(*p)) {
    case 8: asm volatile("movq %1, %0" : "=r"(result) : "m"(*once(p))); break;
    case 4: asm volatile("movl %1, %0" : "=r"(result) : "m"(*once(p))); break;
    case 2: asm volatile("movw %1, %0" : "=r"(result) : "m"(*once(p))); break;
    }
    if (sizeof(*p) != 8)
        asm volatile("unreachable_width_instruction" ::: "memory");
    return result;
}
int test(void) {
    long words[3] = {31, 0x123456789abcdefL, 47};
    calls = 0;
    long result = load(&words[1]);
    return result != words[1] || calls != 1 || words[0] != 31 || words[2] != 47;
}
"#;

#[test]
fn constant_width_asm_paths_finalize_without_optional_optimization() {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("constant-asm-paths");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.c"), SOURCE).unwrap();
    fs::write(
        directory.join("main.c"),
        "int test(void); int main(void) { return test(); }",
    )
    .unwrap();
    for debug_info in [false, true] {
        let module = keel_c::compile(
            SOURCE,
            "constant-asm-paths.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        for optimize in [false, true] {
            let mut module = module.clone();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let assembly = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
            assert!(!assembly.contains("unreachable_width_instruction"));
            let object = keel_asm::x86::assemble_object(&assembly).unwrap();
            let stem = format!("generated-{debug_info}-{optimize}");
            fs::write(directory.join(format!("{stem}.o")), object).unwrap();
            if Target::host() == Some(Target::X86_64LinuxGnu) {
                let executable = directory.join(&stem);
                let output = Command::new("clang")
                    .arg(directory.join(format!("{stem}.o")))
                    .arg(directory.join("main.c"))
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(Command::new(&executable).status().unwrap().code(), Some(0));
            }
        }
    }
}

#[test]
fn incoming_goto_and_runtime_conditions_keep_asm_paths_reachable() {
    for source in [
        "long f(long *p, int enter) { long x; if (0) { target: asm volatile(\"movl %1,%0\" : \"=r\"(x) : \"m\"(*p)); return x; } if (enter) goto target; return 0; }",
        "long f(long *p, int enter) { long x = 0; if (enter) asm volatile(\"movl %1,%0\" : \"=r\"(x) : \"m\"(*p)); return x; }",
    ] {
        let module = keel_c::compile(
            source,
            "reachable-asm.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        for error in [
            keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap_err(),
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap_err(),
        ] {
            assert_eq!(error.func, "f");
            assert!(error.to_string().contains("32-bit register"), "{error}");
        }
    }
}

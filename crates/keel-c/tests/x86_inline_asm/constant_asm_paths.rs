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
                ..Default::default()
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
fn constant_query_selects_the_immediate_path_after_required_inlining() {
    let source = r#"
static inline __attribute__((always_inline)) unsigned choose(unsigned value) {
    if (__builtin_constant_p(value)) {
        unsigned result;
        asm volatile("movl %1, %0" : "=r"(result) : "i"(value));
        return result;
    }
    return value + 1;
}
unsigned fixed(void) { return choose(41); }
unsigned dynamic(unsigned value) { return choose(value); }
unsigned discarded(unsigned value) {
    return __builtin_constant_p(value++) ? 0 : value;
}
"#;
    let target = Target::X86_64LinuxGnu;
    let module = keel_c::compile_with_language(
        source,
        "inline-constant-query.c",
        keel_c::Options {
            target,
            ..Default::default()
        },
        keel_c::FunctionBinding::default(),
        keel_c::LibraryBuiltins::default(),
        keel_c::LanguageOptions {
            deferred_constant_queries: true,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    for function in &module.functions {
        let has_asm = function.layout_insts().any(|(_, inst)| {
            matches!(
                function.inst_data(inst),
                keel_ir::InstData::InlineAsm { .. }
            )
        });
        assert_eq!(has_asm, function.name == "fixed", "{}", function.name);
        assert!(
            function
                .callees()
                .all(|(_, callee)| !callee.name.starts_with('\u{1}'))
        );
    }
    let assembly = keel_codegen::compile(&module, target).unwrap();
    let object = keel_asm::x86::assemble_object(&assembly).unwrap();
    if Target::host() == Some(target) {
        let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("deferred-query");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("generated.o"), object).unwrap();
        fs::write(
            directory.join("main.c"),
            "unsigned fixed(void), dynamic(unsigned), discarded(unsigned);\n\
             int main(void) { return fixed() != 41 || dynamic(41) != 42 || discarded(7) != 7; }",
        )
        .unwrap();
        let executable = directory.join("test");
        let output = Command::new("clang")
            .arg(directory.join("generated.o"))
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
        assert!(Command::new(executable).status().unwrap().success());
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
                ..Default::default()
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

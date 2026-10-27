use keel_ir::Target;
use std::{fs, process::Command};

const SOURCE: &str = r#"
char marked[8] __attribute__((nonstring));
extern char redeclared[8] __attribute__((__nonstring__));
char redeclared[8];
typedef char Buffer[8];
struct Record {
    char plain[8];
    char marked[8] __attribute__((nonstring));
    struct { unsigned char nested[3] __attribute__((nonstring)); };
};
union Views { char plain[8]; char marked[8] __attribute__((nonstring)); };
_Static_assert(__builtin_has_attribute(marked, nonstring), "global");
_Static_assert(__builtin_has_attribute((marked), nonstring), "parentheses");
_Static_assert(__builtin_has_attribute(*(&marked), nonstring), "identity address");
_Static_assert(__builtin_has_attribute(*(&marked + 0), nonstring), "zero offset");
_Static_assert(__builtin_has_attribute(((char (*)[8])&marked)[0], nonstring), "same pointer type");
_Static_assert(!__builtin_has_attribute(&marked, nonstring), "address value");
_Static_assert(!__builtin_has_attribute(marked[0], nonstring), "element");
_Static_assert(__builtin_has_attribute(redeclared, __nonstring__), "merged");
_Static_assert(!__builtin_has_attribute(marked + 0, nonstring), "arithmetic");
_Static_assert(!__builtin_has_attribute((char *)marked, nonstring), "cast");
_Static_assert(!__builtin_has_attribute(Buffer, nonstring), "type");
_Static_assert(!__builtin_has_attribute(4, nonstring), "constant");
_Static_assert(sizeof(__builtin_has_attribute(marked, nonstring)) == sizeof(_Bool), "bool");
int queries(char *argument __attribute__((nonstring))) {
    struct Record record;
    union Views views;
    char local[4] __attribute__((nonstring));
    static signed char saved[4] __attribute__((nonstring));
    _Static_assert(__builtin_has_attribute(argument, nonstring), "parameter");
    _Static_assert(__builtin_has_attribute(local, nonstring), "local");
    _Static_assert(__builtin_has_attribute(saved, nonstring), "static local");
    _Static_assert(__builtin_has_attribute(record.marked, nonstring), "member");
    _Static_assert(__builtin_has_attribute(record.nested, nonstring), "anonymous member");
    _Static_assert(!__builtin_has_attribute(record.plain, nonstring), "plain member");
    _Static_assert(__builtin_has_attribute(views.marked, nonstring), "union member");
    _Static_assert(!__builtin_has_attribute(views.plain, nonstring), "other union member");
    int touched = 0;
    int query = __builtin_has_attribute(touched++, nonstring);
    return touched + query;
}
"#;

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "has-attribute.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

#[test]
fn nonstring_queries_track_declarations_without_evaluating_operands() {
    for target in Target::NATIVE {
        let module = compile(SOURCE, target).unwrap_or_else(|e| panic!("{target:?}: {e}"));
        keel_ir::verify_module(&module).unwrap();
        for optimize in [false, true] {
            let mut module = module.clone();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let program = keel_codegen::compile_program(&module, target).unwrap();
            let object = match target {
                Target::Aarch64AppleDarwin => {
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                }
                Target::Aarch64LinuxGnu => {
                    keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap()
                }
                Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(&program).unwrap(),
                _ => unreachable!(),
            };
            if Target::host() == Some(target) {
                let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("nonstring-opt{optimize}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("queries.o"), object).unwrap();
                fs::write(dir.join("main.c"), "extern int queries(char *); int main(void) { char buffer[8]; return queries(buffer); }\n").unwrap();
                let output = Command::new("clang")
                    .arg(dir.join("queries.o"))
                    .arg(dir.join("main.c"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(Command::new(dir.join("run")).status().unwrap().success());
            }
        }
    }
}

#[test]
fn unsupported_queries_and_invalid_annotations_fail_closed() {
    for (source, expected) in [
        (
            "int x; int f(void) { return __builtin_has_attribute(x, aligned); }",
            "only nonstring",
        ),
        (
            "char x[4]; int f(void) { return __builtin_has_attribute(x, nonstring(1)); }",
            "expected",
        ),
        (
            "int x __attribute__((nonstring));",
            "character array or pointer",
        ),
        (
            "char x __attribute__((nonstring));",
            "character array or pointer",
        ),
        ("typedef char B[4] __attribute__((nonstring));", "typedef"),
        (
            "void f(void) __attribute__((nonstring));",
            "character array or pointer",
        ),
        (
            "char x[4] __attribute__((nonstring(1)));",
            "without arguments",
        ),
        (
            "char __attribute__((nonstring));",
            "character array or pointer",
        ),
        (
            "int f(void) { return __builtin_has_attribute(char __attribute__((nonstring)) *, nonstring); }",
            "type name",
        ),
    ] {
        let error = compile(source, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{source}: {error}");
    }
}

#[test]
fn feature_queries_advertise_the_checked_subset() {
    let target = Target::X86_64LinuxGnu;
    let pp = keel_c::preprocess("#if !__has_builtin(__builtin_has_attribute) || !__has_attribute(nonstring)\n#error missing nonstring query\n#endif\nint ok;\n", "feature.c", &keel_c::PreprocessOptions { target, ..Default::default() }).unwrap();
    compile(&pp, target).unwrap();
}

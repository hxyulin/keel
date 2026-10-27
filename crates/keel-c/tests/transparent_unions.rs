//! Pointer-only transparent unions retain the existing one-word aggregate ABI.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
typedef union { unsigned long *word; int *integer; void *any; } Arg __attribute__((transparent_union));
unsigned long external_read(Arg);
unsigned long read_arg(Arg arg) { return arg.any ? *(unsigned long *)arg.any : 13; }
unsigned long call_word(unsigned long *p) { return external_read(p); }
unsigned long call_integer(int *p) { return external_read(p); }
unsigned long call_null(void) { return external_read(0); }
unsigned long call_void(void *p) { return external_read(p); }
unsigned long call_indirect(unsigned long (*f)(Arg), unsigned long *p) { return f(p); }
unsigned long *advance(unsigned long **p) { return (*p)++; }
unsigned long call_once(unsigned long **p) { return external_read(advance(p)); }
"#;
const RUNNER: &str = r#"
typedef union { unsigned long *word; int *integer; void *any; } Arg __attribute__((transparent_union));
extern unsigned long read_arg(Arg),call_word(unsigned long *),call_integer(int *),call_null(void),call_void(void *),call_indirect(unsigned long (*)(Arg),unsigned long *),call_once(unsigned long **);
unsigned long external_read(Arg arg) { return arg.any ? *(unsigned long *)arg.any : 13; }
int main(void) { unsigned long values[]={0xfedcba9876543210UL,37},*p=values;
 if(read_arg(values)!=values[0] || read_arg((int *)values)!=values[0] || read_arg(0)!=13) return 1;
 if(call_word(values)!=values[0] || call_integer((int *)values)!=values[0] || call_void(values)!=values[0] || call_null()!=13) return 2;
 if(call_indirect(external_read,values)!=values[0]) return 3;
 if(call_once(&p)!=values[0] || p!=values+1) return 4;
 return 0;
}
"#;
#[test]
fn pointer_union_calls_cross_native_compiler_boundaries() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("transparent-unions");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.c");
    let runner = dir.join("runner.c");
    fs::write(&source, SOURCE).unwrap();
    fs::write(&runner, RUNNER).unwrap();
    for target in Target::NATIVE {
        let module = keel_c::compile(
            SOURCE,
            "transparent.c",
            keel_c::Options {
                target,
                debug_info: true,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let module = keel_ir::parse(&module.to_string()).unwrap();
        let assembly = keel_codegen::compile(&module, target).unwrap();
        if Some(target) == Target::host() {
            let generated = dir.join("generated.s");
            fs::write(&generated, assembly).unwrap();
            for input in [&source, &generated] {
                let exe = input.with_extension("exe");
                let output = Command::new("clang")
                    .args(["-O2", "-w"])
                    .arg(input)
                    .arg(&runner)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(Command::new(exe).status().unwrap().success());
            }
        }
    }
    if let Some(path) = std::env::var_os("KEEL_TRANSPARENT_UNION_EXPORT") {
        let path = std::path::PathBuf::from(path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("source.c"), SOURCE).unwrap();
        fs::write(path.join("main.c"), RUNNER).unwrap();
        let module = keel_c::compile(
            SOURCE,
            "transparent.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap();
        fs::write(
            path.join("generated.o"),
            keel_asm::x86::assemble_object(
                &keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn transparent_union_restrictions_remain_diagnostics() {
    for source in [
        "typedef union {int x;float y;} U __attribute__((transparent_union));",
        "typedef struct {void *p;} U __attribute__((transparent_union));",
        "typedef union {int *p;long *q;} U __attribute__((transparent_union));void f(U);void g(void){f(7);}",
        "typedef union {int *p;long *q;} U __attribute__((transparent_union));void g(void){U u;int x;u=&x;}",
    ] {
        assert!(
            keel_c::compile(
                source,
                "invalid.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "{source}"
        );
    }
}

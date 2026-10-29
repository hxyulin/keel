//! GNU extern-inline implementations do not define the public addressable symbol.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
int boot_count;
static __attribute__((constructor,always_inline)) void boot(void){++boot_count;}

extern inline __attribute__((gnu_inline,always_inline)) int body(int x){return x+5;}
int through_body(int x){return body(x);}
int (*through_address(void))(int){return body;}
int (*stored_address)(int)=body;
extern inline __attribute__((gnu_inline)) int external(int x){return x+9;}
int through_external(int x){return external(x);}
inline __attribute__((gnu_inline)) int emitted(int x){return x+17;}
"#;
const RUNNER: &str = r#"
int body(int x){return x+71;}
int external(int x){return x+99;}
extern int through_body(int),through_external(int),emitted(int);
extern int (*through_address(void))(int),(*stored_address)(int);
extern int boot_count;
int main(void){return boot_count!=1 || through_body(3)!=8 || through_external(3)!=102 || through_address()(3)!=74 || stored_address(3)!=74 || emitted(3)!=20;}
"#;
#[test]
fn inline_calls_and_external_addresses_have_separate_owners() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gnu-extern-inline");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.c");
    let runner = dir.join("runner.c");
    fs::write(&source, SOURCE).unwrap();
    fs::write(&runner, RUNNER).unwrap();
    for target in Target::NATIVE {
        let module = keel_c::compile(
            SOURCE,
            "gnu-inline.c",
            keel_c::Options {
                target,
                debug_info: true,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(
            !module
                .functions
                .iter()
                .any(|f| f.name == "body" || f.name == "external" || f.name == "unused_immediate")
        );
        assert!(
            module
                .functions
                .iter()
                .any(|f| f.name == "emitted" && !f.local)
        );
        let text = module.to_string();
        let module = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let assembly = keel_codegen::compile(&module, target).unwrap();
        if Some(target) == Target::host() {
            let path = dir.join("generated.s");
            fs::write(&path, assembly).unwrap();
            for input in [&source, &path] {
                let exe = input.with_extension("exe");
                let run = Command::new("clang")
                    .args(["-O0", "-w"])
                    .arg(input)
                    .arg(&runner)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    run.status.success(),
                    "{}",
                    String::from_utf8_lossy(&run.stderr)
                );
                assert!(Command::new(exe).status().unwrap().success());
            }
        }
    }
    if let Some(path) = std::env::var_os("KEEL_GNU_INLINE_EXPORT") {
        let path = std::path::PathBuf::from(path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("source.c"), SOURCE).unwrap();
        fs::write(path.join("main.c"), RUNNER).unwrap();
        let module = keel_c::compile(
            SOURCE,
            "gnu-inline.c",
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
fn aliases_cannot_target_an_unemitted_inline_implementation() {
    let unused = "extern inline __attribute__((gnu_inline)) int unused_immediate(int x){asm(\"\"::\"i\"(x));return x;}";
    let module = keel_c::compile(
        unused,
        "unused-gnu-inline.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    assert!(module.functions.is_empty());
    let source = "extern inline __attribute__((gnu_inline)) int body(void){return 1;} extern int alias(void) __attribute__((alias(\"body\")));";
    assert!(
        keel_c::compile(
            source,
            "invalid-inline.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false
            }
        )
        .unwrap_err()
        .message
        .contains("no emitted")
    );
}

#[test]
fn inline_only_sections_do_not_escape_into_external_declarations() {
    let source = "extern inline __attribute__((gnu_inline,always_inline,section(\".text.inline_body\"))) int body(int x){return x+1;} int caller(int x){return body(x);}";
    let module = keel_c::compile(
        source,
        "inline-section.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(
        module
            .decls
            .iter()
            .filter(|d| d.name == "body")
            .all(|d| d.attrs.section.is_none())
    );
    keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
}

#[cfg(feature = "profile")]
#[test]
fn profiled_compilation_uses_the_same_inline_ownership_rules() {
    let options = keel_c::Options {
        target: Target::X86_64LinuxGnu,
        debug_info: false,
    };
    let normal = keel_c::compile(SOURCE, "inline-profile.c", options).unwrap();
    let (profiled, _) = keel_c::profile::compile(SOURCE, "inline-profile.c", options).unwrap();
    assert_eq!(normal.to_string(), profiled.to_string());
}

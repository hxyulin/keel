use keel_ir::{InstData, Target};
use std::{fs, process::Command};

const SOURCE: &str = r#"
inline __attribute__((gnu_inline, always_inline)) int public_body(int x) {
    asm volatile("" : "+r"(x));
    return x + 5;
}
int direct(int x) { return public_body(x); }
int (*address(void))(int) { return public_body; }
int (*stored)(int) = public_body;
"#;
const RUNNER: &str = r#"
int public_body(int), direct(int);
int (*address(void))(int);
extern int (*stored)(int);
int main(void) { return direct(3)!=8 || public_body(3)!=8 || address()(3)!=8 || stored(3)!=8; }
"#;

#[test]
fn public_required_body_keeps_its_symbol_and_binding() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("public-required-inline");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(dir.join("main.c"), RUNNER).unwrap();
    for target in Target::NATIVE {
        let source = if matches!(target, Target::Aarch64AppleDarwin | Target::X86_64LinuxGnu) {
            SOURCE.to_owned()
        } else {
            SOURCE.replace("asm volatile(\"\" : \"+r\"(x));", "")
        };
        for debug_info in [false, true] {
            let module = keel_c::compile(
                &source,
                "public-required.c",
                keel_c::Options { target, debug_info },
            )
            .unwrap();
            keel_ir::verify_module(&module).unwrap();
            let body = module
                .functions
                .iter()
                .find(|f| f.name == "public_body")
                .unwrap();
            assert!(
                !body.local
                    && !body.attrs.weak
                    && !body.attrs.noninterposable
                    && !body.attrs.no_semantic_interposition
            );
            let direct = module
                .functions
                .iter()
                .find(|f| f.name == "direct")
                .unwrap();
            assert!(!direct.layout_insts().any(|(_, i)| matches!(direct.inst_data(i), InstData::Call { func, .. } if direct.callee(*func).name == "public_body")));
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let assembly = keel_codegen::compile(&module, target).unwrap();
                let stem = format!("generated-{target:?}-{debug_info}-{optimize}");
                let source = dir.join(format!("{stem}.s"));
                fs::write(&source, assembly).unwrap();
                if Some(target) == Target::host() {
                    let executable = dir.join(&stem);
                    let output = Command::new("clang")
                        .arg(&source)
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(&executable)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(Command::new(executable).status().unwrap().code(), Some(0));
                }
            }
        }
    }
}

#[test]
fn weak_required_definitions_keep_the_binding_gate() {
    let error = keel_c::compile(
        "inline __attribute__((gnu_inline,always_inline,weak)) int f(int x) { asm volatile(\"\" : \"+r\"(x)); return x; } int g(int x) { return f(x); }",
        "weak-inline.c", keel_c::Options { target: Target::X86_64LinuxGnu, debug_info: false },
    ).unwrap_err();
    assert!(error.to_string().contains("public_definition"), "{error}");
}

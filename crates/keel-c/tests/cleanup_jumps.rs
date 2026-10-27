//! Goto edges unwind declaration lifetimes without consuming sibling paths.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};

const SOURCE: &str = include_str!("fixtures/cleanup_jumps.c");

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: status={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cleanup_jumps_match_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cleanup-jumps");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    for optimize in [false, true] {
        run(Command::new("clang")
            .arg(if optimize { "-O2" } else { "-O0" })
            .arg(dir.join("source.c"))
            .arg(dir.join("main.c"))
            .arg("-o")
            .arg(dir.join("reference")));
        run(&mut Command::new(dir.join("reference")));
    }
    for target in Target::NATIVE {
        for debug_info in [false, true] {
            for optimize in [false, true] {
                let mut module = keel_c::compile(
                    SOURCE,
                    "cleanup-jumps.c",
                    keel_c::Options { target, debug_info },
                )
                .unwrap();
                keel_ir::verify_module(&module).unwrap();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                keel_ir::verify_module(&module).unwrap();
                let program = keel_codegen::compile_program(&module, target).unwrap();
                let object = match target {
                    Target::Aarch64AppleDarwin => {
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                    }
                    Target::Aarch64LinuxGnu => {
                        keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap()
                    }
                    Target::X86_64LinuxGnu => {
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    }
                    Target::Wasm32UnknownUnknown => unreachable!(),
                };
                assert!(!object.is_empty());
                if Some(target) == Target::host() {
                    fs::write(dir.join("generated.o"), object).unwrap();
                    run(Command::new("clang")
                        .arg(dir.join("generated.o"))
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(dir.join("run")));
                    run(&mut Command::new(dir.join("run")));
                }
            }
        }
    }
}

#[test]
fn unsupported_cleanup_entries_and_vm_jumps_are_diagnosed() {
    let cases = [
        (
            "goto end; int x CLEAN=1;end:return 0;",
            "bypasses initialization",
        ),
        (
            "{goto inner;} {int x CLEAN=1;inner:return 0;}",
            "bypasses initialization",
        ),
        (
            "int x CLEAN=1;goto inner;return ({inner:;1;});",
            "enters a statement expression",
        ),
        (
            "void *p=&&end;{int x CLEAN=1;goto *p;}end:return 0;",
            "computed goto across",
        ),
        (
            "int x CLEAN=1;void *p=&&end;goto *p;return ({end:;1;});",
            "enters a statement expression",
        ),
        (
            "int x CLEAN=1;int n=2;int v[n];goto end;end:return 0;",
            "VLAs",
        ),
        (
            "int x CLEAN=1;int n=2;int v[n];goto *&&end;end:return 0;",
            "VLAs",
        ),
    ];
    for target in Target::NATIVE {
        for (body, expected) in cases {
            let source = format!("void c(int*);int f(void){{{body}}}")
                .replace("CLEAN", "__attribute__((cleanup(c)))");
            let error = keel_c::compile(
                &source,
                "reject-cleanup-jump.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(
                error.message.contains(expected),
                "{body}: expected {expected}, got {error}"
            );
        }
    }
}

#[test]
fn asm_goto_cleanup_edges_compile() {
    let source = r#"
static int log_value;
static void record(int *p){log_value=log_value*10+*p;}
int f(int n){
 int a __attribute__((cleanup(record)))=1;
 {int b __attribute__((cleanup(record)))=2;
 asm goto("movl $7,%0; testl %1,%1; jnz %l[done]" : "+r"(b) : "r"(n) : "cc" : done);
 log_value=3;
done:;
 }
 return log_value;
}
int test(void){
 log_value=0;if(f(1)!=7 || log_value!=71)return 1;
 log_value=0;if(f(0)!=37 || log_value!=371)return 2;
 return 0;
}
"#;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("asm-cleanup-jumps");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    run(Command::new("clang")
        .args(["-target", "x86_64-linux-gnu", "-S", "-emit-llvm"])
        .arg(dir.join("source.c"))
        .arg("-o")
        .arg(dir.join("reference.ll")));
    for debug_info in [false, true] {
        for optimize in [false, true] {
            let mut module = keel_c::compile(
                source,
                "asm-cleanup-jump.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info,
                },
            )
            .unwrap();
            keel_ir::verify_module(&module).unwrap();
            let function = module.functions.iter().find(|f| f.name == "f").unwrap();
            let (asm_block, asm_inst) = function
                .layout_insts()
                .find(|(_, i)| {
                    matches!(function.inst_data(*i), keel_ir::InstData::InlineAsm { .. })
                })
                .unwrap();
            let output = function.inst_results(asm_inst)[1];
            let instructions = function.block_insts(asm_block);
            assert!(instructions.iter().any(|&i| matches!(function.inst_data(i), keel_ir::InstData::Store {value, ..} if *value == output)), "asm output must be stored before successor cleanup");
            assert!(
                !instructions
                    .iter()
                    .any(|&i| matches!(function.inst_data(i), keel_ir::InstData::Call { .. })),
                "cleanup must run only after edge selection"
            );
            let keel_ir::InstData::Switch { dests, .. } =
                function.inst_data(*instructions.last().unwrap())
            else {
                panic!("asm goto needs successor dispatch")
            };
            let calls = |block| {
                function
                    .block_insts(block)
                    .iter()
                    .filter(|&&i| matches!(function.inst_data(i), keel_ir::InstData::Call { .. }))
                    .count()
            };
            assert_eq!(
                calls(dests[0].block),
                0,
                "fallthrough must retain the cleanup until the label join"
            );
            assert_eq!(
                calls(dests[1].block),
                2,
                "the label join cleans the inner and outer locals exactly once"
            );
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
            let object = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert!(!object.is_empty());
            if Target::host() == Some(Target::X86_64LinuxGnu) {
                fs::write(dir.join("generated.o"), object).unwrap();
                run(Command::new("clang")
                    .arg(dir.join("generated.o"))
                    .arg(dir.join("main.c"))
                    .arg("-o")
                    .arg(dir.join("run")));
                run(&mut Command::new(dir.join("run")));
            }
        }
    }
    for (body, expected) in [
        (
            "{int b __attribute__((cleanup(record)))=2;asm goto(\"\"::::end);}end:return 0;",
            "asm goto across",
        ),
        (
            "asm goto(\"\"::::end);int b __attribute__((cleanup(record)))=2;end:return 0;",
            "bypasses initialization",
        ),
        (
            "int n=2;int v[n];asm goto(\"\"::::end);end:return 0;",
            "VLAs",
        ),
    ] {
        let source = format!(
            "void record(int*);int f(void){{int a __attribute__((cleanup(record)))=1;{body}}}"
        );
        let error = keel_c::compile(
            &source,
            "reject-asm-cleanup.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(error.message.contains(expected), "{error}");
    }
}

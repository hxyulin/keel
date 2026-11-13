//! Relocation, switch and division boundaries for direct x86 emission.
use super::{parse, request};
use keel_codegen::{Request, backend_profile};
use object::{Object, ObjectSection, ObjectSymbol};
use std::fmt::Write;

pub(super) fn execute(name: &str, source: &str, oracle: &str) {
    execute_count(name, source, oracle, None);
}
pub(super) fn execute_count(name: &str, source: &str, oracle: &str, expected: Option<usize>) {
    let module = parse(source);
    let req = request();
    let (program, report) = backend_profile::capture(|| req.program(&module));
    let program = program.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        expected.unwrap_or(module.functions.len()) as u64
    );
    let object = keel_asm::assemble_program_for(req.target, &program).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_source_for(req.target, &req.text(&module).unwrap()).unwrap()
    );
    keel_test_support::native_program!(name)
        .file("input.keel", source)
        .file("input.o", &object)
        .file("oracle.c", oracle)
        .link(["-O2", "oracle.c", "input.o"])
        .check();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        use std::{fs, process::Command};
        if !Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .unwrap()
            .success()
        {
            keel_test_support::skip!("Rosetta unavailable for direct integer oracle");
        }
        // Preserve encoded instruction bytes, symbolic instruction lengths and CFI.
        // Translate only ELF metadata and symbol spelling to Mach-O conventions.
        let file = object::File::parse(object.as_slice()).unwrap();
        let mut names = file
            .symbols()
            .filter_map(|symbol| symbol.name().ok())
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .collect::<Vec<_>>();
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        let mut assembly = String::new();
        for line in req.text(&module).unwrap().lines() {
            let trimmed = line.trim_start();
            if [".type", ".size", ".hidden", ".local"]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                continue;
            }
            let mut line = if trimmed.starts_with(".section") {
                if trimmed.contains(".note.GNU-stack") {
                    continue;
                }
                if trimmed.contains(".text") {
                    ".text".to_owned()
                } else if trimmed.contains(".rodata") {
                    ".const".to_owned()
                } else {
                    ".data".to_owned()
                }
            } else {
                line.replace("@PLT", "")
            };
            for name in &names {
                line = line.replace(&format!("\"{name}\""), &format!("\"_{name}\""));
                line = line.replace(&format!(".globl {name}"), &format!(".globl _{name}"));
                if line.trim() == format!("{name}:") {
                    line = format!("_{name}:");
                }
            }
            writeln!(assembly, "{line}").unwrap();
        }
        let dir = keel_test_support::TempDir::new("x86-direct-integer-rosetta");
        fs::write(dir.join("input.s"), assembly).unwrap();
        fs::write(dir.join("oracle.c"), oracle).unwrap();
        let Some(clang) = keel_test_support::clang() else {
            keel_test_support::skip!(keel_test_support::NO_CLANG);
        };
        let output = clang
            .command()
            .current_dir(dir.path())
            .args([
                "-arch", "x86_64", "-O2", "oracle.c", "input.s", "-o", "oracle",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            Command::new("arch")
                .arg("-x86_64")
                .arg(dir.join("oracle"))
                .status()
                .unwrap()
                .success(),
            "{name}"
        );
    }
}

#[test]
fn switches_reuse_existing_sparse_narrow_and_edge_oracles() {
    execute(
        "switches",
        include_str!("../programs/switch.keel"),
        include_str!("../programs/switch.c"),
    );
}

#[test]
fn explicit_traps_and_nontrapping_nonleaf_paths_match_c() {
    execute(
        "explicit-traps",
        include_str!("../compile_speed/trap.keel"),
        include_str!("../compile_speed/trap.c"),
    );
}

const SYMBOLS: &str = "decl data @outside\ndecl @callback(i64)->i64\ndata @defined = align 8 { i64 37 }\nfunc @function_address()->ptr {\nblock0:\n v0=symbol_addr @callback\n return v0\n}\nfunc @selected(i64)->i64 {\nblock0(v0:i64):\n v1=iconst.i64 3\n switch v0,block3,[-1:block1,0x123456789:block2]\nblock1:\n v2=symbol_addr @defined\n v3=load.i64 v2\n v4=sdiv v3,v1\n return v4\nblock2:\n v5=symbol_addr @outside\n v6=load.i64 v5\n v7=srem v6,v1\n return v7\nblock3:\n v8=symbol_addr @callback\n v9=call_indirect v8(v0):(i64)->i64\n return v9\n}\n";
#[test]
fn symbol_addresses_mix_with_switches_division_and_indirect_calls() {
    let oracle = "#include <stdint.h>\nuint64_t outside=11;extern uint64_t defined,selected(uint64_t);extern void *function_address(void);\nuint64_t callback(uint64_t x){return x+9;}\nint main(void){if(defined!=37||selected(UINT64_MAX)!=12||selected(UINT64_C(0x123456789))!=2||selected(19)!=28||function_address()!=(void*)callback)return 1;return 0;}\n";
    execute("symbols-switch-division", SYMBOLS, oracle);
}

#[test]
fn symbol_address_relocations_preserve_models_binding_and_tls_fallback() {
    use keel_codegen::x86_64::CodeModel;
    let source = "decl data @external\ndata @local = align 8 { i64 1 }\ndata @weak = weak align 8 { i64 2 }\nfunc @f(i64)->ptr {\nblock0(v0:i64):\n v1=symbol_addr @local\n v2=symbol_addr @external\n v3=symbol_addr @weak\n switch v0,block3,[0:block1,1:block2]\nblock1:\n return v1\nblock2:\n return v2\nblock3:\n return v3\n}\n";
    let module = parse(source);
    for model in [
        CodeModel::PositionIndependent,
        CodeModel::Small,
        CodeModel::Kernel,
    ] {
        let mut req = request();
        req.emission.x86_code_model = Some(model);
        let program = req.program(&module).unwrap();
        let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_source_for(req.target, &req.text(&module).unwrap()).unwrap()
        );
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let section = file.section_by_name(".text").unwrap();
        let data = section.data().unwrap();
        let mut seen = Vec::new();
        for (offset, relocation) in section.relocations() {
            let object::RelocationTarget::Symbol(index) = relocation.target() else {
                continue;
            };
            let name = file.symbol_by_index(index).unwrap().name().unwrap();
            let expected = match model {
                CodeModel::PositionIndependent => {
                    if name == "local" {
                        object::RelocationFlags::Elf {
                            r_type: object::elf::R_X86_64_PC32,
                        }
                    } else {
                        object::RelocationFlags::Elf {
                            r_type: object::elf::R_X86_64_GOTPCREL,
                        }
                    }
                }
                CodeModel::Small => object::RelocationFlags::Elf {
                    r_type: object::elf::R_X86_64_32,
                },
                CodeModel::Kernel => object::RelocationFlags::Elf {
                    r_type: object::elf::R_X86_64_32S,
                },
                _ => unreachable!(),
            };
            assert_eq!(relocation.flags(), expected, "{name}/{model:?}");
            // The instruction after every symbolic address is the canonical
            // eight-byte stack store. This checks the reserved site length.
            let after = offset as usize + 4;
            assert_eq!(&data[after..after + 4], &[0x48, 0x89, 0x84, 0x24]);
            seen.push(name.to_owned());
        }
        seen.sort();
        assert_eq!(seen, ["external", "local", "weak"]);
    }
    let module = parse(
        "data @tls = thread_local align 8 { i64 1 }\nfunc @f()->ptr {\nblock0:\n v0=symbol_addr @tls\n return v0\n}\n",
    );
    let req = request();
    let (program, report) = backend_profile::capture(|| req.program(&module));
    let program = program.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
    assert_eq!(
        program,
        Request::new(req.target, req.pipeline)
            .program(&module)
            .unwrap()
    );
}

#[test]
fn division_matches_integer_oracles_and_preserves_traps_at_all_widths() {
    let mut source = String::new();
    let mut oracle = String::from(
        "#include <stdint.h>\n#include <signal.h>\n#include <sys/wait.h>\n#include <unistd.h>\n",
    );
    let mut main = String::from(
        "int main(void){\nuint64_t values[]={0,1,2,3,7,127,128,255,32767,32768,65535,UINT64_C(0x80000000),UINT64_C(0x8000000000000000),UINT64_MAX};\n",
    );
    oracle.push_str("typedef uint64_t(*operation)(uint64_t,uint64_t);\nstatic void trapped(int signal){_exit(signal==SIGILL?0:2);}\nstatic int must_trap(operation op,uint64_t a,uint64_t b){pid_t p=fork();if(p<0)return 1;if(!p){signal(SIGILL,trapped);op(a,b);_exit(1);}int status;if(waitpid(p,&status,0)!=p)return 1;return !WIFEXITED(status)||WEXITSTATUS(status)!=0;}\n");
    for bits in [8, 16, 32, 64] {
        let mask = if bits == 64 {
            "UINT64_MAX".to_owned()
        } else {
            format!("((UINT64_C(1)<<{bits})-1)")
        };
        let min = format!("(UINT64_C(1)<<{})", bits - 1);
        for op in ["udiv", "urem", "sdiv", "srem"] {
            let extend = if bits < 64 { " v3=uext.i64 v2\n" } else { "" };
            let result = if bits < 64 { "v3" } else { "v2" };
            writeln!(source,"func @{op}{bits}(i{bits},i{bits})->i64 {{\nblock0(v0:i{bits},v1:i{bits}):\n v2={op} v0,v1\n{extend} return {result}\n}}").unwrap();
            writeln!(oracle,"extern uint64_t {op}{bits}(uint{bits}_t,uint{bits}_t);\nstatic uint64_t adapt_{op}{bits}(uint64_t a,uint64_t b){{return {op}{bits}((uint{bits}_t)a,(uint{bits}_t)b);}}").unwrap();
            let expression = if op.starts_with('u') {
                format!("a{}b", if op == "udiv" { '/' } else { '%' })
            } else {
                format!(
                    "(uint64_t)((int64_t)(int{bits}_t)a{}(int64_t)(int{bits}_t)b)",
                    if op == "sdiv" { '/' } else { '%' }
                )
            };
            writeln!(main,"for(unsigned i=0;i<14;i++)for(unsigned j=0;j<14;j++){{uint64_t a=(uint{bits}_t)values[i],b=(uint{bits}_t)values[j];if(!b)continue;int overflow=a=={min}&&b=={mask};").unwrap();
            if op == "sdiv" {
                main.push_str("if(overflow)continue;\n");
            }
            let expression = if op == "srem" {
                format!("overflow?0:({expression})")
            } else {
                expression
            };
            writeln!(main,"uint64_t expected=({expression})&{mask};if({op}{bits}(a,b)!=expected)return 1;}}\nif(must_trap(adapt_{op}{bits},7,0))return 2;").unwrap();
            if op == "sdiv" {
                writeln!(
                    main,
                    "if(must_trap(adapt_{op}{bits},{min},{mask}))return 3;"
                )
                .unwrap();
            }
            if op == "srem" {
                writeln!(main, "if({op}{bits}({min},{mask})!=0)return 4;").unwrap();
            }
        }
    }
    main.push_str("return 0;}\n");
    oracle.push_str(&main);
    execute("division-traps", &source, &oracle);
}

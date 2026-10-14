//! The keel driver:
//!
//! ```text
//! keel compile [-O] [-g] [--target <triple>] <input.keel> [-o <output.s>]   IR to assembly
//! keel cc [-O] [-g] [--emit-ir] [--target <triple>] <input.i> [-o <output>]  preprocessed C to assembly
//! keel opt [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
//! keel asm <input.s> -o <output.o>                 A64 assembly to Mach-O
//! keel passes                                      list the passes
//! ```
//!
//! `-O` runs the default pass pipeline before code generation; `opt` runs
//! the given passes (the default pipeline without `-p`) and prints the IR.
//! Assembly has debug info if the input names source files (`file`
//! lines); `-g` gives an input without them its own lines instead, for
//! stepping through the IR in a debugger.
//! `cc` compiles one preprocessed C file (`clang -E`), to assembly or,
//! with `--emit-ir`, to keel IR; `-g` gives it debug info, with every
//! local in memory where a debugger can see it (link with `clang -g`).
//! Text output goes to stdout without `-o`; `asm` requires `-o` and always
//! writes an Apple arm64 Mach-O object. `--target` picks the machine for codegen,
//! `aarch64-apple-darwin` or `x86_64-unknown-linux-gnu`; the default is
//! the one keel runs on.

use keel_codegen::Target;
use keel_ir::Module;
use std::process::ExitCode;

const USAGE: &str = "usage:
  keel compile [-O] [-g] [--target <triple>] <input.keel> [-o <output.s>]
  keel cc [-O] [-g] [--emit-ir] [--target <triple>] <input.i> [-o <output>]
  keel opt [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
  keel asm <input.s> -o <output.o>
  keel passes";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("keel: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Command-line flags shared by the subcommands.
#[derive(Default)]
struct Options {
    input: Option<String>,
    output: Option<String>,
    optimize: bool,
    ir_lines: bool,
    emit_ir: bool,
    passes: Option<Vec<String>>,
    target: Option<Target>,
}

impl Options {
    fn target(&self) -> Result<Target, String> {
        self.target
            .or_else(Target::host)
            .ok_or_else(|| "keel does not run on this machine's target; pass `--target`".into())
    }
}

fn parse_options(args: &[String], allowed: &[&str]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let flag = arg.as_str();
        if flag.starts_with('-') && !allowed.contains(&flag) {
            return Err(format!("unknown flag `{flag}`\n{USAGE}"));
        }
        match flag {
            "-o" => opts.output = Some(args.next().ok_or("`-o` needs a file")?.clone()),
            "-O" => opts.optimize = true,
            "-g" => opts.ir_lines = true,
            "--emit-ir" => opts.emit_ir = true,
            "--target" => {
                let triple = args.next().ok_or("`--target` needs a target triple")?;
                let target = Target::from_triple(triple).ok_or_else(|| {
                    let known: Vec<_> = Target::ALL.iter().map(|t| t.triple()).collect();
                    format!("unknown target `{triple}`; known: {}", known.join(", "))
                })?;
                opts.target = Some(target);
            }
            "-p" => {
                let list = args.next().ok_or("`-p` needs a list of passes")?;
                opts.passes = Some(list.split(',').map(|s| s.trim().to_string()).collect());
            }
            _ if opts.input.is_none() => opts.input = Some(arg.clone()),
            _ => return Err(format!("unexpected argument `{arg}`\n{USAGE}")),
        }
    }
    Ok(opts)
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    match cmd.as_str() {
        "asm" => {
            let opts = parse_options(rest, &["-o"])?;
            let input = opts.input.as_ref().ok_or(USAGE)?;
            let output = opts
                .output
                .as_ref()
                .ok_or("`asm` requires `-o <output.o>`")?;
            let src = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
            let object = keel_asm::assemble_object(&src).map_err(|e| format!("{input}: {e}"))?;
            std::fs::write(output, object).map_err(|e| format!("{output}: {e}"))
        }
        "compile" => {
            let opts = parse_options(rest, &["-o", "-O", "-g", "--target"])?;
            let (input, mut module) = load(&opts)?;
            if opts.optimize {
                keel_opt::optimize(&mut module).map_err(|e| format!("{input}: {e}"))?;
            }
            let asm = keel_codegen::compile(&module, opts.target()?)
                .map_err(|e| format!("{input}: {e}"))?;
            write_output(&opts, &asm)
        }
        "cc" => {
            let opts = parse_options(rest, &["-o", "-O", "-g", "--emit-ir", "--target"])?;
            let input = opts.input.clone().ok_or(USAGE)?;
            let src = std::fs::read_to_string(&input).map_err(|e| format!("{input}: {e}"))?;
            let target = opts.target()?;
            let c_opts = keel_c::Options {
                debug_info: opts.ir_lines,
                target,
            };
            let mut module = keel_c::compile(&src, &input, c_opts).map_err(|e| e.to_string())?;
            if let Err(errors) = keel_ir::verify_module(&module) {
                let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
                return Err(format!(
                    "{input}: internal error, invalid IR:\n{}",
                    lines.join("\n")
                ));
            }
            if opts.optimize {
                keel_opt::optimize(&mut module).map_err(|e| format!("{input}: {e}"))?;
            }
            if opts.emit_ir {
                return write_output(&opts, &module.to_string());
            }
            let asm = keel_codegen::compile(&module, opts.target()?)
                .map_err(|e| format!("{input}: {e}"))?;
            write_output(&opts, &asm)
        }
        "opt" => {
            let opts = parse_options(rest, &["-o", "-p"])?;
            let (input, mut module) = load(&opts)?;
            let result = match &opts.passes {
                Some(names) => {
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    keel_opt::run_passes(&mut module, &names)
                }
                None => keel_opt::optimize(&mut module),
            };
            result.map_err(|e| format!("{input}: {e}"))?;
            write_output(&opts, &module.to_string())
        }
        "passes" => {
            for pass in keel_opt::PASSES {
                println!("{:14} {}", pass.name, pass.description);
            }
            println!(
                "\ndefault pipeline (-O): {}",
                keel_opt::DEFAULT_PIPELINE.join(", ")
            );
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Reads, parses and verifies the input.
fn load(opts: &Options) -> Result<(String, Module), String> {
    let input = opts.input.clone().ok_or(USAGE)?;
    let src = std::fs::read_to_string(&input).map_err(|e| format!("{input}: {e}"))?;
    let module = if opts.ir_lines {
        keel_ir::parse_with_ir_lines(&src, &input)
    } else {
        keel_ir::parse(&src)
    };
    let module = module.map_err(|e| format!("{input}:{e}"))?;
    if let Err(errors) = keel_ir::verify_module(&module) {
        let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        return Err(format!("{input}: verifier errors:\n{}", lines.join("\n")));
    }
    Ok((input, module))
}

fn write_output(opts: &Options, text: &str) -> Result<(), String> {
    match &opts.output {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{path}: {e}")),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

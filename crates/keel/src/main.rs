//! The keel driver:
//!
//! ```text
//! keel compile [-O] [-g] <input.keel> [-o <output.s>]   IR to aarch64 assembly
//! keel opt [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
//! keel passes                                      list the passes
//! ```
//!
//! `-O` runs the default pass pipeline before code generation; `opt` runs
//! the given passes (the default pipeline without `-p`) and prints the IR.
//! Assembly has debug info if the input names source files (`file`
//! lines); `-g` gives an input without them its own lines instead, for
//! stepping through the IR in a debugger.
//! Output goes to stdout without `-o`.

use keel_ir::Module;
use std::process::ExitCode;

const USAGE: &str = "usage:
  keel compile [-O] [-g] <input.keel> [-o <output.s>]
  keel opt [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
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
    passes: Option<Vec<String>>,
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
        "compile" => {
            let opts = parse_options(rest, &["-o", "-O", "-g"])?;
            let (input, mut module) = load(&opts)?;
            if opts.optimize {
                keel_opt::optimize(&mut module).map_err(|e| format!("{input}: {e}"))?;
            }
            let asm =
                keel_codegen::aarch64::compile(&module).map_err(|e| format!("{input}: {e}"))?;
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

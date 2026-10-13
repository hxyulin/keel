//! `keel compile <input.keel> [-o <output.s>]`: parse, verify and compile IR
//! to aarch64 assembly. Without `-o` the assembly goes to stdout.

use std::process::ExitCode;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("keel: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let usage = "usage: keel compile <input.keel> [-o <output.s>]";
    let (input, output) = match args.as_slice() {
        [cmd, input] if cmd == "compile" => (input, None),
        [cmd, input, flag, output] if cmd == "compile" && flag == "-o" => (input, Some(output)),
        _ => return Err(usage.into()),
    };

    let src = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
    let module = keel_ir::parse(&src).map_err(|e| format!("{input}:{e}"))?;
    if let Err(errors) = keel_ir::verify_module(&module) {
        let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        return Err(format!("{input}: verifier errors:\n{}", lines.join("\n")));
    }
    let asm = keel_codegen::aarch64::compile(&module).map_err(|e| format!("{input}: {e}"))?;
    match output {
        Some(path) => std::fs::write(path, asm).map_err(|e| format!("{path}: {e}")),
        None => {
            print!("{asm}");
            Ok(())
        }
    }
}

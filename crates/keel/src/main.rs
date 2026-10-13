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
    let funcs = keel_ir::parse(&src).map_err(|e| format!("{input}:{e}"))?;
    for func in &funcs {
        if let Err(errors) = keel_ir::verify(func) {
            let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
            return Err(format!(
                "{input}: verifier errors in @{}:\n{}",
                func.name,
                lines.join("\n")
            ));
        }
    }
    let asm = keel_codegen::aarch64::compile(&funcs).map_err(|e| format!("{input}: {e}"))?;
    match output {
        Some(path) => std::fs::write(path, asm).map_err(|e| format!("{path}: {e}")),
        None => {
            print!("{asm}");
            Ok(())
        }
    }
}

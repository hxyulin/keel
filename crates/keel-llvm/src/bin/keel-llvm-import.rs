//! Native replacement for suite/llvm_import.py.
use keel_llvm::import::{Importer, Options};
use std::{
    io::{Read, Write},
    path::Path,
};
fn run() -> Result<(), String> {
    let mut verifier = None;
    let mut library = None;
    let mut input = None;
    let mut output = None;
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--libllvm" => library = Some(args.next().ok_or("--libllvm requires a path")?),
            "--keel" => verifier = Some(args.next().ok_or("--keel requires a path")?),
            "--localize-odr-functions" => options.localize_odr_functions = true,
            "-o" | "--output" => output = Some(args.next().ok_or("output requires a path or -")?),
            "-h" | "--help" => {
                println!(
                    "keel-llvm-import --libllvm LLVM23_LIBRARY [--localize-odr-functions] INPUT.ll|- [-o OUTPUT.keel|-]\n--keel PATH optionally adds an external verification check; verification always runs in-process."
                );
                return Ok(());
            }
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(arg).is_some() {
                    return Err("multiple inputs".into());
                }
            }
        }
    }
    let input = input.ok_or("input path or - required")?;
    let library = library.ok_or("--libllvm required")?;
    let mut source = Vec::new();
    if input == "-" {
        std::io::stdin()
            .read_to_end(&mut source)
            .map_err(|e| e.to_string())?;
    } else {
        source = std::fs::read(&input).map_err(|e| format!("{input}: {e}"))?;
    }
    // SAFETY: --libllvm explicitly selects the trusted LLVM 23 runtime for this tool.
    let importer = unsafe { Importer::load(Path::new(&library)) }.map_err(|e| e.to_string())?;
    let text = importer
        .translate(&source, options)
        .map_err(|e| e.to_string())?;
    if let Some(verifier) = verifier {
        let dir = std::env::temp_dir().join(format!("keel-llvm-import-{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let result = (|| {
            let input = dir.join("verify.keel");
            std::fs::write(&input, &text).map_err(|e| e.to_string())?;
            let result = std::process::Command::new(verifier)
                .args(["opt", "--opt-tier", "none"])
                .arg(input)
                .args(["-o", "/dev/null"])
                .output()
                .map_err(|e| e.to_string())?;
            if !result.status.success() {
                return Err(format!(
                    "Keel verification failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(dir);
        result?;
    }
    let output = output.unwrap_or_else(|| "-".into());
    if output == "-" {
        return std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string());
    }
    let path = Path::new(&output);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = parent.join(format!(".keel-llvm-import-{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("llvm-import: {e}");
        std::process::exit(1);
    }
}

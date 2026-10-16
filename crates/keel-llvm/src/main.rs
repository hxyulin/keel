use keel_llvm::{TargetProfile, export};
use std::io::{Read, Write};
use std::path::Path;

fn run() -> Result<(), String> {
    let mut input = None;
    let mut output = None;
    let mut target = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let value = args
                    .next()
                    .ok_or("--target requires arm64-apple-macosx11.0.0")?;
                target = Some(match value.as_str() {
                    "arm64-apple-macosx11.0.0" => TargetProfile::AppleArm64,
                    _ => {
                        return Err(format!(
                            "unsupported target {value}; only arm64-apple-macosx11.0.0 is supported"
                        ));
                    }
                });
            }
            "-o" => {
                output = Some(args.next().ok_or("-o requires a path or -")?);
            }
            "--help" | "-h" => {
                println!(
                    "keel-llvm --target arm64-apple-macosx11.0.0 <input.keel|-> [-o output.ll|-]\nDiagnostic exporter; see crates/keel-llvm/README.md for supported subset."
                );
                return Ok(());
            }
            "-" => {
                if input.replace(arg).is_some() {
                    return Err("multiple input files".into());
                }
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(arg).is_some() {
                    return Err("multiple input files".into());
                }
            }
        }
    }
    let target = target.ok_or("explicit --target arm64-apple-macosx11.0.0 required")?;
    let input = input.ok_or("input path or - required")?;
    let src = if input == "-" {
        let mut src = String::new();
        std::io::stdin()
            .read_to_string(&mut src)
            .map_err(|e| e.to_string())?;
        src
    } else {
        std::fs::read_to_string(&input).map_err(|e| format!("{input}: {e}"))?
    };
    let module = keel_ir::parse(&src).map_err(|e| format!("{input}: {e}"))?;
    let llvm = export(&module, target)?;
    if let Some(path) = output.filter(|s| s != "-") {
        // Only publish a complete successful translation. A same-directory
        // temporary file plus rename also avoids truncation on write failure.
        let path = Path::new(&path);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let tmp = parent.join(format!(".keel-llvm-{}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(llvm.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result.map_err(|e| e.to_string())?;
    } else {
        std::io::stdout()
            .write_all(llvm.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("keel-llvm: {e}");
        std::process::exit(1);
    }
}

use keel_cranelift::{OptLevel, Options, TargetProfile};
use std::io::{Read, Write};
use std::path::Path;

const USAGE: &str = "keel-cranelift --target arm64-apple-macosx11.0.0 [--opt none|speed] [--emit object|clif] [--no-verify] <input.keel|-> -o <output|->
Experimental diagnostic Keel IR -> Cranelift bridge; see crates/keel-cranelift/README.md.
--opt defaults to none. --emit defaults to object (Mach-O). The CLIF verifier
runs unless --no-verify is given. Objects require -o <path>.";

fn run() -> Result<(), String> {
    let mut input = None;
    let mut output = None;
    let mut target = None;
    let mut opt = OptLevel::None;
    let mut clif = false;
    let mut verify_clif = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let value = args
                    .next()
                    .ok_or("--target requires arm64-apple-macosx11.0.0")?;
                target = Some(TargetProfile::from_cli(&value).ok_or_else(|| {
                    format!(
                        "unsupported target {value}; only arm64-apple-macosx11.0.0 is supported"
                    )
                })?);
            }
            "--opt" => {
                let value = args.next().ok_or("--opt requires none or speed")?;
                opt = OptLevel::parse(&value)
                    .ok_or_else(|| format!("unknown --opt {value}; expected none or speed"))?;
            }
            "--emit" => match args.next().as_deref() {
                Some("object") => clif = false,
                Some("clif") => clif = true,
                _ => return Err("--emit requires object or clif".into()),
            },
            "--no-verify" => verify_clif = false,
            "-o" => output = Some(args.next().ok_or("-o requires a path or -")?),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(arg).is_some() {
                    return Err("multiple input files".into());
                }
            }
        }
    }
    let target = target.ok_or("explicit --target arm64-apple-macosx11.0.0 required")?;
    let input = input.ok_or("input path or - required")?;
    let output = output.ok_or("-o <output> required")?;
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
    let options = Options {
        target,
        opt,
        verify_clif,
    };
    // The whole result is buffered: nothing is written unless it succeeds.
    let bytes = if clif {
        keel_cranelift::translate_to_clif(&module, &options)?.into_bytes()
    } else {
        if output == "-" {
            return Err("refusing to write a binary object to stdout".into());
        }
        keel_cranelift::compile_object(&module, &options)?
    };
    if output == "-" {
        return std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| e.to_string());
    }
    // Publish through a same-directory temporary file and rename, so an
    // existing output survives any failure.
    let path = Path::new(&output);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = parent.join(format!(".keel-cranelift-{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    let result = (|| {
        file.write_all(&bytes)?;
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
        eprintln!("keel-cranelift: {e}");
        std::process::exit(1);
    }
}

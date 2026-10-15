use std::path::{Path, PathBuf};

pub fn run(args: &[String]) -> Result<(), String> {
    // An explicit fallback preserves the system driver's full argument surface.
    // It is selected before native parsing; native failures never trigger it.
    if args.iter().any(|a| a == "--system") {
        let mut command = std::process::Command::new("clang");
        for arg in args.iter().filter(|a| *a != "--system") {
            command.arg(if matches!(arg.as_str(), "--sysroot" | "-syslibroot") {
                "-isysroot"
            } else {
                arg
            });
        }
        let status = command
            .status()
            .map_err(|e| format!("system linker: {e}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("system linker exited with {status}"))
        };
    }

    let mut inputs = Vec::new();
    let mut libraries = vec!["System".to_owned()];
    let mut output = PathBuf::from("a.out");
    let mut root = None;
    let mut debug_directory = None;
    let mut dsym: Option<PathBuf> = None;
    let mut minimum = 11 << 16;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--debug-map" => {
                debug_directory = Some(PathBuf::from(
                    args.next().ok_or("--debug-map requires a directory")?,
                ))
            }
            "--dsym" => {
                dsym = Some(PathBuf::from(
                    args.next().ok_or("--dsym requires an output path")?,
                ))
            }
            "-o" => output = args.next().ok_or("-o requires a path")?.into(),
            "--sysroot" | "-syslibroot" => {
                root = Some(PathBuf::from(
                    args.next().ok_or("--sysroot requires a path")?,
                ))
            }
            "-l" => libraries.push(args.next().ok_or("-l requires a library")?.clone()),
            "-mmacosx-version-min" => {
                minimum = version(args.next().ok_or("minimum version required")?)?
            }
            a if a.starts_with("-l") => libraries.push(a[2..].into()),
            a if a.starts_with("-mmacosx-version-min=") => {
                minimum = version(a.strip_prefix("-mmacosx-version-min=").unwrap())?
            }
            a if a.starts_with('-') => {
                return Err(format!(
                    "unsupported native linker option `{a}`; use `keel link --system` for the Clang link path"
                ));
            }
            _ => inputs.push(PathBuf::from(arg)),
        }
    }
    if inputs.is_empty() {
        return Err("usage: keel link [--system] [--sysroot SDK] [-l NAME] <input.o|input.a>... -o executable".into());
    }
    let root = match root.or_else(|| {
        std::env::var_os("SDKROOT")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }) {
        Some(root) => root,
        None => {
            let result = std::process::Command::new("xcrun")
                .args(["--sdk", "macosx", "--show-sdk-path"])
                .output()
                .map_err(|e| format!("SDK discovery: {e}"))?;
            if !result.status.success() {
                return Err("SDK discovery failed; pass --sysroot".into());
            }
            PathBuf::from(String::from_utf8_lossy(&result.stdout).trim())
        }
    };
    let mut dylibs = Vec::new();
    for library in libraries {
        if library.is_empty() || library.contains('/') {
            return Err("library names must be nonempty basenames".into());
        }
        let path = root.join("usr/lib").join(format!("lib{library}.tbd"));
        let interface = keel_link::Dylib::from_tbd(&path, &root)?;
        if !dylibs
            .iter()
            .any(|d: &keel_link::Dylib| d.install_name == interface.install_name)
        {
            dylibs.push(interface);
        }
    }
    let owned: Vec<_> = inputs
        .iter()
        .map(|p| {
            std::fs::read(p)
                .map(|b| (p.to_string_lossy().into_owned(), b))
                .map_err(|e| format!("{}: {e}", p.display()))
        })
        .collect::<Result<_, _>>()?;
    let inputs: Vec<_> = owned
        .iter()
        .map(|(name, bytes)| keel_link::Input { name, bytes })
        .collect();
    if dsym.is_some() && debug_directory.is_none() {
        debug_directory = Some(output.with_extension("debug-objects"));
    }
    let debug_directory = debug_directory
        .map(|p| {
            if p.is_absolute() {
                Ok(p)
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(p))
                    .map_err(|e| e.to_string())
            }
        })
        .transpose()?;
    let executable = keel_link::link(
        &inputs,
        keel_link::Options {
            dylibs,
            minimum_macos: minimum,
            sdk_version: sdk_version(&root),
            debug_directory,
        },
    )?;
    for warning in &executable.warnings {
        eprintln!("keel link: warning: {warning}");
    }
    keel_link::write_signed(&executable, &output)?;
    if let Some(path) = dsym {
        let result = std::process::Command::new("/usr/bin/dsymutil")
            .arg(&output)
            .arg("-o")
            .arg(path)
            .output()
            .map_err(|e| format!("dsymutil: {e}"))?;
        if !result.status.success() {
            return Err(format!(
                "dsymutil failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        if !result.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&result.stderr));
        }
    }
    Ok(())
}
fn version(s: &str) -> Result<u32, String> {
    let parts = s
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid macOS version")?;
    if parts.is_empty()
        || parts.len() > 3
        || parts[0] > 65535
        || parts.iter().skip(1).any(|v| *v > 255)
    {
        return Err("invalid macOS version".into());
    }
    Ok((parts[0] << 16)
        | (parts.get(1).copied().unwrap_or(0) << 8)
        | parts.get(2).copied().unwrap_or(0))
}
fn sdk_version(root: &Path) -> u32 {
    // Build-version SDK is informational; 0 means unspecified when the SDK
    // metadata isn't available, rather than guessing from a symlink name.
    let Ok(text) = std::fs::read_to_string(root.join("SDKSettings.json")) else {
        return 0;
    };
    text.split('"')
        .collect::<Vec<_>>()
        .windows(4)
        .find_map(|w| {
            if w[0] == "Version" {
                version(w[2]).ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

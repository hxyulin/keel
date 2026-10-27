use crate::input::Source;
use std::path::{Path, PathBuf};

// This is Keel's native syntax, not a partial Clang option table. Put --system
// before any Clang-only option: its entire remaining argument surface is opaque.
fn native_value_option(arg: &str) -> bool {
    matches!(
        arg,
        "-o" | "-e"
            | "--entry"
            | "-T"
            | "-z"
            | "--script"
            | "--dynamic-linker"
            | "--rpath"
            | "-rpath"
            | "--sysroot"
            | "-syslibroot"
            | "-L"
            | "-l"
            | "--require-export"
            | "--debug-map"
            | "--dsym"
            | "-mmacosx-version-min"
    )
}

fn system_selector(args: &[String]) -> Option<usize> {
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        match arg.as_str() {
            "--system" => return Some(i),
            "--" => return None,
            "--target" | "-target" => i += 2,
            a if native_value_option(a) => i += 2,
            a if !a.starts_with('-')
                || matches!(
                    a,
                    "--dead-strip"
                        | "-dead_strip"
                        | "-r"
                        | "--relocatable"
                        | "--gc-sections"
                        | "--no-gc-sections"
                        | "--static"
                        | "-static"
                        | "-Bstatic"
                        | "-Bdynamic"
                        | "--whole-archive"
                        | "--no-whole-archive"
                        | "--start-group"
                        | "--end-group"
                        | "-("
                        | "-)"
                )
                || [
                    "--target=",
                    "--script=",
                    "-T",
                    "-target=",
                    "--sysroot=",
                    "-mmacosx-version-min=",
                    "-l",
                    "-L",
                ]
                .iter()
                .any(|prefix| a.starts_with(prefix)) =>
            {
                i += 1
            }
            _ => return None,
        }
    }
    None
}

pub fn run(args: &[String]) -> Result<(), String> {
    // Only the Keel prefix selects the route. Once selected, Clang owns the
    // opaque argument surface (including response/config files and environment).
    if let Some(system) = system_selector(args) {
        let mut command = std::process::Command::new("clang");
        for (i, arg) in args.iter().enumerate() {
            if i == system {
                continue;
            }
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

    let mut target_args = args.iter();
    while let Some(arg) = target_args.next() {
        if arg == "--" {
            break;
        }
        let target = if arg == "--target" || arg == "-target" {
            target_args.next().map(String::as_str)
        } else {
            if native_value_option(arg) {
                target_args.next();
                continue;
            }
            arg.strip_prefix("--target=")
                .or_else(|| arg.strip_prefix("-target="))
        };
        if target == Some("wasm32-unknown-unknown") {
            return Err(
                "unsupported target wasm32-unknown-unknown: native linking is unavailable".into(),
            );
        }
    }

    let mut native_args = Vec::new();
    let mut target = if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        }
    } else {
        "aarch64-apple-darwin"
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            native_args.push(arg.clone());
            native_args.extend(iter.cloned());
            break;
        } else if arg == "--target" {
            target = iter.next().ok_or("--target requires a target triple")?;
        } else {
            native_args.push(arg.clone());
            if native_value_option(arg) {
                if let Some(value) = iter.next() {
                    native_args.push(value.clone());
                }
            }
        }
    }
    match target {
        "x86_64-unknown-linux-gnu" => {
            return run_elf(&native_args, keel_link::elf::Machine::X86_64);
        }
        "aarch64-unknown-linux-gnu" | "aarch64-linux-gnu" => {
            return run_elf(&native_args, keel_link::elf::Machine::Aarch64);
        }
        "aarch64-apple-darwin" => {}
        _ => return Err(format!("unsupported linker target `{target}`")),
    }
    let args = &native_args;

    let mut inputs = Vec::new();
    let mut libraries = vec!["System".to_owned()];
    let mut output = PathBuf::from("a.out");
    let mut root = None;
    let mut debug_directory = None;
    let mut dsym: Option<PathBuf> = None;
    let mut dead_strip = false;
    let mut required_exports = Vec::new();
    let mut minimum = 11 << 16;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dead-strip" | "-dead_strip" => dead_strip = true,
            "--require-export" => required_exports.push(
                args.next()
                    .ok_or("--require-export requires a symbol")?
                    .clone(),
            ),
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
            if !cfg!(target_os = "macos") {
                return Err(
                    "macOS SDK required for Darwin linking; pass --sysroot or set SDKROOT".into(),
                );
            }
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
    let dylibs = load_dylibs(&root, &libraries, keel_link::Dylib::from_tbd)?;
    let owned: Vec<_> = inputs
        .iter()
        .map(|p| {
            Source::open(p)
                .map(|b| (p.to_string_lossy().into_owned(), b))
                .map_err(|e| format!("{}: {e}", p.display()))
        })
        .collect::<Result<_, _>>()?;
    let inputs: Vec<_> = owned
        .iter()
        .map(|(name, source)| keel_link::Input {
            name,
            bytes: source.bytes(),
        })
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
    let warnings = keel_link::link_signed(
        &inputs,
        keel_link::Options {
            dylibs,
            dead_strip,
            required_exports,
            minimum_macos: minimum,
            sdk_version: sdk_version(&root),
            debug_directory,
        },
        &output,
    )?;
    for warning in &warnings {
        eprintln!("keel link: warning: {warning}");
    }
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

fn load_dylibs(
    root: &Path,
    libraries: &[String],
    mut read: impl FnMut(&Path, &Path) -> Result<keel_link::Dylib, String>,
) -> Result<Vec<keel_link::Dylib>, String> {
    let mut dylibs = Vec::new();
    // This set belongs to one link with one sysroot and the parser's fixed
    // macOS arm64 target. Reuse the entire successfully parsed reexport graph
    // for aliases of that file, never for another file with the same install
    // name. Keep parsing the requested path so diagnostics retain its spelling.
    let mut parsed_files = std::collections::BTreeSet::new();
    for library in libraries {
        if library.is_empty() || library.contains('/') {
            return Err("library names must be nonempty basenames".into());
        }
        let path = root.join("usr/lib").join(format!("lib{library}.tbd"));
        // Identity lookup is only an optimization. Let the reader report any
        // failure using its existing error handling rather than canonicalize's.
        let identity = path.canonicalize().ok();
        if identity.as_ref().is_some_and(|p| parsed_files.contains(p)) {
            continue;
        }
        let interface = read(&path, root)?;
        if let Some(identity) = identity {
            parsed_files.insert(identity);
        }
        if !dylibs
            .iter()
            .any(|d: &keel_link::Dylib| d.install_name == interface.install_name)
        {
            dylibs.push(interface);
        }
    }
    Ok(dylibs)
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

fn append_elf_script(script: &mut Option<String>, path: &str) -> Result<(), String> {
    use std::io::Read;
    let combined = script.get_or_insert_with(String::new);
    let remaining = (1024usize * 1024).saturating_sub(combined.len().saturating_add(1));
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut text = String::new();
    file.take(remaining as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("{path}: {e}"))?;
    if text.len() > remaining {
        return Err("linker scripts exceed 1 MiB limit".into());
    }
    combined.push_str(&text);
    combined.push('\n');
    Ok(())
}

fn run_elf(args: &[String], machine: keel_link::elf::Machine) -> Result<(), String> {
    use keel_link::elf_selection::{Request as SelectionRequest, Selection};
    enum Request {
        Path(PathBuf),
        Library(String, bool),
        Control(SelectionRequest<'static>),
    }
    let mut requests = Vec::new();
    let mut directories = Vec::new();
    let mut sysroot = None;
    let mut output = PathBuf::from("a.out");
    let mut options = keel_link::elf::Options {
        machine,
        ..Default::default()
    };
    let force_static = args
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| matches!(a.as_str(), "--static" | "-static"));
    let mut static_libraries = force_static;
    let mut group_depth = 0usize;
    let mut relocatable = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => output = args.next().ok_or("-o requires a path")?.into(),
            "-e" | "--entry" => {
                options.entry = args.next().ok_or("--entry requires a symbol")?.clone();
                options.entry_override = Some(options.entry.clone());
            }
            "-z" => {
                let value = args.next().ok_or("-z requires an option")?;
                if let Some(size) = value.strip_prefix("max-page-size=") {
                    let page = if let Some(hex) = size.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                    } else {
                        size.parse()
                    };
                    options.max_page_size =
                        Some(page.map_err(|_| format!("invalid maximum page size `{size}`"))?);
                } else if value != "noexecstack" {
                    return Err(format!("unsupported native ELF -z option `{value}`"));
                }
            }
            "-T" | "--script" => {
                let path = args.next().ok_or("--script requires a path")?;
                append_elf_script(&mut options.script, path)?;
            }
            "-r" | "--relocatable" => relocatable = true,
            "--whole-archive" => requests.push(Request::Control(SelectionRequest::WholeArchive)),
            "--no-whole-archive" => {
                requests.push(Request::Control(SelectionRequest::NoWholeArchive))
            }
            "--start-group" | "-(" => {
                group_depth += 1;
                requests.push(Request::Control(SelectionRequest::StartGroup));
            }
            "--end-group" | "-)" => {
                group_depth = group_depth
                    .checked_sub(1)
                    .ok_or("--end-group without --start-group")?;
                requests.push(Request::Control(SelectionRequest::EndGroup));
            }
            "--gc-sections" => options.gc_sections = true,
            "--no-gc-sections" => options.gc_sections = false,
            "--dynamic-linker" => {
                options.interpreter = args
                    .next()
                    .ok_or("--dynamic-linker requires a path")?
                    .clone()
            }
            "--rpath" | "-rpath" => options
                .runpath
                .push(args.next().ok_or("--rpath requires a path")?.clone()),
            "--sysroot" => {
                sysroot = Some(PathBuf::from(
                    args.next().ok_or("--sysroot requires a directory")?,
                ))
            }
            "-L" => directories.push(PathBuf::from(args.next().ok_or("-L requires a directory")?)),
            "-l" => requests.push(Request::Library(
                args.next().ok_or("-l requires a library")?.clone(),
                static_libraries,
            )),
            "--static" | "-static" => {}
            "-Bstatic" => static_libraries = true,
            "-Bdynamic" => static_libraries = force_static,
            "--" => {
                requests.extend(args.map(|a| Request::Path(a.into())));
                break;
            }
            a if a.starts_with("--entry=") => {
                options.entry = a[8..].into();
                options.entry_override = Some(options.entry.clone());
            }
            a if a.starts_with("-e") => {
                options.entry = a[2..].into();
                options.entry_override = Some(options.entry.clone());
            }
            a if a.starts_with("--script=") => append_elf_script(&mut options.script, &a[9..])?,
            a if a.starts_with("-T") => append_elf_script(&mut options.script, &a[2..])?,
            a if a.starts_with("-L") => directories.push(PathBuf::from(&a[2..])),
            a if a.starts_with("-l") => {
                requests.push(Request::Library(a[2..].into(), static_libraries))
            }
            a if a.starts_with("--sysroot=") => sysroot = Some(a[10..].into()),
            a if a.starts_with('-') => {
                return Err(format!("unsupported native ELF linker option `{a}`"));
            }
            _ => requests.push(Request::Path(PathBuf::from(arg))),
        }
    }
    if group_depth != 0 {
        return Err("--start-group without --end-group".into());
    }
    if !requests.iter().any(|r| !matches!(r, Request::Control(_))) {
        return Err("usage: keel link --target x86_64-unknown-linux-gnu [-e _start] [-L dir] [-l name] <input.o|input.a|input.so>... -o executable".into());
    }
    // Cross links consult only explicitly supplied paths or the target sysroot.
    // Native Linux links also search conventional target-library directories.
    if let Some(root) = sysroot.or_else(|| cfg!(target_os = "linux").then(|| PathBuf::from("/"))) {
        for dir in [
            if machine == keel_link::elf::Machine::Aarch64 {
                "lib/aarch64-linux-gnu"
            } else {
                "lib/x86_64-linux-gnu"
            },
            if machine == keel_link::elf::Machine::Aarch64 {
                "usr/lib/aarch64-linux-gnu"
            } else {
                "usr/lib/x86_64-linux-gnu"
            },
            "lib64",
            "usr/lib64",
            "lib",
            "usr/lib",
        ] {
            directories.push(root.join(dir));
        }
    }
    let paths = requests
        .iter()
        .map(|r| match r {
            Request::Control(_) => Ok(None),
            Request::Path(path) => Ok(Some(path.clone())),
            Request::Library(name, static_only) => {
                if name.is_empty() || name.contains('/') || name.contains('\0') {
                    return Err("library names must be nonempty basenames".into());
                }
                let candidates = if let Some(exact) = name.strip_prefix(':') {
                    vec![exact.to_owned()]
                } else if *static_only {
                    vec![format!("lib{name}.a")]
                } else {
                    vec![format!("lib{name}.so"), format!("lib{name}.a")]
                };
                directories
                    .iter()
                    .flat_map(|d| candidates.iter().map(move |n| d.join(n)))
                    .find(|p| p.is_file())
                    .map(Some)
                    .ok_or_else(|| format!("library `{name}` not found; pass -L or --sysroot"))
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut owned = paths
        .iter()
        .map(|path| {
            path.as_ref()
                .map(|p| {
                    Source::open(p)
                        .map(|b| (p.to_string_lossy().into_owned(), b))
                        .map_err(|e| format!("{}: {e}", p.display()))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (path, input) in paths.iter().zip(&mut owned) {
        let (Some(path), Some((_, source))) = (path, input) else {
            continue;
        };
        if source.bytes().starts_with(b"!<thin>\n") {
            let target = match machine {
                keel_link::elf::Machine::X86_64 => keel_link::archive::Target::X86_64LinuxGnu,
                keel_link::elf::Machine::Aarch64 => keel_link::archive::Target::Aarch64LinuxGnu,
            };
            let regular = keel_link::archive::materialize_thin(source.bytes(), target, |name| {
                let member = path.parent().unwrap_or_else(|| Path::new(".")).join(name);
                let metadata =
                    std::fs::metadata(&member).map_err(|e| format!("{}: {e}", member.display()))?;
                if metadata.len() > 512 * 1024 * 1024 {
                    return Err("thin archive member exceeds 512 MiB limit".into());
                }
                std::fs::read(&member).map_err(|e| format!("{}: {e}", member.display()))
            })
            .map_err(|e| format!("{}: {e}", path.display()))?;
            *source = Source::Read(regular);
        }
    }
    if relocatable
        && (options.gc_sections
            || !options.runpath.is_empty()
            || options.script.is_some()
            || options.max_page_size.is_some())
    {
        return Err("partial linking does not support scripts, maximum-page-size overrides, --gc-sections or --rpath".into());
    }
    if force_static
        && owned.iter().flatten().any(|(_, source)| {
            let b = source.bytes();
            b.len() >= 18 && &b[..4] == b"\x7fELF" && b[16..18] == [3, 0]
        })
    {
        return Err("--static cannot link shared libraries".into());
    }
    let requests = requests
        .iter()
        .zip(&owned)
        .map(|(request, source)| match request {
            Request::Control(control) => *control,
            _ => {
                let (name, source) = source.as_ref().expect("file request was loaded");
                SelectionRequest::File {
                    name,
                    bytes: source.bytes(),
                }
            }
        })
        .collect::<Vec<_>>();
    let entry = if relocatable {
        String::new()
    } else {
        options.effective_entry()?
    };
    let selection = Selection::prepare(&requests, &entry)?;
    let inputs = selection.inputs();
    let executable = if relocatable {
        keel_link::elf::relocatable::link(&inputs, machine)?
    } else {
        keel_link::elf::link(&inputs, options)?
    };
    for warning in &executable.warnings {
        eprintln!("keel link: warning: {warning}");
    }
    // Write beside the destination and install only after the complete image is
    // written. create_new prevents collisions with concurrent linker processes.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = output.with_file_name(format!(".keel-elf-{}-{stamp}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("{}: {e}", temporary.display()))?;
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        file.write_all(&executable.bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(if relocatable {
                0o644
            } else {
                0o755
            }))?;
        }
        drop(file);
        std::fs::rename(&temporary, &output)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("{}: {e}", output.display()))
}

#[cfg(test)]
mod tbd_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Sdk(PathBuf);
    impl Sdk {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "keel-cli-tbd-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("usr/lib")).unwrap();
            Self(root)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join("usr/lib").join(format!("lib{name}.tbd"))
        }
        fn write(&self, name: &str, install: &str, symbol: &str) {
            std::fs::write(
                self.path(name),
                format!(
                    "--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: '{install}'\nexports:\n  - targets: [arm64-macos]\n    symbols: [{symbol}]\n"
                ),
            )
            .unwrap();
        }
        fn load(&self, names: &[&str]) -> Result<(Vec<keel_link::Dylib>, Vec<PathBuf>), String> {
            let mut reads = Vec::new();
            let libraries = names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            let dylibs = load_dylibs(&self.0, &libraries, |path, root| {
                reads.push(path.to_owned());
                keel_link::Dylib::from_tbd(path, root)
            })?;
            Ok((dylibs, reads))
        }
    }
    impl Drop for Sdk {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_aliases_parse_one_interface_graph_in_request_order() {
        let sdk = Sdk::new();
        sdk.write("System.B", "/usr/lib/libSystem.B.dylib", "_system");
        for (alias, target) in [
            ("System", "System.B"),
            ("m", "System"),
            ("pthread", "System.B"),
        ] {
            std::os::unix::fs::symlink(format!("lib{target}.tbd"), sdk.path(alias)).unwrap();
        }
        sdk.write("other", "/usr/lib/libother.dylib", "_other");
        let (dylibs, reads) = sdk
            .load(&["System", "m", "other", "pthread", "System"])
            .unwrap();
        assert_eq!(reads, [sdk.path("System"), sdk.path("other")]);
        assert_eq!(dylibs.len(), 2);
        assert_eq!(dylibs[0].install_name, "/usr/lib/libSystem.B.dylib");
        assert_eq!(dylibs[1].install_name, "/usr/lib/libother.dylib");
    }

    #[test]
    fn distinct_files_with_same_install_name_are_validated_and_first_exports_win() {
        let sdk = Sdk::new();
        sdk.write("first", "/usr/lib/libsame.dylib", "_first");
        sdk.write("second", "/usr/lib/libsame.dylib", "_second");
        for (names, selected, absent) in [
            (["first", "second", "second"], "_first", "_second"),
            (["second", "first", "first"], "_second", "_first"),
        ] {
            let (dylibs, reads) = sdk.load(&names).unwrap();
            assert_eq!(reads, [sdk.path(names[0]), sdk.path(names[1])]);
            assert_eq!(dylibs.len(), 1);
            assert_eq!(dylibs[0].exports, BTreeSet::from([selected.into()]));
            let object = keel_asm::assemble(&format!(".globl _main\n_main:\nb {absent}\n"))
                .unwrap()
                .to_macho()
                .unwrap();
            let error = keel_link::link(
                &[keel_link::Input {
                    name: "missing.o",
                    bytes: &object,
                }],
                keel_link::Options {
                    dylibs,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            assert!(
                error.contains(&format!("undefined symbol `{absent}`")),
                "{error}"
            );
        }
    }

    #[test]
    fn distinct_files_with_same_install_name_still_report_parser_failures() {
        let sdk = Sdk::new();
        sdk.write("first", "/usr/lib/libsame.dylib", "_first");
        for text in [
            "--- !tapi-tbd\ntbd-version: 4\ntargets: [x86_64-macos]\ninstall-name: '/usr/lib/libsame.dylib'\n",
            "--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: '/usr/lib/libsame.dylib'\nexports: [\n",
            "--- !tapi-tbd\ntbd-version: 3\ninstall-name: '/usr/lib/libsame.dylib'\n",
        ] {
            std::fs::write(sdk.path("second"), text).unwrap();
            let expected = keel_link::Dylib::from_tbd(&sdk.path("second"), &sdk.0).unwrap_err();
            assert_eq!(sdk.load(&["first", "second"]).unwrap_err(), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn aliases_preserve_requested_path_diagnostics_and_missing_file_errors() {
        let sdk = Sdk::new();
        std::fs::write(sdk.path("bad"), "exports: [").unwrap();
        std::os::unix::fs::symlink("libbad.tbd", sdk.path("alias")).unwrap();
        std::os::unix::fs::symlink("libabsent.tbd", sdk.path("dangling")).unwrap();
        for name in ["alias", "missing", "dangling"] {
            let path = sdk.path(name);
            let expected = keel_link::Dylib::from_tbd(&path, &sdk.0).unwrap_err();
            assert!(expected.starts_with(&format!("{}:", path.display())));
            assert_eq!(sdk.load(&[name]).unwrap_err(), expected);
        }
        for name in ["", "../bad"] {
            assert_eq!(
                sdk.load(&[name]).unwrap_err(),
                "library names must be nonempty basenames"
            );
        }
    }

    #[test]
    fn reexports_keep_sysroot_target_filtering_and_weak_exports_per_invocation() {
        let sdk = Sdk::new();
        let other_sdk = Sdk::new();
        // Both roots deliberately reference the exact same umbrella file. Its
        // external reexports must be resolved separately against each sysroot.
        let umbrella = r#"--- !tapi-tbd
tbd-version: 4
targets: [arm64e-macos]
install-name: '/usr/lib/libumbrella.dylib'
exports:
  - targets: [arm64-macos]
    symbols: [_right]
    weak-symbols: [_weak]
    thread-local-symbols: [_thread]
  - targets: [x86_64-macos, arm64-ios, arm64e.x1-macos]
    symbols: [_wrong]
reexported-libraries:
  - targets: [arm64-macos]
    libraries: ['/usr/lib/libchild.dylib', '/usr/lib/libembedded.dylib']
  - targets: [arm64-ios]
    libraries: ['/usr/lib/libmissing.dylib']
--- !tapi-tbd
tbd-version: 4
targets: [arm64-macos]
install-name: '/usr/lib/libembedded.dylib'
exports:
  - targets: [arm64-macos]
    symbols: [_embedded]
--- !tapi-tbd
tbd-version: 4
targets: [arm64-macos]
install-name: '/usr/lib/libunreachable.dylib'
exports:
  - targets: [arm64-macos]
    symbols: [_unreachable]
"#;
        std::fs::write(sdk.path("umbrella"), umbrella).unwrap();
        std::fs::hard_link(sdk.path("umbrella"), other_sdk.path("umbrella")).unwrap();
        for (root, symbol) in [(&sdk, "_child"), (&other_sdk, "_other_child")] {
            root.write("child", "/usr/lib/libchild.dylib", symbol);
            let (dylibs, reads) = root.load(&["umbrella", "umbrella"]).unwrap();
            assert_eq!(reads, [root.path("umbrella")]);
            assert_eq!(
                dylibs[0].exports,
                BTreeSet::from([
                    "_right".into(),
                    "_weak".into(),
                    "_thread".into(),
                    "_embedded".into(),
                    symbol.into(),
                ])
            );
            assert_eq!(dylibs[0].weak_exports, BTreeSet::from(["_weak".into()]));
        }
        sdk.write("child", "/usr/lib/libchild.dylib", "_updated");
        let (dylibs, _) = sdk.load(&["umbrella"]).unwrap();
        assert!(dylibs[0].exports.contains("_updated"));
        assert!(!dylibs[0].exports.contains("_child"));
    }

    #[test]
    fn missing_and_cyclic_reexport_failures_are_preserved() {
        let sdk = Sdk::new();
        sdk.write("parent", "/usr/lib/libparent.dylib", "_parent");
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(sdk.path("parent"))
            .unwrap();
        writeln!(file, "reexported-libraries:\n  - targets: [arm64-macos]\n    libraries: ['/usr/lib/libchild.dylib']").unwrap();
        for cycle in [false, true] {
            if cycle {
                sdk.write("child", "/usr/lib/libchild.dylib", "_child");
                let mut child = std::fs::OpenOptions::new()
                    .append(true)
                    .open(sdk.path("child"))
                    .unwrap();
                writeln!(child, "reexported-libraries:\n  - targets: [arm64-macos]\n    libraries: ['/usr/lib/libparent.dylib']").unwrap();
            }
            let expected = keel_link::Dylib::from_tbd(&sdk.path("parent"), &sdk.0).unwrap_err();
            assert_eq!(sdk.load(&["parent", "parent"]).unwrap_err(), expected);
        }
    }
}

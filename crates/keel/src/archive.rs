use keel_link::archive::{Target, write};
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(Target::X86_64LinuxGnu)
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(Target::Aarch64LinuxGnu)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(Target::Aarch64AppleDarwin)
    } else {
        None
    };
    let mut output = None;
    let mut members = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                target = Some(
                    match args
                        .next()
                        .ok_or("--target requires a target triple")?
                        .as_str()
                    {
                        "x86_64-unknown-linux-gnu" => Target::X86_64LinuxGnu,
                        "aarch64-unknown-linux-gnu" | "aarch64-linux-gnu" => {
                            Target::Aarch64LinuxGnu
                        }
                        "aarch64-apple-darwin" => Target::Aarch64AppleDarwin,
                        other => return Err(format!("unsupported archive target `{other}`")),
                    },
                )
            }
            "-o" => {
                if output.is_some() {
                    return Err("duplicate -o".into());
                }
                output = Some(PathBuf::from(args.next().ok_or("-o requires a path")?));
            }
            "--" => {
                members.extend(args.cloned());
                break;
            }
            a if a.starts_with('-') => return Err(format!("unsupported archive option `{a}`")),
            _ => members.push(arg.clone()),
        }
    }
    let output =
        output.ok_or("usage: keel archive [--target <triple>] -o <output.a> <member.o>...")?;
    if members.is_empty() {
        return Err("archive requires at least one member object".into());
    }
    let target = target.ok_or("archive target required on this host; pass --target")?;
    let owned = members
        .iter()
        .map(|name| {
            std::fs::read(name)
                .map(|b| (name, b))
                .map_err(|e| format!("{name}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = owned
        .iter()
        .map(|(name, bytes)| keel_link::Input { name, bytes })
        .collect::<Vec<_>>();
    write(&inputs, target, &output)
}

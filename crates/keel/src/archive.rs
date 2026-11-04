use crate::target::{parse_target, resolve_target};
use keel_link::archive::{Target, write};
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut target = None;
    let mut output = None;
    let mut members = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let parsed = parse_target(args.next().ok_or("--target requires a target triple")?)?;
                if parsed == Target::Wasm32UnknownUnknown {
                    return Err("unsupported target wasm32-unknown-unknown: native archives are unavailable".into());
                }
                target = Some(parsed);
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
    let target = resolve_target(target)?;
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

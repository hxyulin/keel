//! One process can assemble several independent inputs without retaining their
//! mappings or assembly state. Batch output names are validated before writing.
use crate::input::Source;
use keel_asm::TargetFeatures;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Options {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    features: TargetFeatures,
    x86_features: keel_asm::x86::Features,
    x86_level: Option<keel_asm::x86::CpuLevel>,
    x86: bool,
    arm_elf: bool,
    x86_syntax: Option<keel_asm::x86::Syntax>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut opts = Self {
            arm_elf: cfg!(all(target_os = "linux", target_arch = "aarch64")),
            ..Self::default()
        };
        let mut args = args.iter();
        let mut positional = false;
        while let Some(arg) = args.next() {
            if positional {
                opts.inputs.push(arg.into());
                continue;
            }
            match arg.as_str() {
                "--" => positional = true,
                "-o" => {
                    let value = args.next().ok_or("`-o` needs a file")?;
                    if opts.output.replace(value.into()).is_some() {
                        return Err("`-o` may only be specified once".into());
                    }
                }
                "--out-dir" => {
                    let value = args.next().ok_or("`--out-dir` needs a directory")?;
                    if opts.out_dir.replace(value.into()).is_some() {
                        return Err("`--out-dir` may only be specified once".into());
                    }
                }
                "--target" => {
                    let (x86, arm_elf) = match args.next().map(String::as_str) {
                        Some("x86_64-unknown-linux-gnu" | "x86_64-linux-gnu") => (true, false),
                        Some("aarch64-unknown-linux-gnu" | "aarch64-linux-gnu") => (false, true),
                        Some("aarch64-apple-darwin") => (false, false),
                        Some("wasm32-unknown-unknown") => return Err("unsupported target wasm32-unknown-unknown: native assembly is unavailable".into()),
                        _ => return Err("assembler --target expects x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu or aarch64-apple-darwin".into()),
                    };
                    opts.x86 = x86;
                    opts.arm_elf = arm_elf;
                }
                "--x86-syntax" => {
                    opts.x86_syntax = Some(match args.next().map(String::as_str) {
                        Some("intel") => keel_asm::x86::Syntax::Intel,
                        Some("att") => keel_asm::x86::Syntax::Att,
                        _ => return Err("--x86-syntax expects intel or att".into()),
                    });
                }
                "--cpu-level" => {
                    let name = args
                        .next()
                        .ok_or("--cpu-level needs v1, v2, v3 or legacy")?;
                    opts.x86_level = if name == "legacy" {
                        None
                    } else {
                        Some(keel_asm::x86::CpuLevel::parse(name)?)
                    };
                    opts.x86_features = opts
                        .x86_level
                        .map(keel_asm::x86::Features::for_level)
                        .unwrap_or_default();
                }
                "--target-features" => {
                    opts.x86_features = opts
                        .x86_level
                        .map(keel_asm::x86::Features::for_level)
                        .unwrap_or_default()
                        .with_features(
                            args.next()
                                .ok_or("--target-features needs avx,bmi1,bmi2 or none")?,
                        )?;
                }
                "--cpu" => {
                    opts.features = match args.next().map(String::as_str) {
                        Some("baseline") => TargetFeatures::default(),
                        Some("apple-m5") => TargetFeatures::apple_m5(),
                        Some(cpu) => {
                            return Err(format!(
                                "unknown assembler CPU `{cpu}`; expected baseline or apple-m5"
                            ));
                        }
                        None => return Err("`--cpu` needs baseline or apple-m5".into()),
                    };
                }
                flag if flag.starts_with('-') => {
                    return Err(format!("unknown flag `{flag}`\n{}", crate::USAGE));
                }
                _ => opts.inputs.push(arg.into()),
            }
        }
        if opts.x86 && opts.features != TargetFeatures::default() {
            return Err("--cpu apple-m5 requires the AArch64 assembler target".into());
        }
        if !opts.x86 && opts.x86_syntax.is_some() {
            return Err("--x86-syntax requires the x86 assembler target".into());
        }
        if !opts.x86 && !opts.x86_features.is_empty() {
            return Err("x86 target features require the x86 assembler target".into());
        }
        if opts.inputs.is_empty() {
            return Err(crate::USAGE.into());
        }
        match (&opts.output, &opts.out_dir) {
            (Some(_), Some(_)) => return Err("`-o` and `--out-dir` are mutually exclusive".into()),
            (Some(_), None) if opts.inputs.len() != 1 => {
                return Err(
                    "`-o` requires exactly one input; use `--out-dir` for multiple inputs".into(),
                );
            }
            (None, None) => {
                return Err("`asm` requires `-o <output.o>` or `--out-dir <directory>`".into());
            }
            _ => {}
        }
        Ok(opts)
    }

    fn outputs(&self) -> Result<Vec<PathBuf>, String> {
        if let Some(output) = &self.output {
            // Preserve single-file in-place output, releasing the map first.
            return Ok(vec![output.clone()]);
        }
        let dir = self.out_dir.as_ref().unwrap();
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let dir = std::fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut inputs = HashSet::new();
        let mut input_ids = HashSet::new();
        for input in &self.inputs {
            let path =
                std::fs::canonicalize(input).map_err(|e| format!("{}: {e}", input.display()))?;
            inputs.insert(path_key(&path));
            if let Some(id) = file_id(input) {
                input_ids.insert(id);
            }
        }
        let mut names = HashSet::new();
        let mut output_ids = HashSet::new();
        let mut outputs = Vec::with_capacity(self.inputs.len());
        for input in &self.inputs {
            let name = input
                .file_name()
                .ok_or_else(|| format!("{}: input has no file name", input.display()))?;
            let output = dir.join(name).with_extension("o");
            // Canonicalization also catches existing symlink aliases.
            let resolved = std::fs::canonicalize(&output).unwrap_or_else(|_| output.clone());
            let key = path_key(&resolved);
            if inputs.contains(&key) || file_id(&output).is_some_and(|id| input_ids.contains(&id)) {
                return Err(format!(
                    "{}: batch output would overwrite an input",
                    output.display()
                ));
            }
            if !names.insert(key) || file_id(&output).is_some_and(|id| !output_ids.insert(id)) {
                return Err(format!(
                    "{}: multiple inputs map to the same output",
                    output.display()
                ));
            }
            outputs.push(output);
        }
        Ok(outputs)
    }
}

// Conservatively reject case-only output collisions on macOS and Windows,
// including when the destination file does not exist yet.
fn path_key(path: &Path) -> PathBuf {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        PathBuf::from(path.as_os_str().to_string_lossy().to_lowercase())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        path.to_owned()
    }
}

#[cfg(unix)]
fn file_id(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_id(_: &Path) -> Option<(u64, u64)> {
    None
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let opts = Options::parse(args)?;
    let outputs = opts.outputs()?;
    for (input, output) in opts.inputs.iter().zip(outputs) {
        let src = Source::open(input).map_err(|e| format!("{}: {e}", input.display()))?;
        let text = match src.text() {
            Ok(text) => std::borrow::Cow::Borrowed(text),
            Err(e) if !opts.x86 => std::fs::read(input)
                .ok()
                .and_then(|bytes| comment_lossy(&bytes))
                .map(std::borrow::Cow::Owned)
                .ok_or_else(|| format!("{}: {e}", input.display()))?,
            Err(e) => return Err(format!("{}: {e}", input.display())),
        };
        let object = if opts.x86 {
            keel_asm::x86::assemble_object_with_syntax_and_features(
                &text,
                opts.x86_syntax.unwrap_or_default(),
                opts.x86_features,
            )
        } else if opts.arm_elf {
            keel_asm::assemble_elf_object_with_features(&text, opts.features)
        } else {
            keel_asm::assemble_object_with_features(&text, opts.features)
        }
        .map_err(|e| format!("{}: {e}", input.display()))?;
        drop(src);
        std::fs::write(&output, object).map_err(|e| format!("{}: {e}", output.display()))?;
    }
    Ok(())
}

/// Clang's verbose AArch64 assembly prints DWARF data bytes as raw characters
/// in `;` comments (such as `DW_OP_implicit_value` operands in DWARF 5
/// `__debug_loclists`), which need not be UTF-8. Only after strict decoding
/// fails, accept input whose invalid bytes all lie in `;` or `//` line
/// comments outside quotes, replacing them. Anything else (including invalid
/// bytes in strings, code, or block comments) still fails.
fn comment_lossy(bytes: &[u8]) -> Option<String> {
    let mut out = String::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        if let Ok(line) = std::str::from_utf8(line) {
            out.push_str(line);
            continue;
        }
        let (mut quoted, mut escaped, mut comment) = (false, false, None);
        for (i, &b) in line.iter().enumerate() {
            match b {
                _ if escaped => escaped = false,
                b'\\' if quoted => escaped = true,
                b'"' => quoted = !quoted,
                b';' if !quoted => comment = Some(i),
                b'/' if !quoted && line.get(i + 1) == Some(&b'/') => comment = Some(i),
                b'/' if !quoted && line.get(i + 1) == Some(&b'*') => return None,
                _ => {}
            }
            if comment.is_some() {
                break;
            }
        }
        let comment = comment?;
        out.push_str(std::str::from_utf8(&line[..comment]).ok()?);
        out.push_str(&String::from_utf8_lossy(&line[comment..]));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "keel-asm-batch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn invoke(&self, inputs: &[&str], flags: &[&str]) -> Result<(), String> {
            let mut args: Vec<String> = inputs
                .iter()
                .map(|p| self.0.join(p).display().to_string())
                .collect();
            args.extend(flags.iter().map(|s| s.to_string()));
            args.extend([
                "--out-dir".into(),
                self.0.join("objects").display().to_string(),
            ]);
            run(&args)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn batch_matches_independent_objects_and_resets_symbols_and_features() {
        let temp = Temp::new();
        let first = ".text\n.globl _same\n_same:\nsdot v0.4s,v1.16b,v2.16b\nret\n";
        let second = ".text\n.globl _same\n_same:\nmov x0,#42\nret\n";
        std::fs::write(temp.0.join("first.s"), first).unwrap();
        std::fs::write(temp.0.join("second.s"), second).unwrap();
        temp.invoke(&["first.s", "second.s"], &["--cpu", "apple-m5"])
            .unwrap();
        for (name, source) in [("first", first), ("second", second)] {
            assert_eq!(
                std::fs::read(temp.0.join(format!("objects/{name}.o"))).unwrap(),
                keel_asm::assemble_object_with_features(source, TargetFeatures::apple_m5())
                    .unwrap()
            );
        }
    }

    #[test]
    fn x86_source_dialect_is_explicit_and_resets_per_input() {
        let temp = Temp::new();
        std::fs::write(temp.0.join("default.s"), "movq %r10,%r9\nretq\n").unwrap();
        std::fs::write(
            temp.0.join("switch.s"),
            ".intel_syntax noprefix\nmov r9,r10\nret\n",
        )
        .unwrap();
        temp.invoke(
            &["switch.s", "default.s"],
            &["--x86-syntax", "att", "--target", "x86_64-linux-gnu"],
        )
        .unwrap();
        assert_eq!(
            std::fs::read(temp.0.join("objects/default.o")).unwrap(),
            std::fs::read(temp.0.join("objects/switch.o")).unwrap()
        );
        assert!(
            temp.invoke(&["default.s"], &["--target", "x86_64-linux-gnu"])
                .is_err()
        );
        for flags in [
            vec!["--x86-syntax", "att"],
            vec!["--target", "x86_64-linux-gnu", "--x86-syntax", "unknown"],
            vec!["--target", "x86_64-linux-gnu", "--x86-syntax"],
        ] {
            assert!(temp.invoke(&["default.s"], &flags).is_err());
        }
    }

    #[test]
    fn validates_output_modes_and_end_of_flags() {
        for (args, expected) in [
            (vec!["a.s", "b.s", "-o", "a.o"], "exactly one input"),
            (
                vec!["a.s", "-o", "a.o", "--out-dir", "out"],
                "mutually exclusive",
            ),
            (vec!["a.s", "b.s"], "requires"),
            (vec!["a.s", "--out-dir"], "needs a directory"),
            (
                vec!["a.s", "-o", "a.o", "-o", "b.o"],
                "only be specified once",
            ),
        ] {
            let args: Vec<_> = args.into_iter().map(String::from).collect();
            assert!(Options::parse(&args).err().unwrap().contains(expected));
        }
        let args = ["-o", "out.o", "--", "-source.s"].map(String::from);
        assert_eq!(
            Options::parse(&args).unwrap().inputs,
            [PathBuf::from("-source.s")]
        );
    }

    #[test]
    fn rejects_collisions_before_writing_any_objects() {
        let temp = Temp::new();
        for dir in ["a", "b", "objects"] {
            std::fs::create_dir(temp.0.join(dir)).unwrap();
        }
        for name in ["a/input.s", "b/input.s"] {
            std::fs::write(temp.0.join(name), ".text\nret\n").unwrap();
        }
        std::fs::write(temp.0.join("objects/input.o"), "preserve").unwrap();
        assert!(
            temp.invoke(&["a/input.s", "b/input.s"], &[])
                .unwrap_err()
                .contains("same output")
        );
        assert_eq!(
            std::fs::read(temp.0.join("objects/input.o")).unwrap(),
            b"preserve"
        );
        assert!(
            temp.invoke(&["objects/input.o"], &[])
                .unwrap_err()
                .contains("overwrite an input")
        );
        assert!(
            temp.invoke(&["a/input.s", "missing.s"], &[])
                .unwrap_err()
                .contains("missing.s")
        );
        assert_eq!(
            std::fs::read(temp.0.join("objects/input.o")).unwrap(),
            b"preserve"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_output_aliases_to_other_inputs() {
        let temp = Temp::new();
        std::fs::create_dir(temp.0.join("objects")).unwrap();
        std::fs::write(temp.0.join("first.s"), ".text\nret\n").unwrap();
        std::fs::write(temp.0.join("second.s"), ".text\nnop\n").unwrap();
        std::fs::hard_link(temp.0.join("second.s"), temp.0.join("objects/first.o")).unwrap();
        assert!(
            temp.invoke(&["first.s", "second.s"], &[])
                .unwrap_err()
                .contains("overwrite an input")
        );
        std::fs::remove_file(temp.0.join("objects/first.o")).unwrap();
        std::os::unix::fs::symlink(temp.0.join("second.s"), temp.0.join("objects/first.o"))
            .unwrap();
        assert!(
            temp.invoke(&["first.s", "second.s"], &[])
                .unwrap_err()
                .contains("overwrite an input")
        );
    }

    #[test]
    fn batch_stops_at_first_assembly_error_and_keeps_completed_outputs() {
        let temp = Temp::new();
        for (name, src) in [
            ("first.s", "ret\n"),
            ("bad.s", "invalid_instruction\n"),
            ("last.s", "nop\n"),
        ] {
            std::fs::write(temp.0.join(name), src).unwrap();
        }
        let error = temp
            .invoke(&["first.s", "bad.s", "last.s"], &[])
            .unwrap_err();
        assert!(error.contains("bad.s"));
        assert!(temp.0.join("objects/first.o").exists());
        assert!(!temp.0.join("objects/bad.o").exists());
        assert!(!temp.0.join("objects/last.o").exists());
    }

    #[test]
    fn invalid_utf8_is_accepted_only_inside_line_comments() {
        let fixed = comment_lossy(b".byte 228 ; \xe4\n.byte 1 // \xff\xfe\r\nret\n").unwrap();
        assert_eq!(
            fixed,
            ".byte 228 ; \u{fffd}\n.byte 1 // \u{fffd}\u{fffd}\r\nret\n"
        );
        for bad in [
            &b".ascii \"\xe4\"\n"[..],
            b".ascii \"; \xe4\"\n",
            b".ascii \"\\\"; \xe4\"\n",
            b"nop \xe4 ; x\n",
            b"nop /* \xe4 */\n",
            b"nop # \xe4\n",
        ] {
            assert_eq!(comment_lossy(bad), None, "{}", String::from_utf8_lossy(bad));
        }
        let temp = Temp::new();
        let input = temp.0.join("raw.s");
        let mut source = b".data\n.byte 228 ; ".to_vec();
        source.extend([0xe4, b'\n']);
        std::fs::write(&input, &source).unwrap();
        temp.invoke(&["raw.s"], &[]).unwrap();
        let object = std::fs::read(temp.0.join("objects/raw.o")).unwrap();
        assert_eq!(
            object,
            keel_asm::assemble_object_with_features(
                ".data\n.byte 228\n",
                TargetFeatures::default()
            )
            .unwrap()
        );
        source.extend(b".ascii \"");
        source.extend([0xe4, b'"', b'\n']);
        std::fs::write(&input, &source).unwrap();
        assert!(temp.invoke(&["raw.s"], &[]).unwrap_err().contains("UTF-8"));
        temp.invoke(&["raw.s"], &["--target", "x86_64-unknown-linux-gnu"])
            .unwrap_err();
    }
}

#[cfg(test)]
mod feature_option_tests {
    use super::*;
    #[test]
    fn feature_flags_are_x86_only_and_order_independent() {
        let parse = |target: &str| {
            Options::parse(
                &[
                    "--target-features",
                    "bmi1,bmi2",
                    "input.s",
                    "-o",
                    "out.o",
                    "--target",
                    target,
                ]
                .map(str::to_owned),
            )
        };
        assert_eq!(
            parse("x86_64-unknown-linux-gnu").unwrap().x86_features,
            keel_asm::x86::Features {
                bmi1: true,
                bmi2: true,
                ..Default::default()
            }
        );
        assert!(parse("aarch64-apple-darwin").is_err());
    }
}

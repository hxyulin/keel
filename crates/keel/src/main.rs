//! The keel driver:
//!
//! ```text
//! keel compile [-O] [-g] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.keel> [-o <output.s>]   IR to assembly
//! keel cc [-O] [-g] [-E [-dM]|--emit-ir] [-I dir] [-D name[=value]] [-U name] [--sysroot dir] [-isystem dir] [-mmacosx-version-min=version] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.c|input.i> [-o <output>]  C to assembly
//! keel opt [--opt-tier <tier>] [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
//! keel asm [--cpu <baseline|apple-m5>] <input.s> -o <output.o>                 A64 assembly to Mach-O
//! keel asm [--cpu <baseline|apple-m5>] <input.s>... --out-dir <directory>      batch A64 assembly
//! keel asm-metrics <input.s> [-o <metrics.json>]     exact instruction locations and sizes
//! keel passes                                      list the passes
//! ```
//!
//! `-O` runs the default pass pipeline before code generation; `opt` runs
//! the given passes (the default pipeline without `-p`) and prints the IR.
//! Assembly has debug info if the input names source files (`file`
//! lines); `-g` gives an input without them its own lines instead, for
//! stepping through the IR in a debugger.
//! `cc` preprocesses raw C (or accepts `.i` files), then compiles to assembly or,
//! with `--emit-ir`, to keel IR; `-E` stops after preprocessing; `-g` gives it debug info, with every
//! local in memory where a debugger can see it (link with `clang -g`).
//! `compile`/`cc --emit-object -o output.o` compose structured codegen and
//! object emission in-process for AArch64 Mach-O.
//! Text output goes to stdout without `-o`; `asm` requires `-o` and always
//! writes an Apple arm64 Mach-O object. `--target` picks the machine for codegen,
//! `aarch64-apple-darwin` or `x86_64-unknown-linux-gnu`; the default is
//! the one keel runs on.

use keel_codegen::Target;
use keel_ir::Module;
use std::process::ExitCode;
mod asm;
mod input;

const USAGE: &str = "usage:
  keel compile [-O] [-g] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.keel> [-o <output.s>]
  keel cc [-O] [-g] [-E [-dM]|--emit-ir] [-I dir] [-D name[=value]] [-U name] [--sysroot dir] [-isystem dir] [-mmacosx-version-min=version] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.c|input.i> [-o <output>]
  keel opt [--opt-tier <tier>] [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
  keel compile|cc [--emit-object] <input> -o <output.o>  (AArch64 Mach-O)
  keel asm [--cpu <baseline|apple-m5>] <input.s> -o <output.o>
  keel asm [--cpu <baseline|apple-m5>] <input.s>... --out-dir <directory>
  keel asm-metrics <input.s> [-o <metrics.json>]
  keel passes

  Input files must remain unchanged during parsing.";

fn main() -> ExitCode {
    #[cfg(feature = "profile-with-tracy")]
    let _client = tracy_client::Client::start();
    profiling::register_thread!("keel");
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("keel: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Command-line flags shared by the subcommands.
#[derive(Default)]
struct Options {
    input: Option<String>,
    output: Option<String>,
    optimization: keel_opt::OptimizationTier,
    allocation: keel_codegen::regalloc::AllocationTier,
    selection: keel_codegen::SelectionOptions,
    tuning: keel_codegen::tuning::TuningCpu,
    ir_lines: bool,
    emit_ir: bool,
    emit_object: bool,
    preprocess_only: bool,
    dump_macros: bool,
    sysroot: Option<std::path::PathBuf>,
    system_include_dirs: Vec<std::path::PathBuf>,
    darwin_deployment: Option<String>,
    include_dirs: Vec<std::path::PathBuf>,
    defines: Vec<(String, Option<String>)>,
    passes: Option<Vec<String>>,
    target: Option<Target>,
}

impl Options {
    /// Explicit pipelines override tier presets, independently of flag order.
    fn optimize(&self, module: &mut Module) -> Result<(), keel_opt::OptError> {
        match &self.passes {
            Some(names) => {
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                keel_opt::run_passes(module, &names)
            }
            None => keel_opt::optimize_with_tier(module, self.optimization),
        }
    }

    fn target(&self) -> Result<Target, String> {
        let target = self.target.or_else(Target::host).ok_or_else(|| {
            "keel does not run on this machine's target; pass `--target`".to_string()
        })?;
        self.tuning
            .validate_target(target)
            .map_err(str::to_string)?;
        Ok(target)
    }

    fn preprocessing_sysroot(&self, target: Target) -> Result<Option<std::path::PathBuf>, String> {
        if let Some(root) = &self.sysroot {
            if !root.is_dir() {
                return Err(format!("sysroot is not a directory: {}", root.display()));
            }
            return Ok(Some(root.clone()));
        }
        if target != Target::Aarch64AppleDarwin {
            return Ok(None);
        }
        if let Some(root) = std::env::var_os("SDKROOT").filter(|s| !s.is_empty()) {
            let root = std::path::PathBuf::from(root);
            if !root.is_dir() {
                return Err(format!("SDKROOT is not a directory: {}", root.display()));
            }
            return Ok(Some(root));
        }
        if cfg!(target_os = "macos")
            && let Ok(out) = std::process::Command::new("xcrun")
                .args(["--sdk", "macosx", "--show-sdk-path"])
                .output()
            && out.status.success()
        {
            let root = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if root.is_dir() {
                return Ok(Some(root));
            }
        }
        // Sources using only compiler headers can still compile without an SDK.
        Ok(None)
    }
}

fn parse_options(args: &[String], allowed: &[&str]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let raw = arg.as_str();
        let flag = if raw.starts_with("--sysroot=") {
            "--sysroot"
        } else if raw.starts_with("-mmacosx-version-min=") {
            "-mmacosx-version-min"
        } else if raw.starts_with("-I") {
            "-I"
        } else if raw.starts_with("-D") {
            "-D"
        } else if raw.starts_with("-U") {
            "-U"
        } else {
            raw
        };
        if flag.starts_with('-') && !allowed.contains(&flag) {
            return Err(format!("unknown flag `{flag}`\n{USAGE}"));
        }
        match flag {
            "-I" | "-D" | "-U" => {
                let value = if raw.len() > 2 {
                    raw[2..].to_string()
                } else {
                    args.next()
                        .ok_or_else(|| format!("`{flag}` needs a value"))?
                        .clone()
                };
                match flag {
                    "-I" => opts.include_dirs.push(value.into()),
                    "-D" => {
                        let (name, body) = value.split_once('=').unwrap_or((&value, "1"));
                        opts.defines.push((name.into(), Some(body.into())));
                    }
                    _ => opts.defines.push((value, None)),
                }
            }
            "--sysroot" | "-isysroot" => {
                let value = raw
                    .strip_prefix("--sysroot=")
                    .map(str::to_string)
                    .or_else(|| args.next().cloned())
                    .ok_or("`--sysroot` needs a directory")?;
                if value.is_empty() {
                    return Err("`--sysroot` needs a nonempty directory".into());
                }
                opts.sysroot = Some(value.into());
            }
            "-isystem" => opts
                .system_include_dirs
                .push(args.next().ok_or("`-isystem` needs a directory")?.into()),
            "-mmacosx-version-min" => {
                opts.darwin_deployment = Some(
                    raw.split_once('=')
                        .map(|(_, v)| v.to_string())
                        .or_else(|| args.next().cloned())
                        .ok_or("`-mmacosx-version-min` needs a version")?,
                )
            }
            "-dM" => {
                opts.dump_macros = true;
                opts.preprocess_only = true;
            }
            "-E" => opts.preprocess_only = true,
            "-o" => opts.output = Some(args.next().ok_or("`-o` needs a file")?.clone()),
            "-O" => opts.optimization = keel_opt::OptimizationTier::Full,
            "--opt-tier" => {
                opts.optimization = match args.next().map(String::as_str) {
                    Some("none") => keel_opt::OptimizationTier::None,
                    Some("basic") => keel_opt::OptimizationTier::Basic,
                    Some("full") => keel_opt::OptimizationTier::Full,
                    _ => return Err("`--opt-tier` needs none, basic, or full".into()),
                };
            }
            "--regalloc" => {
                opts.allocation = match args.next().map(String::as_str) {
                    Some("fast") => keel_codegen::regalloc::AllocationTier::Fast,
                    Some("full") => keel_codegen::regalloc::AllocationTier::Full,
                    _ => return Err("`--regalloc` needs fast or full".into()),
                };
            }
            "--isel-tier" => {
                let tier = match args.next().map(String::as_str) {
                    Some("baseline") => keel_codegen::SelectionTier::Baseline,
                    Some("trees") => keel_codegen::SelectionTier::Trees,
                    Some("full") => keel_codegen::SelectionTier::Full,
                    _ => return Err("`--isel-tier` needs baseline, trees, or full".into()),
                };
                opts.selection = tier.options();
            }
            "-g" => opts.ir_lines = true,
            "--emit-ir" => opts.emit_ir = true,
            "--emit-object" => opts.emit_object = true,
            "--isel" => {
                let passes = args
                    .next()
                    .ok_or("`--isel` needs none, all, or trees,folds,hints")?;
                opts.selection = keel_codegen::SelectionOptions::default();
                for pass in passes.split(',') {
                    match pass {
                        "none" if passes == "none" => {}
                        "all" if passes == "all" => {
                            opts.selection = keel_codegen::SelectionOptions::ALL
                        }
                        "trees" => opts.selection.trees = true,
                        "folds" => opts.selection.folded_operands = true,
                        "hints" => opts.selection.register_hints = true,
                        _ => {
                            return Err(format!(
                                "unknown selection pass `{pass}`; expected none, all, or trees,folds,hints"
                            ));
                        }
                    }
                }
            }
            "--tune" => {
                let name = args.next().ok_or("`--tune` needs generic or apple-m5")?;
                opts.tuning = keel_codegen::tuning::TuningCpu::from_name(name)
                    .ok_or("unknown tuning CPU; expected generic or apple-m5")?;
            }
            "--target" => {
                let triple = args.next().ok_or("`--target` needs a target triple")?;
                let target = Target::from_triple(triple).ok_or_else(|| {
                    let known: Vec<_> = Target::ALL.iter().map(|t| t.triple()).collect();
                    format!("unknown target `{triple}`; known: {}", known.join(", "))
                })?;
                opts.target = Some(target);
            }
            "-p" => {
                let list = args.next().ok_or("`-p` needs a list of passes")?;
                opts.passes = Some(list.split(',').map(|s| s.trim().to_string()).collect());
            }
            _ if opts.input.is_none() => opts.input = Some(arg.clone()),
            _ => return Err(format!("unexpected argument `{arg}`\n{USAGE}")),
        }
    }
    Ok(opts)
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    match cmd.as_str() {
        "asm-metrics" => {
            let opts = parse_options(rest, &["-o"])?;
            let input = opts.input.as_ref().ok_or(USAGE)?;
            let src = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
            let (assembly, locations) =
                keel_asm::assemble_with_metrics(&src).map_err(|e| format!("{input}: {e}"))?;
            let rows: Vec<_> = locations
                .iter()
                .map(|loc| {
                    let word = u32::from_le_bytes(
                        assembly.text[loc.offset..loc.offset + 4]
                            .try_into()
                            .unwrap(),
                    );
                    format!(
                        "{{\"line\":{},\"offset\":{},\"word\":{}}}",
                        loc.line, loc.offset, word
                    )
                })
                .collect();
            let output = format!(
                "{{\"schema_version\":1,\"target\":\"aarch64-apple-darwin\",\"text_bytes\":{},\"instruction_bytes\":{},\"instructions\":[{}]}}\n",
                assembly.text.len(),
                locations.len() * 4,
                rows.join(",")
            );
            write_output(&opts, &output)
        }
        "asm" => asm::run(rest),
        "compile" => {
            let opts = parse_options(
                rest,
                &[
                    "-o",
                    "-O",
                    "-p",
                    "-g",
                    "--target",
                    "--emit-object",
                    "--isel",
                    "--opt-tier",
                    "--regalloc",
                    "--isel-tier",
                    "--tune",
                ],
            )?;
            let (input, mut module) = load(&opts)?;
            opts.optimize(&mut module)
                .map_err(|e| format!("{input}: {e}"))?;
            emit_module(&opts, &module).map_err(|e| format!("{input}: {e}"))
        }
        "cc" => {
            let opts = parse_options(
                rest,
                &[
                    "-o",
                    "-O",
                    "-p",
                    "-g",
                    "--emit-ir",
                    "-E",
                    "-dM",
                    "-I",
                    "-D",
                    "-U",
                    "--sysroot",
                    "-isysroot",
                    "-isystem",
                    "-mmacosx-version-min",
                    "--target",
                    "--emit-object",
                    "--isel",
                    "--opt-tier",
                    "--regalloc",
                    "--isel-tier",
                    "--tune",
                ],
            )?;
            let input = opts.input.clone().ok_or(USAGE)?;
            let src = input::Source::open(&input).map_err(|e| format!("{input}: {e}"))?;
            let text = src.text().map_err(|e| format!("{input}: {e}"))?;
            let target = opts.target()?;
            if opts.preprocess_only && (opts.emit_ir || opts.emit_object) {
                return Err("preprocessing (`-E`/`-dM`) and `--emit-ir`/`--emit-object` cannot be used together".into());
            }
            let preprocessed = if opts.preprocess_only || !input.ends_with(".i") {
                Some(
                    keel_c::preprocess(
                        text,
                        &input,
                        &keel_c::PreprocessOptions {
                            include_dirs: opts.include_dirs.clone(),
                            system_include_dirs: opts.system_include_dirs.clone(),
                            sysroot: opts.preprocessing_sysroot(target)?,
                            darwin_deployment: opts.darwin_deployment.clone().or_else(|| {
                                (target == Target::Aarch64AppleDarwin)
                                    .then(|| std::env::var("MACOSX_DEPLOYMENT_TARGET").ok())
                                    .flatten()
                            }),
                            defines: opts.defines.clone(),
                            target,
                            dump_macros: opts.dump_macros,
                        },
                    )
                    .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            if opts.preprocess_only {
                drop(src);
                return write_output(&opts, &preprocessed.unwrap());
            }
            let c_opts = keel_c::Options {
                debug_info: opts.ir_lines,
                target,
            };
            let mut module = {
                profiling::scope!("c-frontend");
                keel_c::compile(preprocessed.as_deref().unwrap_or(text), &input, c_opts)
                    .map_err(|e| e.to_string())?
            };
            drop(src);
            if let Err(errors) = keel_ir::verify_module(&module) {
                let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
                return Err(format!(
                    "{input}: internal error, invalid IR:\n{}",
                    lines.join("\n")
                ));
            }
            opts.optimize(&mut module)
                .map_err(|e| format!("{input}: {e}"))?;
            if opts.emit_ir && opts.emit_object {
                return Err("`--emit-ir` and `--emit-object` are mutually exclusive".into());
            }
            if opts.emit_ir {
                return write_output(&opts, &module.to_string());
            }
            emit_module(&opts, &module).map_err(|e| format!("{input}: {e}"))
        }
        "opt" => {
            let opts = parse_options(rest, &["-o", "-p", "--opt-tier", "-O"])?;
            let (input, mut module) = load(&opts)?;
            let result = match &opts.passes {
                Some(names) => {
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    keel_opt::run_passes(&mut module, &names)
                }
                None if rest.iter().any(|arg| arg == "--opt-tier" || arg == "-O") => {
                    keel_opt::optimize_with_tier(&mut module, opts.optimization)
                }
                None => keel_opt::optimize(&mut module),
            };
            result.map_err(|e| format!("{input}: {e}"))?;
            write_output(&opts, &module.to_string())
        }
        "passes" => {
            for pass in keel_opt::PASSES {
                println!("{:14} {}", pass.name(), pass.description());
            }
            println!(
                "\ndefault pipeline (-O): {}",
                keel_opt::DEFAULT_PIPELINE.join(", ")
            );
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Produce source or an object without crossing a process or filesystem boundary.
fn emit_module(opts: &Options, module: &Module) -> Result<(), String> {
    let target = opts.target()?;
    let selector = keel_codegen::tuning::TunedSelector {
        options: opts.selection,
        cpu: opts.tuning,
    };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selector,
        allocator: &opts.allocation,
    };
    if opts.emit_object {
        let output = opts
            .output
            .as_ref()
            .ok_or("`--emit-object` requires `-o <output.o>`")?;
        let program = keel_codegen::compile_program_with_pipeline(module, target, &pipeline)
            .map_err(|e| e.to_string())?;
        let object =
            keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
                .map_err(|e| e.to_string())?;
        std::fs::write(output, object).map_err(|e| format!("{output}: {e}"))
    } else {
        let source = keel_codegen::compile_with_pipeline(module, target, &pipeline)
            .map_err(|e| e.to_string())?;
        write_output(opts, &source)
    }
}

/// Reads, parses and verifies the input.
fn load(opts: &Options) -> Result<(String, Module), String> {
    profiling::scope!("parse-and-verify");
    let input = opts.input.clone().ok_or(USAGE)?;
    let src = input::Source::open(&input).map_err(|e| format!("{input}: {e}"))?;
    let text = src.text().map_err(|e| format!("{input}: {e}"))?;
    let module = if opts.ir_lines {
        keel_ir::parse_with_ir_lines(text, &input)
    } else {
        keel_ir::parse(text)
    };
    let module = module.map_err(|e| format!("{input}:{e}"))?;
    if let Err(errors) = keel_ir::verify_module(&module) {
        let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        return Err(format!("{input}: verifier errors:\n{}", lines.join("\n")));
    }
    Ok((input, module))
}

fn write_output(opts: &Options, text: &str) -> Result<(), String> {
    match &opts.output {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{path}: {e}")),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Options, String> {
        parse_options(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &[
                "-O",
                "-p",
                "--opt-tier",
                "--isel-tier",
                "--isel",
                "--regalloc",
            ],
        )
    }

    #[test]
    fn removed_mmap_flag_is_rejected_for_every_input_command() {
        for command in ["asm", "compile", "cc", "opt"] {
            let error = run(vec![command.into(), "--mmap".into()]).unwrap_err();
            assert!(error.starts_with("unknown flag `--mmap`"));
        }
    }

    #[test]
    fn direct_object_emission_matches_source_path_and_rejects_other_targets() {
        let dir = std::env::temp_dir().join(format!("keel-object-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = "func @answer() -> i64 {\nblock0:\n v0 = iconst.i64 42\n return v0\n}\n";
        let input = dir.join("answer.keel");
        let output = dir.join("answer.o");
        std::fs::write(&input, source).unwrap();
        let invoke = |target: &str, out: bool| {
            let mut args = vec![
                "compile".into(),
                input.display().to_string(),
                "--target".into(),
                target.into(),
                "--emit-object".into(),
            ];
            if out {
                args.extend(["-o".into(), output.display().to_string()]);
            }
            run(args)
        };
        assert!(
            invoke("aarch64-apple-darwin", false)
                .unwrap_err()
                .contains("requires `-o")
        );
        invoke("aarch64-apple-darwin", true).unwrap();
        let module = keel_ir::parse(source).unwrap();
        let text = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
        assert_eq!(
            std::fs::read(&output).unwrap(),
            keel_asm::assemble_object(&text).unwrap()
        );
        std::fs::remove_file(&output).unwrap();
        assert!(
            invoke("x86_64-unknown-linux-gnu", true)
                .unwrap_err()
                .contains("requires aarch64")
        );
        assert!(!output.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn assembler_cpu_selection_gates_numeric_encodings_end_to_end() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-cli-numeric-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let input = dir.join("numeric.s");
        let output = dir.join("numeric.o");
        let source = "fadd v0.8h,v1.8h,v2.8h\nsdot v0.4s,v1.16b,v2.16b\nbfdot v0.4s,v1.8h,v2.8h\n";
        std::fs::write(&input, source).unwrap();
        let invoke = |flags: &[&str]| {
            let mut args = vec![
                "asm".to_owned(),
                input.display().to_string(),
                "-o".into(),
                output.display().to_string(),
            ];
            args.extend(flags.iter().map(|s| s.to_string()));
            run(args)
        };
        assert!(invoke(&[]).unwrap_err().contains("FEAT_FP16"));
        assert!(!output.exists());
        invoke(&["--cpu", "apple-m5"]).unwrap();
        assert_eq!(
            std::fs::read(&output).unwrap(),
            keel_asm::assemble_object_with_features(source, keel_asm::TargetFeatures::apple_m5())
                .unwrap()
        );
        assert!(
            invoke(&["--cpu", "apple-m5", "--cpu", "baseline"])
                .unwrap_err()
                .contains("FEAT_FP16")
        );
        assert!(
            invoke(&["--cpu", "unknown"])
                .unwrap_err()
                .contains("unknown assembler CPU")
        );
        assert!(invoke(&["--cpu"]).unwrap_err().contains("needs"));
        std::fs::remove_dir_all(&dir).unwrap();
        // CPU selection belongs to assembly; it must not silently change codegen.
        assert!(
            run(vec!["compile".into(), "--cpu".into(), "apple-m5".into()])
                .unwrap_err()
                .contains("unknown flag")
        );
    }

    #[test]
    fn stage_flags_are_independent_and_last_flag_wins() {
        let opts = options(&[
            "-O",
            "--opt-tier",
            "basic",
            "--isel-tier",
            "full",
            "--regalloc",
            "fast",
        ])
        .unwrap();
        assert_eq!(opts.optimization, keel_opt::OptimizationTier::Basic);
        assert_eq!(opts.selection, keel_codegen::SelectionOptions::ALL);
        assert_eq!(
            opts.allocation,
            keel_codegen::regalloc::AllocationTier::Fast
        );
        let opts = options(&[
            "--opt-tier",
            "none",
            "-O",
            "--isel-tier",
            "full",
            "--isel",
            "trees",
        ])
        .unwrap();
        assert_eq!(opts.optimization, keel_opt::OptimizationTier::Full);
        assert_eq!(opts.selection, keel_codegen::SelectionTier::Trees.options());
    }

    #[test]
    fn default_tiers_and_invalid_values() {
        let opts = options(&[]).unwrap();
        assert_eq!(opts.optimization, keel_opt::OptimizationTier::None);
        assert_eq!(opts.selection, keel_codegen::SelectionOptions::default());
        assert_eq!(
            opts.allocation,
            keel_codegen::regalloc::AllocationTier::Full
        );
        for flag in ["--opt-tier", "--isel-tier", "--regalloc"] {
            assert!(options(&[flag]).is_err());
            assert!(options(&[flag, "invalid"]).is_err());
        }
    }

    #[test]
    fn explicit_pipeline_overrides_tiers_in_either_order() {
        let source = "func @f() -> i64 {\nblock0:\n    v0 = iconst.i64 20\n    v1 = iconst.i64 22\n    v2 = iadd v0, v1\n    return v2\n}";
        for args in [["-p", "dce", "-O"], ["-O", "-p", "dce"]] {
            let opts = options(&args).unwrap();
            let mut module = keel_ir::parse(source).unwrap();
            opts.optimize(&mut module).unwrap();
            assert!(
                module.to_string().contains("iadd"),
                "only dce should run; Full would fold the sum"
            );
        }
        let opts = options(&["-p", "missing"]).unwrap();
        let mut module = keel_ir::parse(source).unwrap();
        assert!(matches!(
            opts.optimize(&mut module),
            Err(keel_opt::OptError::UnknownPass(_))
        ));
        assert!(options(&["-p"]).is_err());
    }
}

#[cfg(test)]
mod tuning_tests {
    use super::*;

    #[test]
    fn tuning_is_explicit_independent_and_diagnoses_incompatible_targets() {
        let allowed = ["--tune", "--target", "-O", "--isel-tier"];
        let parse = |args: &[&str]| {
            parse_options(
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                &allowed,
            )
        };
        for args in [
            vec!["--tune", "apple-m5", "-O"],
            vec!["-O", "--tune", "apple-m5"],
        ] {
            let opts = parse(&args).unwrap();
            assert_eq!(opts.tuning, keel_codegen::tuning::TuningCpu::AppleM5);
            assert!(
                !opts.selection.trees,
                "tuning must not change selection effort"
            );
        }
        assert!(parse(&["--tune", "unrecognized"]).is_err());
        assert!(parse(&["--tune"]).is_err());
        assert!(
            parse(&["--tune", "apple-m5", "--target", "x86_64-unknown-linux-gnu"])
                .unwrap()
                .target()
                .is_err()
        );
        assert_eq!(
            parse(&["--tune", "generic", "--target", "x86_64-unknown-linux-gnu"])
                .unwrap()
                .target()
                .unwrap(),
            Target::X86_64LinuxGnu
        );
    }
}

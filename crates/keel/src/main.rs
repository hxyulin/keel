//! The keel driver:
//!
//! ```text
//! keel compile [--code-model pic|small|kernel] [-O0|-O1|-O2|-O3|-Os] [-g] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--cpu-level <v1|v2|v3>] [--target-features <features>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.keel> [-o <output.s>]   IR to assembly
//! keel cc [-O0|-O1|-O2|-O3|-Os] [-g] [--code-model pic|small|kernel] [--link-intent executable|shared] [-fsemantic-interposition|-fno-semantic-interposition] [-E [-dM]|--emit-ir] [-I dir] [-D name[=value]] [-U name] [--sysroot dir] [-isystem dir] [-mmacosx-version-min=version] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--cpu-level <v1|v2|v3>] [--target-features <features>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.c|input.i> [-o <output>]  C to assembly
//! keel opt [--opt-tier <tier>] [--target <triple>] [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
//! keel asm [--x86-syntax <intel|att>] [--target <triple>] [--cpu <baseline|apple-m5>] [--cpu-level <v1|v2|v3>] [--target-features <features>] <input.s> -o <output.o>                 assembly to native object
//! keel asm [--x86-syntax <intel|att>] [--target <triple>] [--cpu <baseline|apple-m5>] [--cpu-level <v1|v2|v3>] [--target-features <features>] <input.s>... --out-dir <directory>      batch assembly
//! keel asm-metrics <input.s> [-o <metrics.json>]     exact instruction locations and sizes
//! keel passes                                      list the passes
//! ```
//!
//! `-O` enables the full IR pipeline and all target selection passes; `opt` runs
//! the given passes (the default pipeline without `-p`) and prints the IR.
//! Assembly has debug info if the input names source files (`file`
//! lines); `-g` gives an input without them its own lines instead, for
//! stepping through the IR in a debugger.
//! `cc` preprocesses raw C (or accepts `.i` files), then compiles to assembly or,
//! with `--emit-ir`, to keel IR; `-E` stops after preprocessing; `-g` gives it debug info, with every
//! local in memory where a debugger can see it (link with `clang -g`).
//! `compile`/`cc --emit-object -o output.o` compose structured codegen and
//! object emission in-process for AArch64 Mach-O and AArch64/x86-64 ELF.
//! Text output goes to stdout without `-o`; `asm` requires `-o` or `--out-dir`
//! and supports AArch64 Mach-O, AArch64 Linux ELF and x86-64 ELF objects.
//! `--target` picks the machine for codegen,
//! `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu` or
//! `x86_64-unknown-linux-gnu`; the default is
//! the one keel runs on.

use keel_codegen::Target;
use keel_ir::Module;
use std::process::ExitCode;
mod archive;
mod asm;
mod disasm;
mod input;
mod link;
mod optimization;
mod unwind_options;
mod wasm;
mod x86_options;

const USAGE: &str = "usage:
  keel compile [--code-model pic|small|kernel] [-O0|-O1|-O2|-O3|-Os] [-g] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--cpu-level <v1|v2|v3>] [--target-features <features>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.keel> [-o <output.s>]
  keel cc [-O0|-O1|-O2|-O3|-Os] [-g] [--code-model pic|small|kernel] [--link-intent executable|shared] [-fsemantic-interposition|-fno-semantic-interposition] [-E [-dM]|--emit-ir] [-I dir] [-D name[=value]] [-U name] [--sysroot dir] [-isystem dir] [-mmacosx-version-min=version] [-p <pass,pass,..>] [--opt-tier <tier>] [--regalloc <tier>] [--tune <cpu>] [--cpu-level <v1|v2|v3>] [--target-features <features>] [--isel-tier <tier>] [--isel <passes>] [--target <triple>] <input.c|input.i> [-o <output>]
  keel opt [--opt-tier <tier>] [--target <triple>] [-p <pass,pass,..>] <input.keel> [-o <output.keel>]
  keel compile|cc|opt [--verify-each | --no-verify-each] [--opt-report <new-report.json>] <input> [-o <output>]
  keel compile|cc [--emit-object] <input> -o <output.o>  (AArch64 Mach-O or AArch64/x86-64 ELF)
  keel asm [--x86-syntax <intel|att>] [--target <triple>] [--cpu <baseline|apple-m5>] [--cpu-level <v1|v2|v3>] [--target-features <features>] <input.s> -o <output.o>
  keel asm [--x86-syntax <intel|att>] [--target <triple>] [--cpu <baseline|apple-m5>] [--cpu-level <v1|v2|v3>] [--target-features <features>] <input.s>... --out-dir <directory>
  keel asm-metrics <input.s> [-o <metrics.json>]
  keel disasm <input.o> [-o <output.txt>]  (AArch64 or x86-64 Mach-O/ELF)
  keel link [--system] [--target <triple>] [-e entry] [-L DIR] [--rpath PATH] [--sysroot SDK] [-l NAME] [--dead-strip] [--require-export SYMBOL] [--debug-map DIR] [--dsym PATH] <input.o|input.a|input.so>... -o executable
  keel archive [--target <triple>] -o <output.a> <member.o>...
  keel cc|compile --target wasm32-unknown-unknown --emit-wasm [--export name]... <input> -o <output.wasm>
  Wasm memory: --wasm-memory-pages 1 --wasm-data-floor 1024 --wasm-stack-bottom 32768 --wasm-stack-top 65536
  Wasm: --wasm-opt basic opts into the bounded profile; no native optimization/debug flags.
  keel passes

  --target also accepts aarch64-unknown-linux-gnu (Linux AAPCS64/ELF).
  -O0 disables optional IR optimization; -O1 favors compile time.
  -O2 balances optimization effort; -O3 uses the full pipeline; -O retains -O3 behavior.
  -Os favors size: less optional inlining/expansion and smaller x86 layout/frames.
  --isel / --isel-tier and --regalloc override preset stages in either order.
  Repeated explicit stage options use the last value.
  --opt-tier none|fast|basic|balanced|full|size controls IR optimization; -p overrides its pipeline in either order.
  fast runs bounded local/CFG cleanup and small single-stream sum reductions; allocator/selection defaults are independent.
  -fno-asynchronous-unwind-tables omits optional compiler-generated CFI.
  -funwind-tables requests tables independently; disable both to suppress both requests.
  Unwind flags do not change stack frames, debug info or explicit user assembly CFI.
  Repeated verification defaults on with debug assertions or feature verify-each.
  --verify-each / --no-verify-each override this; contradictory flags are rejected.
  Initial input verification and canonical restoration always run.
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

/// Intended final use of native C compilation output, independent of its format.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CLinkIntent {
    Executable,
    Shared,
}

/// Command-line flags shared by the subcommands.
#[derive(Default)]
struct Options {
    input: Option<String>,
    output: Option<String>,
    opt_report: Option<String>,
    optimization: keel_opt::OptimizationTier,
    optimization_level: Option<optimization::Level>,
    verification: keel_opt::VerificationPolicy,
    allocation: keel_codegen::regalloc::AllocationTier,
    selection: keel_codegen::SelectionOptions,
    #[cfg(feature = "experimental-regalloc")]
    remat: u8,
    tuning: keel_codegen::tuning::TuningCpu,
    x86_features: keel_asm::x86::Features,
    x86_options: crate::x86_options::X86Options,
    ir_lines: bool,
    emit_ir: bool,
    function_binding: Option<keel_c::FunctionBinding>,
    link_intent: Option<CLinkIntent>,
    x86_code_model: Option<keel_codegen::x86_64::CodeModel>,
    unwind: unwind_options::UnwindOptions,
    emit_object: bool,
    emit_wasm: bool,
    wasm: wasm::Options,
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
    fn preprocessing_defines(&self) -> Vec<(String, Option<String>)> {
        let mut defines = Vec::new();
        if self.optimization != keel_opt::OptimizationTier::None {
            defines.push(("__OPTIMIZE__".into(), Some("1".into())));
        }
        if self.optimization == keel_opt::OptimizationTier::Size {
            defines.push(("__OPTIMIZE_SIZE__".into(), Some("1".into())));
        }
        // Explicit -D/-U overrides compiler-provided optimization macros.
        defines.extend(self.defines.iter().cloned());
        defines
    }

    fn c_function_binding(&self) -> keel_c::FunctionBinding {
        self.function_binding.unwrap_or_else(|| {
            if self.link_intent == Some(CLinkIntent::Executable) {
                keel_c::FunctionBinding::NoSemanticInterposition
            } else {
                keel_c::FunctionBinding::SemanticInterposition
            }
        })
    }

    fn emission_options(&self) -> keel_codegen::EmissionOptions {
        let mut emission = keel_codegen::EmissionOptions::default();
        if self.optimization_level == Some(optimization::Level::Size) {
            emission.goal = keel_codegen::CodegenGoal::Size;
        }
        emission.unwind_tables = self.unwind.policy();
        emission.x86_code_model = self.x86_code_model;
        emission
    }

    fn optimizer_options(&self) -> keel_opt::OptimizeOptions {
        keel_opt::OptimizeOptions {
            verification: self.verification,
        }
    }

    /// Explicit pipelines override tier presets, independently of flag order.
    fn optimize(&self, module: &mut Module) -> Result<(), String> {
        let names: Vec<_> = self
            .passes
            .as_ref()
            .map(|names| names.iter().map(String::as_str).collect())
            .unwrap_or_else(|| self.optimization.pipeline().to_vec());
        self.run_optimization(module, &names, self.vector_target())
    }

    fn optimization_context(
        &self,
        target: keel_opt::VectorTarget,
    ) -> Result<keel_opt::OptimizationContext, String> {
        self.reject_unavailable_target()?;
        // Target-free `opt` also works on hosts with no code generation target.
        // Its legacy Portable policy needs neither host resolution nor features.
        if target == keel_opt::VectorTarget::Portable
            && self.target.is_none()
            && !self.x86_options.supplied()
            && self.x86_features.is_empty()
        {
            return Ok(target.into());
        }
        let selector = keel_codegen::tuning::TunedSelector {
            options: self.selection,
            cpu: self.tuning,
        };
        let selector = keel_codegen::x86_features::FeatureSelector {
            inner: &selector,
            features: self.x86_features,
        };
        let capabilities =
            keel_codegen::x86_features::vector_capabilities(self.target()?, &selector)
                .map_err(str::to_string)?;
        Ok(keel_opt::OptimizationContext {
            target,
            capabilities,
        })
    }

    fn run_optimization(
        &self,
        module: &mut Module,
        names: &[&str],
        target: keel_opt::VectorTarget,
    ) -> Result<(), String> {
        let context = self.optimization_context(target)?;
        let Some(path) = &self.opt_report else {
            return keel_opt::run_passes_with_context_and_options(
                module,
                names,
                context,
                self.optimizer_options(),
            )
            .map_err(|e| e.to_string());
        };
        let mut report = keel_opt::OptimizationReport::default();
        keel_opt::run_passes_with_context_and_report_and_options(
            module,
            names,
            context,
            &mut report,
            self.optimizer_options(),
        )
        .map_err(|e| e.to_string())?;
        // Never overwrite an input, output, previous report, symlink, or device.
        // Path conflicts are checked before compilation; create_new also guards races.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("optimization report {path}: {e}"))?;
        file.write_all(report.to_json().as_bytes())
            .map_err(|e| format!("optimization report {path}: {e}"))
    }

    fn vector_target(&self) -> keel_opt::VectorTarget {
        match self.target.or_else(Target::host) {
            Some(Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu) => {
                keel_opt::VectorTarget::Aarch64Neon
            }
            Some(Target::X86_64LinuxGnu) => keel_opt::VectorTarget::X86Sse2,
            Some(Target::Wasm32UnknownUnknown) | None => keel_opt::VectorTarget::Portable,
        }
    }

    fn reject_unavailable_target(&self) -> Result<(), String> {
        if self.target == Some(Target::Wasm32UnknownUnknown) {
            return Err("unsupported target wasm32-unknown-unknown: public C, optimization and native output pipelines are unavailable".into());
        }
        Ok(())
    }

    fn target(&self) -> Result<Target, String> {
        self.reject_unavailable_target()?;
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
    let mut optimize_preset = None;
    let mut explicit_allocation = false;
    let mut explicit_selection = false;
    let mut supplied = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let raw = arg.as_str();
        let flag = if raw.starts_with("--sysroot=") {
            "--sysroot"
        } else if raw.starts_with("-mmacosx-version-min=") {
            "-mmacosx-version-min"
        } else if optimization::Level::parse(raw).is_some() {
            "-O"
        } else if raw.starts_with("--code-model=") || raw.starts_with("-mcmodel=") {
            "--code-model"
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
        if flag.starts_with('-') {
            supplied.push(flag);
        }
        match flag {
            "--emit-wasm" => opts.emit_wasm = true,
            "--wasm-opt" => {
                if args.next().map(String::as_str) != Some("basic") {
                    return Err("`--wasm-opt` needs basic".into());
                }
                opts.wasm.basic = true;
            }
            "--export" => opts.wasm.exports.push(
                args.next()
                    .ok_or("`--export` needs a function name")?
                    .clone(),
            ),
            "--wasm-memory-pages"
            | "--wasm-data-floor"
            | "--wasm-stack-bottom"
            | "--wasm-stack-top" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("`{flag}` needs a decimal u32"))?;
                opts.wasm.set_number(flag, value)?;
            }
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
            "--verify-each" | "--no-verify-each" => {
                let policy = if flag == "--verify-each" {
                    keel_opt::VerificationPolicy::Enabled
                } else {
                    keel_opt::VerificationPolicy::Disabled
                };
                if opts.verification != keel_opt::VerificationPolicy::Default
                    && opts.verification != policy
                {
                    return Err(
                        "`--verify-each` and `--no-verify-each` are mutually exclusive".into(),
                    );
                }
                opts.verification = policy;
            }
            "--opt-report" => {
                let path = args.next().ok_or("`--opt-report` needs a file")?;
                if path.is_empty() || path == "-" {
                    return Err("`--opt-report` needs a nonempty file path (not stdout)".into());
                }
                if opts.opt_report.replace(path.clone()).is_some() {
                    return Err("`--opt-report` may only be specified once".into());
                }
            }
            "-O" => {
                let level = optimization::Level::parse(raw).expect("normalized optimization flag");
                opts.optimization = level.optimization();
                opts.optimization_level = Some(level);
                optimize_preset = Some(level);
            }
            "--opt-tier" => {
                opts.optimization = match args.next().map(String::as_str) {
                    Some("none") => keel_opt::OptimizationTier::None,
                    Some("fast") => keel_opt::OptimizationTier::Fast,
                    Some("basic") => keel_opt::OptimizationTier::Basic,
                    Some("full") => keel_opt::OptimizationTier::Full,
                    Some("balanced") => keel_opt::OptimizationTier::Balanced,
                    Some("size") => keel_opt::OptimizationTier::Size,
                    _ => {
                        return Err(
                            "`--opt-tier` needs none, fast, basic, balanced, full, or size".into(),
                        );
                    }
                };
            }
            "--remat" => {
                #[cfg(not(feature = "experimental-regalloc"))]
                return Err("--remat requires experimental-regalloc".into());
                #[cfg(feature = "experimental-regalloc")]
                {
                    opts.remat = match args.next().map(String::as_str) {
                        Some("constants") => 1,
                        Some("stack") => 2,
                        Some("all") => 3,
                        Some("pressure") => 7,
                        Some("null") => 8,
                        _ => {
                            return Err(
                                "--remat needs constants, stack, all, pressure, or null".into()
                            );
                        }
                    };
                }
            }
            "--regalloc" => {
                explicit_allocation = true;
                opts.allocation = match args.next().map(String::as_str) {
                    Some("fast") => keel_codegen::regalloc::AllocationTier::Fast,
                    #[cfg(feature = "experimental-regalloc")]
                    Some("local") => keel_codegen::regalloc::AllocationTier::Local,
                    #[cfg(feature = "experimental-regalloc")]
                    Some("split") => keel_codegen::regalloc::AllocationTier::Split,
                    #[cfg(feature = "experimental-regalloc")]
                    Some("weighted") => keel_codegen::regalloc::AllocationTier::Weighted,
                    #[cfg(feature = "experimental-regalloc")]
                    Some("full-affinity") => keel_codegen::regalloc::AllocationTier::FullAffinity,
                    Some("full") => keel_codegen::regalloc::AllocationTier::Full,
                    _ => return Err("`--regalloc` needs fast or full (local/split/weighted/full-affinity require experimental-regalloc)".into()),
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
                explicit_selection = true;
            }
            "-funwind-tables"
            | "-fno-unwind-tables"
            | "-fasynchronous-unwind-tables"
            | "-fno-asynchronous-unwind-tables" => opts.unwind.set(flag),
            "-g" => opts.ir_lines = true,
            "--emit-ir" => opts.emit_ir = true,
            "--code-model" => {
                if opts.x86_code_model.is_some() {
                    return Err("`--code-model` may only be supplied once".into());
                }
                let name = raw
                    .split_once('=')
                    .map(|(_, name)| name)
                    .or_else(|| args.next().map(String::as_str));
                opts.x86_code_model = Some(match name {
                    Some("pic") => keel_codegen::x86_64::CodeModel::PositionIndependent,
                    Some("small") => keel_codegen::x86_64::CodeModel::Small,
                    Some("kernel") => keel_codegen::x86_64::CodeModel::Kernel,
                    Some("medium" | "large") => return Err("medium and large code models are not implemented; use pic, small or kernel".into()),
                    _ => return Err("`--code-model` needs pic, small or kernel".into()),
                });
            }
            "--link-intent" => {
                opts.link_intent = Some(match args.next().map(String::as_str) {
                    Some("executable") => CLinkIntent::Executable,
                    Some("shared") => CLinkIntent::Shared,
                    _ => return Err("`--link-intent` needs executable or shared".into()),
                });
            }
            "-fsemantic-interposition" => {
                opts.function_binding = Some(keel_c::FunctionBinding::SemanticInterposition)
            }
            "-fno-semantic-interposition" => {
                opts.function_binding = Some(keel_c::FunctionBinding::NoSemanticInterposition)
            }
            "--emit-object" => opts.emit_object = true,
            "--isel" => {
                explicit_selection = true;
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
            "--cpu-level" => {
                opts.x86_options
                    .set_level(args.next().ok_or("--cpu-level needs v1, v2 or v3")?)?;
            }
            "--target-features" => {
                opts.x86_options.push_features(
                    args.next()
                        .ok_or("--target-features needs a feature list or none")?,
                );
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
    opts.x86_features = opts.x86_options.resolve()?;
    if opts.x86_options.supplied() && opts.target()? != Target::X86_64LinuxGnu {
        return Err("x86 target features require x86_64-unknown-linux-gnu".into());
    }
    if opts.x86_code_model.is_some() && opts.target()? != Target::X86_64LinuxGnu {
        return Err("x86 code model requires x86_64-unknown-linux-gnu".into());
    }
    if matches!(
        opts.x86_code_model,
        Some(keel_codegen::x86_64::CodeModel::Small | keel_codegen::x86_64::CodeModel::Kernel)
    ) && opts.link_intent == Some(CLinkIntent::Shared)
    {
        return Err("static code model requires a statically linked executable image".into());
    }
    wasm::validate_options(&opts, &supplied)?;
    if !opts.emit_wasm {
        opts.reject_unavailable_target()?;
    }
    // Explicit selectors override the convenience preset regardless of order.
    // IR tier changes and explicit pipelines do not reset selection.
    if let Some(level) = optimize_preset {
        if !explicit_selection {
            opts.selection = level.selection().options();
        }
        if !explicit_allocation {
            opts.allocation = level.allocation();
        }
    }
    #[cfg(feature = "experimental-regalloc")]
    if opts.remat != 0 && opts.allocation == keel_codegen::regalloc::AllocationTier::FullAffinity {
        return Err("--remat is not proven with --regalloc full-affinity".into());
    }
    #[cfg(feature = "experimental-regalloc")]
    if opts.remat & (4 | 8) != 0
        && !matches!(
            opts.allocation,
            keel_codegen::regalloc::AllocationTier::Fast
                | keel_codegen::regalloc::AllocationTier::Full
        )
    {
        return Err(if opts.remat & 8 != 0 {
            "--remat null requires --regalloc fast or full"
        } else {
            "--remat pressure requires --regalloc fast or full"
        }
        .into());
    }
    if opts.emit_ir && opts.emit_object {
        return Err("`--emit-ir` and `--emit-object` are mutually exclusive".into());
    }
    if let Some(report) = &opts.opt_report {
        if opts.preprocess_only {
            return Err("`--opt-report` cannot be used with preprocessing only".into());
        }
        let path = std::path::Path::new(report);
        if path.symlink_metadata().is_ok() {
            return Err(format!("optimization report {report}: file already exists"));
        }
        let resolved_report = report_destination(path)?;
        for other in [&opts.input, &opts.output].into_iter().flatten() {
            if report_destination(std::path::Path::new(other))? == resolved_report {
                return Err("optimization report path conflicts with input or output".into());
            }
        }
    }
    Ok(opts)
}

fn report_destination(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let mut path = path.to_path_buf();
    // canonicalize handles existing files and parent aliases. Resolve a dangling
    // final symlink as well, so a later -o cannot overwrite the newly made report.
    for _ in 0..40 {
        if let Ok(resolved) = path.canonicalize() {
            return Ok(resolved);
        }
        if let Ok(link) = std::fs::read_link(&path) {
            path = if link.is_absolute() {
                link
            } else {
                path.parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(link)
            };
            continue;
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let parent = parent
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .ok_or_else(|| format!("{}: expected a file path", path.display()))?;
        return Ok(parent.join(name));
    }
    Err(format!("{}: too many symbolic links", path.display()))
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
                    let bytes = if loc.section == 0 {
                        &assembly.text
                    } else {
                        assembly.sections[loc.section - 1].data()
                    };
                    let word =
                        u32::from_le_bytes(bytes[loc.offset..loc.offset + 4].try_into().unwrap());
                    let section = if loc.section == 0 {
                        String::new()
                    } else {
                        format!(",\"section\":{}", loc.section)
                    };
                    format!(
                        "{{\"line\":{},\"offset\":{},\"word\":{}{section}}}",
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
        "disasm" => disasm::run(rest),
        "link" => link::run(rest),
        "archive" => archive::run(rest),
        "compile" => {
            let opts = parse_options(
                rest,
                &[
                    "-o",
                    "-O",
                    "-p",
                    "-g",
                    "-funwind-tables",
                    "-fno-unwind-tables",
                    "-fasynchronous-unwind-tables",
                    "-fno-asynchronous-unwind-tables",
                    "--target",
                    "--code-model",
                    "--emit-object",
                    "--emit-wasm",
                    "--wasm-opt",
                    "--export",
                    "--wasm-memory-pages",
                    "--wasm-data-floor",
                    "--wasm-stack-bottom",
                    "--wasm-stack-top",
                    "--isel",
                    "--opt-report",
                    "--verify-each",
                    "--no-verify-each",
                    "--opt-tier",
                    "--regalloc",
                    "--remat",
                    "--isel-tier",
                    "--tune",
                    "--target-features",
                    "--cpu-level",
                ],
            )?;
            if opts.emit_wasm {
                return wasm::compile_ir(&opts);
            }
            let (input, mut module) = load(&opts)?;
            opts.optimize(&mut module)
                .map_err(|e| format!("{input}: {e}"))?;
            emit_module(&opts, module).map_err(|e| format!("{input}: {e}"))
        }
        "cc" => {
            let opts = parse_options(
                rest,
                &[
                    "-o",
                    "-O",
                    "-p",
                    "-g",
                    "-funwind-tables",
                    "-fno-unwind-tables",
                    "-fasynchronous-unwind-tables",
                    "-fno-asynchronous-unwind-tables",
                    "--emit-ir",
                    "-fsemantic-interposition",
                    "-fno-semantic-interposition",
                    "--link-intent",
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
                    "--code-model",
                    "--emit-object",
                    "--emit-wasm",
                    "--wasm-opt",
                    "--export",
                    "--wasm-memory-pages",
                    "--wasm-data-floor",
                    "--wasm-stack-bottom",
                    "--wasm-stack-top",
                    "--isel",
                    "--opt-report",
                    "--verify-each",
                    "--no-verify-each",
                    "--opt-tier",
                    "--regalloc",
                    "--remat",
                    "--isel-tier",
                    "--tune",
                    "--target-features",
                    "--cpu-level",
                ],
            )?;
            if (opts.function_binding.is_some() || opts.link_intent.is_some())
                && (opts.emit_wasm || opts.target == Some(Target::Wasm32UnknownUnknown))
            {
                return Err("C function binding modes require a native target".into());
            }
            if opts.emit_wasm {
                return wasm::compile_c(&opts);
            }
            let input = opts.input.clone().ok_or(USAGE)?;
            let src = input::Source::open(&input).map_err(|e| format!("{input}: {e}"))?;
            let text = src.text().map_err(|e| format!("{input}: {e}"))?;
            let target = opts.target()?;
            if opts.preprocess_only && (opts.emit_ir || opts.emit_object) {
                return Err("preprocessing (`-E`/`-dM`) and `--emit-ir`/`--emit-object` cannot be used together".into());
            }
            let preprocessed = if opts.preprocess_only || !input.ends_with(".i") {
                profiling::scope!("c-preprocessing");
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
                            defines: opts.preprocessing_defines(),
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
                keel_c::compile_with_binding(
                    preprocessed.as_deref().unwrap_or(text),
                    &input,
                    c_opts,
                    opts.c_function_binding(),
                )
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
            if opts.emit_ir {
                return write_output(&opts, &module.to_string());
            }
            emit_module(&opts, module).map_err(|e| format!("{input}: {e}"))
        }
        "opt" => {
            let opts = parse_options(
                rest,
                &[
                    "-o",
                    "-p",
                    "--opt-tier",
                    "-O",
                    "--target",
                    "--cpu-level",
                    "--target-features",
                    "--opt-report",
                    "--verify-each",
                    "--no-verify-each",
                ],
            )?;
            let (input, mut module) = load(&opts)?;
            let target = if opts.target.is_some() {
                opts.vector_target()
            } else {
                keel_opt::VectorTarget::Portable
            };
            let result = match &opts.passes {
                Some(names) => {
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    opts.run_optimization(&mut module, &names, target)
                }
                None if opts.optimization_level.is_some()
                    || rest.iter().any(|arg| arg == "--opt-tier") =>
                {
                    opts.run_optimization(&mut module, opts.optimization.pipeline(), target)
                }
                None => opts.run_optimization(&mut module, keel_opt::DEFAULT_PIPELINE, target),
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
/// The module is released as soon as code generation no longer needs it.
fn emit_module(opts: &Options, module: Module) -> Result<(), String> {
    let target = opts.target()?;
    let selector = keel_codegen::tuning::TunedSelector {
        options: opts.selection,
        cpu: opts.tuning,
    };
    let feature_selector = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features: opts.x86_features,
    };
    #[cfg(feature = "experimental-regalloc")]
    let remat = keel_codegen::regalloc::RematerializingAllocator {
        base: &opts.allocation,
        prefer_spilling: opts.remat & 4 != 0,
        constants: opts.remat & 1 != 0,
        stack_addresses: opts.remat & 2 != 0,
        null_pointers: opts.remat & 8 != 0,
    };
    let allocator: &dyn keel_codegen::regalloc::RegisterAllocator = &opts.allocation;
    #[cfg(feature = "experimental-regalloc")]
    let allocator: &dyn keel_codegen::regalloc::RegisterAllocator =
        if opts.remat != 0 { &remat } else { allocator };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &feature_selector,
        allocator,
    };
    if opts.emit_object {
        let output = opts
            .output
            .as_ref()
            .ok_or("`--emit-object` requires `-o <output.o>`")?;
        // Assemble each function's program as codegen produces it, so the
        // whole module's structured program is never resident at once.
        let mut aarch64 = keel_asm::ProgramAssembler::new(keel_asm::TargetFeatures::default());
        let mut x86 = keel_asm::x86::ProgramAssembler::new(opts.x86_features);
        let mut arm_elf = keel_asm::ElfProgramAssembler::new(keel_asm::TargetFeatures::default());
        let mut arm_elf_sink = |program| {
            profiling::scope!("integrated-assembly");
            arm_elf.push(program)
        };
        let mut aarch64_sink = |program| {
            profiling::scope!("integrated-assembly");
            aarch64.push(program)
        };
        let mut x86_sink = |program| {
            profiling::scope!("integrated-assembly");
            x86.push_with_binary_inputs(program, &mut input::binary_input)
        };
        let sink: &mut dyn FnMut(_) = match target {
            Target::Wasm32UnknownUnknown => return Err(
                "unsupported target wasm32-unknown-unknown: native object emission is unavailable"
                    .into(),
            ),
            Target::Aarch64LinuxGnu => &mut arm_elf_sink,
            Target::Aarch64AppleDarwin => &mut aarch64_sink,
            Target::X86_64LinuxGnu => &mut x86_sink,
        };
        let emission = opts.emission_options();
        keel_codegen::compile_program_chunks_with_pipeline_and_emission(
            &module, target, &pipeline, emission, sink,
        )
        .map_err(|e| e.to_string())?;
        drop(module);
        let object = {
            profiling::scope!("object-finalization");
            match target {
            Target::Wasm32UnknownUnknown => return Err(
                "unsupported target wasm32-unknown-unknown: native object emission is unavailable"
                    .into(),
            ),
            Target::Aarch64LinuxGnu => arm_elf.finish(),
            Target::Aarch64AppleDarwin => aarch64.finish(),
            Target::X86_64LinuxGnu => x86.finish(),
        }
        .map_err(|e| e.to_string())?
        };
        profiling::scope!("object-write");
        std::fs::write(output, object).map_err(|e| format!("{output}: {e}"))
    } else {
        let emission = opts.emission_options();
        let source =
            keel_codegen::compile_with_pipeline_and_emission(&module, target, &pipeline, emission)
                .map_err(|e| e.to_string())?;
        // The process exits once the text is written, and the system takes
        // the module's memory back faster than freeing its every allocation.
        std::mem::forget(module);
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
                "--verify-each",
                "--no-verify-each",
            ],
        )
    }

    #[test]
    fn verification_flags_override_build_default_independently_of_tiers() {
        use keel_opt::VerificationPolicy::{Default, Disabled, Enabled};
        assert_eq!(options(&[]).unwrap().verification, Default);
        for (flag, expected) in [("--verify-each", Enabled), ("--no-verify-each", Disabled)] {
            for flags in [
                vec![flag, "-O"],
                vec!["--opt-tier", "none", flag],
                vec![flag, flag],
            ] {
                let opts = options(&flags).unwrap();
                assert_eq!(opts.verification, expected);
                assert_eq!(
                    opts.optimizer_options().verification.enabled(),
                    expected == Enabled
                );
            }
        }
        for flags in [
            ["--verify-each", "--no-verify-each"],
            ["--no-verify-each", "--verify-each"],
        ] {
            assert!(
                options(&flags)
                    .err()
                    .unwrap()
                    .contains("mutually exclusive")
            );
        }
    }

    #[test]
    fn removed_mmap_flag_is_rejected_for_every_input_command() {
        for command in ["asm", "compile", "cc", "opt"] {
            let error = run(vec![command.into(), "--mmap".into()]).unwrap_err();
            assert!(error.starts_with("unknown flag `--mmap`"));
        }
    }

    #[test]
    fn direct_object_emission_matches_source_path_for_both_targets() {
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
        invoke("x86_64-unknown-linux-gnu", true).unwrap();
        let text = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
        assert_eq!(
            std::fs::read(&output).unwrap(),
            keel_asm::x86::assemble_object(&text).unwrap()
        );
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
    fn full_affinity_is_explicit_and_feature_gated() {
        #[cfg(feature = "experimental-regalloc")]
        {
            let opts = options(&["--regalloc", "full-affinity"]).unwrap();
            assert_eq!(
                opts.allocation,
                keel_codegen::regalloc::AllocationTier::FullAffinity
            );
            for remat in ["constants", "stack", "all", "pressure", "null"] {
                assert!(options(&["--regalloc", "full-affinity", "--remat", remat]).is_err());
            }
        }
        #[cfg(not(feature = "experimental-regalloc"))]
        assert!(options(&["--regalloc", "full-affinity"]).is_err());
    }

    #[test]
    fn level_presets_keep_explicit_backend_overrides_in_either_order() {
        use keel_codegen::{SelectionOptions, regalloc::AllocationTier};
        for level in ["-O0", "-O1", "-O2", "-O3", "-Os"] {
            for flags in [
                vec![level, "--isel-tier", "baseline", "--regalloc", "full"],
                vec!["--isel-tier", "baseline", "--regalloc", "full", level],
            ] {
                let opts = options(&flags).unwrap();
                assert_eq!(opts.selection, SelectionOptions::default(), "{flags:?}");
                assert_eq!(opts.allocation, AllocationTier::Full, "{flags:?}");
            }
        }
        let opts = options(&["-O1"]).unwrap();
        assert_eq!(opts.allocation, AllocationTier::Fast);
        assert_eq!(
            options(&["-O1", "-O0"]).unwrap().allocation,
            options(&[]).unwrap().allocation
        );
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
    fn optimization_preset_selection_and_explicit_overrides() {
        use keel_codegen::{SelectionOptions, SelectionTier};
        use keel_opt::OptimizationTier;
        for tier in ["none", "fast", "basic", "full"] {
            let standalone = options(&["--opt-tier", tier]).unwrap();
            assert_eq!(standalone.selection, SelectionOptions::default());
            assert_eq!(
                standalone.allocation,
                keel_codegen::regalloc::AllocationTier::Full
            );
            for flags in [
                vec!["-O", "--opt-tier", tier],
                vec!["--opt-tier", tier, "-O"],
                vec!["-O", "--opt-tier", tier, "-O"],
            ] {
                let opts = options(&flags).unwrap();
                assert_eq!(opts.selection, SelectionOptions::ALL);
                let expected = if flags.last() == Some(&"-O") {
                    OptimizationTier::Full
                } else {
                    match tier {
                        "none" => OptimizationTier::None,
                        "fast" => OptimizationTier::Fast,
                        "basic" => OptimizationTier::Basic,
                        _ => OptimizationTier::Full,
                    }
                };
                assert_eq!(opts.optimization, expected);
            }
        }
        assert_eq!(options(&[]).unwrap().selection, SelectionOptions::default());
        assert_eq!(
            options(&["-O", "-O"]).unwrap().selection,
            SelectionOptions::ALL
        );
        for (flag, value, expected) in [
            ("--isel", "none", SelectionOptions::default()),
            ("--isel", "all", SelectionOptions::ALL),
            ("--isel", "trees", SelectionTier::Trees.options()),
            (
                "--isel",
                "folds,hints",
                SelectionOptions {
                    trees: false,
                    folded_operands: true,
                    register_hints: true,
                },
            ),
            ("--isel-tier", "baseline", SelectionOptions::default()),
            ("--isel-tier", "trees", SelectionTier::Trees.options()),
            ("--isel-tier", "full", SelectionOptions::ALL),
        ] {
            for flags in [
                vec![flag, value, "-O"],
                vec!["-O", flag, value],
                vec!["-O", flag, value, "-O"],
            ] {
                assert_eq!(options(&flags).unwrap().selection, expected, "{flags:?}");
            }
        }
        for flags in [
            vec!["--isel", "all", "-O", "--isel-tier", "baseline", "-O"],
            vec!["--isel-tier", "full", "-O", "--isel", "none", "-O"],
        ] {
            assert_eq!(
                options(&flags).unwrap().selection,
                SelectionOptions::default()
            );
        }
        assert_eq!(
            options(&["--isel", "none", "-O", "--isel-tier", "full"])
                .unwrap()
                .selection,
            SelectionOptions::ALL
        );
    }

    #[test]
    fn optimization_preset_codegen_and_object_overrides_end_to_end() {
        let dir = std::env::temp_dir().join(format!("keel-cli-selection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ir = "func @f(i64, i64, i64) -> i64 {\nblock0(v0: i64, v1: i64, v2: i64):\n v3 = imul v0, v1\n v4 = iadd v3, v2\n return v4\n}\n";
        let c = "long f(long a, long b, long c) { return a * b + c; }\n";
        for (command, extension, source) in [("compile", "keel", ir), ("cc", "i", c)] {
            let input = dir.join(format!("input.{extension}"));
            let output = dir.join("output");
            std::fs::write(&input, source).unwrap();
            for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
                for object in [false, true] {
                    let invoke = |flags: &[&str]| {
                        let mut args = vec![
                            command.into(),
                            input.display().to_string(),
                            "--target".into(),
                            target.into(),
                            "-o".into(),
                            output.display().to_string(),
                        ];
                        if object {
                            args.push("--emit-object".into());
                        }
                        args.extend(flags.iter().map(|s| s.to_string()));
                        run(args).unwrap();
                        std::fs::read(&output).unwrap()
                    };
                    let selected = invoke(&["-O"]);
                    for flags in [
                        vec!["--isel", "all", "-O"],
                        vec!["-O", "--isel", "all"],
                        vec!["--isel-tier", "full", "-O"],
                    ] {
                        assert_eq!(
                            selected,
                            invoke(&flags),
                            "{command} {target} object={object} {flags:?}"
                        );
                    }
                    let baseline = invoke(&["--opt-tier", "full"]);
                    for flags in [
                        vec!["--isel", "none", "-O"],
                        vec!["-O", "--isel", "none"],
                        vec!["--isel-tier", "baseline", "-O"],
                    ] {
                        assert_eq!(
                            baseline,
                            invoke(&flags),
                            "{command} {target} object={object} {flags:?}"
                        );
                    }
                    if target == "aarch64-apple-darwin" && !object {
                        let selected = String::from_utf8(selected).unwrap();
                        let baseline = String::from_utf8(baseline).unwrap();
                        assert!(selected.contains("\tmadd\t"), "{selected}");
                        assert!(!baseline.contains("\tmadd\t"), "{baseline}");
                        assert!(baseline.contains("\tmul\t"), "{baseline}");
                    }
                }
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_pipeline_overrides_tiers_in_either_order() {
        let source = "func @f() -> i64 {\nblock0:\n    v0 = iconst.i64 20\n    v1 = iconst.i64 22\n    v2 = iadd v0, v1\n    return v2\n}";
        for args in [["-p", "dce", "-O"], ["-O", "-p", "dce"]] {
            let opts = options(&args).unwrap();
            assert_eq!(opts.selection, keel_codegen::SelectionOptions::ALL);
            let mut module = keel_ir::parse(source).unwrap();
            opts.optimize(&mut module).unwrap();
            assert!(
                module.to_string().contains("iadd"),
                "only dce should run; Full would fold the sum"
            );
        }
        let opts = options(&["-p", "missing"]).unwrap();
        let mut module = keel_ir::parse(source).unwrap();
        assert!(opts.optimize(&mut module).unwrap_err().contains("missing"));
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
            assert_eq!(opts.selection, keel_codegen::SelectionOptions::ALL);
        }
        for args in [
            vec!["--tune", "apple-m5"],
            vec!["--tune", "apple-m5", "--isel-tier", "baseline", "-O"],
            vec!["-O", "--tune", "apple-m5", "--isel-tier", "baseline"],
        ] {
            assert_eq!(
                parse(&args).unwrap().selection,
                keel_codegen::SelectionOptions::default()
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

#[cfg(test)]
mod x86_feature_option_tests {
    use super::*;
    #[test]
    fn cpu_profile_resolution_ignores_profile_flag_position() {
        let parse = |flags: &[&str]| {
            let mut args = vec!["--target".into(), "x86_64-unknown-linux-gnu".into()];
            args.extend(flags.iter().map(|s| s.to_string()));
            parse_options(&args, &["--target", "--cpu-level", "--target-features"])
                .unwrap()
                .x86_features
        };
        let before = parse(&[
            "--target-features",
            "-avx2",
            "--cpu-level",
            "v3",
            "--target-features",
            "aes",
        ]);
        let after = parse(&[
            "--cpu-level",
            "v3",
            "--target-features",
            "-avx2",
            "--target-features",
            "aes",
        ]);
        assert_eq!(before, after);
        assert!(!before.avx2 && before.aes);
        for flags in [
            ["--cpu-level", "v1"],
            ["--target-features", "none"],
            ["--target-features", ""],
        ] {
            let args = ["--target", "aarch64-apple-darwin", flags[0], flags[1]].map(str::to_owned);
            assert!(parse_options(&args, &["--target", flags[0]]).is_err());
        }
    }
    #[test]
    fn x86_features_parse_independently_of_tuning() {
        let args = [
            "--target-features",
            "+avx,bmi2,-avx",
            "--target",
            "x86_64-unknown-linux-gnu",
        ]
        .map(str::to_owned);
        let options = parse_options(&args, &["--target-features", "--target"]).unwrap();
        assert_eq!(
            options.x86_features,
            keel_asm::x86::Features {
                bmi2: true,
                xsave: true,
                osxsave: true,
                ..Default::default()
            }
        );
        assert_eq!(options.tuning, keel_codegen::tuning::TuningCpu::Generic);
        for args in [
            vec!["--target-features"],
            vec!["--target-features", "unknown"],
        ] {
            assert!(
                parse_options(
                    &args.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                    &["--target-features"]
                )
                .is_err()
            );
        }
    }
    #[test]
    fn explicit_cpu_levels_are_independent_of_tiers() {
        let args = [
            "--cpu-level",
            "v1",
            "--target-features",
            "bmi2",
            "--target",
            "x86_64-unknown-linux-gnu",
        ]
        .map(str::to_owned);
        let opts = parse_options(&args, &["--cpu-level", "--target-features", "--target"]).unwrap();
        assert!(opts.x86_options.supplied() && opts.x86_features.bmi2);
        assert!(!opts.x86_features.popcnt && !opts.x86_features.sse41);
        assert_eq!(opts.selection, keel_codegen::SelectionOptions::default());
    }
    #[test]
    fn target_feature_lists_accumulate_and_none_clears_the_profile() {
        for level in ["v1", "v2", "v3"] {
            let args = [
                "--target",
                "x86_64-unknown-linux-gnu",
                "--cpu-level",
                level,
                "--target-features",
                "bmi1,bmi2",
                "--target-features",
                "none",
            ]
            .map(str::to_owned);
            let opts =
                parse_options(&args, &["--target", "--cpu-level", "--target-features"]).unwrap();
            assert_eq!(opts.x86_features, keel_asm::x86::Features::NONE);
        }
    }
    #[test]
    fn strict_v1_cli_text_and_direct_object_paths_match() {
        let dir = std::env::temp_dir().join(format!("keel-v1-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("input.keel");
        let text = dir.join("output.s");
        let direct = dir.join("direct.o");
        let indirect = dir.join("indirect.o");
        std::fs::write(&input,"func @pc(i64) -> i64 {\nblock0(v0: i64):\n v1 = popcnt v0\n return v1\n}\nfunc @round(f64) -> f64 {\nblock0(v0: f64):\n v1 = fnearest v0\n return v1\n}\n").unwrap();
        let base = vec![
            "compile".to_owned(),
            "--cpu-level".into(),
            "v1".into(),
            "--target".into(),
            "x86_64-unknown-linux-gnu".into(),
            input.to_string_lossy().into_owned(),
        ];
        let mut args = base.clone();
        args.extend(["-o".into(), text.to_string_lossy().into_owned()]);
        run(args).unwrap();
        let mut args = base;
        args.extend([
            "--emit-object".into(),
            "-o".into(),
            direct.to_string_lossy().into_owned(),
        ]);
        run(args).unwrap();
        run(vec![
            "asm".into(),
            "--cpu-level".into(),
            "v1".into(),
            "--target".into(),
            "x86_64-unknown-linux-gnu".into(),
            text.to_string_lossy().into_owned(),
            "-o".into(),
            indirect.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert_eq!(
            std::fs::read(direct).unwrap(),
            std::fs::read(indirect).unwrap()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn null_remat_is_an_independent_experimental_fast_full_mode() {
        for tier in [
            "fast",
            "full",
            "local",
            "split",
            "weighted",
            "full-affinity",
        ] {
            let args = ["--remat", "null", "--regalloc", tier].map(str::to_owned);
            let result = parse_options(&args, &["--remat", "--regalloc"]);
            #[cfg(feature = "experimental-regalloc")]
            if matches!(tier, "fast" | "full") {
                assert_eq!(result.unwrap().remat, 8);
            } else {
                assert!(result.is_err());
            }
            #[cfg(not(feature = "experimental-regalloc"))]
            assert!(result.is_err());
        }
        #[cfg(feature = "experimental-regalloc")]
        for (mode, bits) in [("constants", 1), ("stack", 2), ("all", 3), ("pressure", 7)] {
            let args = ["--remat", mode, "--regalloc", "fast"].map(str::to_owned);
            assert_eq!(
                parse_options(&args, &["--remat", "--regalloc"])
                    .unwrap()
                    .remat,
                bits
            );
        }
    }
}

#[cfg(test)]
mod vector_context_tests {
    use super::*;
    #[test]
    fn target_free_portable_context_needs_no_codegen_host() {
        let opts = Options::default();
        let context = opts
            .optimization_context(keel_opt::VectorTarget::Portable)
            .unwrap();
        assert_eq!(context.target, keel_opt::VectorTarget::Portable);
        assert_eq!(context.capabilities, keel_ir::VectorCapabilities::BASELINE);
        let mut module = keel_ir::parse("func @f() {\nblock0:\n return\n}").unwrap();
        opts.run_optimization(&mut module, &["dce"], context.target)
            .unwrap();
    }

    fn opts(flags: &[&str]) -> Options {
        let mut args = vec![
            "--target".to_string(),
            "x86_64-unknown-linux-gnu".to_string(),
        ];
        args.extend(flags.iter().map(|s| s.to_string()));
        parse_options(&args, &["--target", "--cpu-level", "--target-features"]).unwrap()
    }
    #[test]
    fn cli_effective_context_matches_library_and_modifier_replacement() {
        for flags in [
            vec!["--cpu-level", "v1"],
            vec!["--cpu-level", "v2"],
            vec!["--cpu-level", "v3"],
            vec!["--cpu-level", "v3", "--target-features", "-avx2"],
            vec!["--cpu-level", "v1", "--target-features", "+avx,+avx2"],
            vec![
                "--cpu-level",
                "v3",
                "--target-features",
                "-avx2",
                "--target-features",
                "+aes",
            ],
        ] {
            let opts = opts(&flags);
            let context = opts.optimization_context(opts.vector_target()).unwrap();
            let selector = keel_codegen::x86_features::FeatureSelector {
                inner: &keel_codegen::SelectionTier::Full,
                features: opts.x86_features,
            };
            assert_eq!(
                context.capabilities,
                keel_codegen::x86_features::vector_capabilities(Target::X86_64LinuxGnu, &selector)
                    .unwrap()
            );
            assert_eq!(context.capabilities.preferred_bits(), 128);
            assert!(!context.capabilities.prefer_wide_signed_i32_sum());
        }
        let replaced = opts(&[
            "--cpu-level",
            "v3",
            "--target-features",
            "-avx2",
            "--target-features",
            "+aes",
        ]);
        assert!(!replaced.x86_features.avx2 && replaced.x86_features.aes);
        let mut invalid = opts(&["--cpu-level", "v1"]);
        invalid.x86_features.avx2 = true;
        let mut module = keel_ir::parse("func @f() {\nblock0:\n return\n}").unwrap();
        let before = module.to_string();
        assert!(invalid.optimize(&mut module).is_err());
        assert_eq!(before, module.to_string());
    }
}

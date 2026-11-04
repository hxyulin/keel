//! `cargo suite codegen-diff`: compiles every suite translation unit to
//! assembly with two keel binaries and reports which objects and functions
//! changed, with instruction and byte deltas from `keel asm-metrics`.

use crate::build::{self, Env, InputMode};
use crate::counters;
use crate::manifest::{Compiler, Config, Configs, Suite};
use crate::style::{Cell, Paint, bar, fail_mark, ok_mark, paint, print_table};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(clap::Args)]
pub struct Options {
    /// The keel binary to compare against.
    #[arg(long)]
    baseline: PathBuf,
    /// The keel binary under test.
    #[arg(long)]
    candidate: PathBuf,
    /// Suites to compile (default: all).
    suites: Vec<String>,
    /// Keel configurations, comma-separated.
    #[arg(short, long, value_delimiter = ',', default_values = ["keel", "keel-O"])]
    config: Vec<String>,
    /// Targets, comma-separated.
    #[arg(long, value_enum, value_delimiter = ',', default_values = ["aarch64", "x86_64"])]
    target: Vec<Target>,
    /// Header root (with usr/include) for x86_64 when this host is not x86-64 Linux.
    #[arg(long)]
    x86_64_sysroot: Option<PathBuf>,
    /// SDK root for aarch64 when this host is not an Apple silicon Mac.
    #[arg(long)]
    aarch64_sysroot: Option<PathBuf>,
    /// Record each compile's retired instructions (`/usr/bin/time -l` on
    /// macOS, `perf stat -e instructions:u` on Linux).
    #[arg(long)]
    icount: bool,
    /// Exit non-zero if any output differs.
    #[arg(long)]
    expect_identical: bool,
    /// Also write the full report as JSON to this file.
    #[arg(long)]
    json: Option<PathBuf>,
    /// Where to put the assembly (default: target/keel-suite/codegen-diff).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Changed functions to list per object.
    #[arg(long, default_value_t = 10)]
    show: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Target {
    /// aarch64-apple-darwin
    Aarch64,
    /// x86_64-unknown-linux-gnu
    #[value(name = "x86_64")]
    X86_64,
}

impl Target {
    pub(crate) fn triple(self) -> &'static str {
        match self {
            Target::Aarch64 => "aarch64-apple-darwin",
            Target::X86_64 => "x86_64-unknown-linux-gnu",
        }
    }

    /// The `os` and `arch` that `[[build.when]]` entries match.
    pub(crate) fn os_arch(self) -> (&'static str, &'static str) {
        match self {
            Target::Aarch64 => ("macos", "aarch64"),
            Target::X86_64 => ("linux", "x86_64"),
        }
    }
}

#[derive(Serialize)]
pub struct Report {
    schema_version: u32,
    baseline: Binary,
    candidate: Binary,
    summary: Vec<Summary>,
    objects: Vec<Object>,
}

#[derive(Serialize)]
struct Binary {
    path: String,
    sha256: String,
}

#[derive(Serialize, Default)]
struct Summary {
    target: &'static str,
    config: String,
    objects: usize,
    identical: usize,
    changed: usize,
    /// Objects neither compiler could build; not compared.
    failed: usize,
    functions_changed: usize,
    functions_comment_only: usize,
    functions_added: usize,
    functions_removed: usize,
    /// Candidate minus baseline over changed objects.
    text_bytes_delta: i64,
    instructions_delta: i64,
    /// Compiler retired instructions summed over objects, with `--icount`.
    baseline_retired: Option<u64>,
    candidate_retired: Option<u64>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Identical,
    Changed,
    /// Both compilers failed.
    Failed,
}

#[derive(Serialize)]
struct Object {
    suite: String,
    source: String,
    target: &'static str,
    config: String,
    status: Status,
    baseline: Side,
    candidate: Side,
    /// Functions present in both outputs with identical text.
    functions_identical: usize,
    /// Every other function, largest byte change first.
    functions: Vec<FunctionChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metrics_error: Option<String>,
}

#[derive(Serialize, Default)]
struct Side {
    asm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retired: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<u64>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
enum Change {
    Changed,
    /// Only comment lines differ.
    CommentOnly,
    Added,
    Removed,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
struct FunctionChange {
    name: String,
    change: Change,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<Size>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate: Option<Size>,
}

impl FunctionChange {
    fn delta(&self, field: fn(&Size) -> u64) -> Option<i64> {
        let side = |s: &Option<Size>| s.as_ref().map_or(0, |s| field(s) as i64);
        (self.baseline.is_some() || self.candidate.is_some())
            .then(|| side(&self.candidate) - side(&self.baseline))
    }
}

#[derive(Serialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Size {
    instructions: u64,
    bytes: u64,
}

/// One translation unit in one target and configuration.
pub(crate) struct Job<'a> {
    pub(crate) suite: &'a Suite,
    pub(crate) work: &'a Path,
    pub(crate) source: &'a Path,
    pub(crate) unit: usize,
    pub(crate) target: Target,
    pub(crate) config: &'a Config,
}

impl Job<'_> {
    /// The source path relative to the suite's work directory or the root.
    pub(crate) fn relative(&self, root: &Path) -> String {
        self.source
            .strip_prefix(self.work)
            .or_else(|_| self.source.strip_prefix(root))
            .unwrap_or(self.source)
            .display()
            .to_string()
    }

    /// The `keel cc` command for this job, writing `out`.
    pub(crate) fn cc(&self, keel: &Path, sysroot: Option<&Path>, out: &Path) -> Vec<String> {
        let build = &self.suite.manifest.build;
        let (os, arch) = self.target.os_arch();
        let mut argv = vec![keel.display().to_string(), "cc".into()];
        argv.extend(build.define_flags_for(os, arch));
        argv.extend(build.include_flags());
        argv.extend(self.config.flags.iter().cloned());
        argv.extend(["--target".into(), self.target.triple().into()]);
        if let Some(dir) = sysroot {
            argv.extend(["--sysroot".into(), dir.display().to_string()]);
        }
        argv.extend([
            self.source.display().to_string(),
            "-o".into(),
            out.display().to_string(),
        ]);
        argv
    }
}

/// The sysroot to pass for each target that is not this host's.
pub(crate) fn sysroots(
    targets: &[Target],
    aarch64: Option<&Path>,
    x86_64: Option<&Path>,
) -> Result<Vec<(Target, PathBuf)>> {
    let host = keel_ir::Target::host().map(|t| t.triple());
    let mut sysroots = Vec::new();
    for (target, sysroot) in [(Target::Aarch64, aarch64), (Target::X86_64, x86_64)] {
        if !targets.contains(&target) {
            continue;
        }
        match sysroot {
            Some(dir) => sysroots.push((
                target,
                dir.canonicalize()
                    .with_context(|| dir.display().to_string())?,
            )),
            None if host != Some(target.triple()) => {
                let flag = match target {
                    Target::Aarch64 => "--aarch64-sysroot",
                    Target::X86_64 => "--x86-64-sysroot",
                };
                bail!(
                    "this host is not {}: give its headers with {flag}, or leave it out with --target",
                    target.triple()
                );
            }
            None => {}
        }
    }
    Ok(sysroots)
}

/// Every translation unit in every target and configuration, largest first,
/// so the largest inputs do not finish last on one thread.
pub(crate) fn units<'a>(
    prepared: &'a [(&'a Suite, build::Sources)],
    targets: &[Target],
    configs: &'a [Config],
) -> Vec<Job<'a>> {
    let mut work: Vec<(u64, Job)> = Vec::new();
    for (suite, sources) in prepared {
        for (unit, source) in sources.pre.iter().enumerate() {
            let size = fs::metadata(source).map_or(0, |m| m.len());
            for &target in targets {
                for config in configs {
                    work.push((
                        size,
                        Job {
                            suite,
                            work: &sources.work,
                            source,
                            unit,
                            target,
                            config,
                        },
                    ));
                }
            }
        }
    }
    work.sort_by_key(|(size, _)| std::cmp::Reverse(*size));
    work.into_iter().map(|(_, job)| job).collect()
}

struct Tools {
    baseline: PathBuf,
    candidate: PathBuf,
    out: PathBuf,
    icount: bool,
    sysroots: Vec<(Target, PathBuf)>,
}

pub fn run(root: &Path, options: Options, jobs: usize, input_mode: InputMode) -> Result<bool> {
    if input_mode != InputMode::Native {
        bail!("codegen-diff preprocesses for each target; use the native input mode");
    }
    let resolve = |p: &Path| p.canonicalize().with_context(|| p.display().to_string());
    let sysroots = sysroots(
        &options.target,
        options.aarch64_sysroot.as_deref(),
        options.x86_64_sysroot.as_deref(),
    )?;
    if options.icount && !cfg!(any(target_os = "macos", target_os = "linux")) {
        bail!("--icount needs macOS or Linux");
    }
    let tools = Tools {
        baseline: resolve(&options.baseline)?,
        candidate: resolve(&options.candidate)?,
        out: options
            .out
            .clone()
            .unwrap_or_else(|| root.join("target/keel-suite/codegen-diff")),
        icount: options.icount,
        sysroots,
    };
    let configs = Configs::load(root)?;
    let selected = configs.select(&options.config, &[])?;
    for config in &selected {
        if config.compiler != Compiler::Keel || config.flags.iter().any(|f| f == "--emit-object") {
            bail!(
                "configuration `{}` does not compile with keel to assembly",
                config.name
            );
        }
    }
    let all = crate::manifest::load_suites(root)?;
    let chosen = crate::choose(&all, &options.suites)?;
    let env = Env {
        root: root.to_path_buf(),
        keel: tools.candidate.clone(),
        jobs,
        input_mode,
    };
    let mut prepared = Vec::new();
    for suite in &chosen {
        prepared.push((*suite, build::sources(&env, suite)?));
    }
    for side in ["baseline", "candidate"] {
        let dir = tools.out.join(side);
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| dir.display().to_string())?;
        }
    }

    let work = units(&prepared, &options.target, &selected);
    let progress = bar(work.len(), "codegen-diff".into());
    let objects = build::parallel(&work, jobs, &progress, |job| {
        compare(root, &tools, job).with_context(|| {
            format!(
                "{} {} {} {}",
                job.suite.name,
                job.source.display(),
                job.target.triple(),
                job.config.name
            )
        })
    });
    progress.finish_and_clear();
    let mut objects = objects?;
    let target_order = |t: &str| options.target.iter().position(|x| x.triple() == t);
    let config_order = |c: &str| selected.iter().position(|x| x.name == c);
    objects.sort_by(|a, b| {
        (
            target_order(a.target),
            config_order(&a.config),
            &a.suite,
            &a.source,
        )
            .cmp(&(
                target_order(b.target),
                config_order(&b.config),
                &b.suite,
                &b.source,
            ))
    });

    let report = Report {
        schema_version: 1,
        baseline: binary(&tools.baseline)?,
        candidate: binary(&tools.candidate)?,
        summary: summarize(&objects),
        objects,
    };
    print_report(root, &report, &tools.out, options.show);
    if let Some(file) = &options.json {
        fs::write(file, serde_json::to_string_pretty(&report)? + "\n")
            .with_context(|| file.display().to_string())?;
    }
    let changed = report.objects.iter().any(|o| o.status == Status::Changed);
    Ok(!(options.expect_identical && changed))
}

fn binary(path: &Path) -> Result<Binary> {
    let digest = Sha256::digest(fs::read(path).with_context(|| path.display().to_string())?);
    Ok(Binary {
        path: path.display().to_string(),
        sha256: digest.iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Compiles one job with both binaries and compares the outputs.
fn compare(root: &Path, tools: &Tools, job: &Job) -> Result<Object> {
    let stem = job.source.file_stem().unwrap().to_string_lossy();
    let name = format!("unit-{}-{stem}.s", job.unit);
    let path = |side: &str| {
        tools
            .out
            .join(side)
            .join(job.target.triple())
            .join(&job.config.name)
            .join(&job.suite.name)
            .join(&name)
    };
    let (base_path, cand_path) = (path("baseline"), path("candidate"));
    let base = compile(tools, &tools.baseline, job, &base_path)?;
    let cand = compile(tools, &tools.candidate, job, &cand_path)?;
    let mut object = Object {
        suite: job.suite.name.clone(),
        source: job.relative(root),
        target: job.target.triple(),
        config: job.config.name.clone(),
        status: Status::Identical,
        baseline: Side {
            asm: base_path.display().to_string(),
            error: base.error,
            retired: base.retired,
            ..Side::default()
        },
        candidate: Side {
            asm: cand_path.display().to_string(),
            error: cand.error,
            retired: cand.retired,
            ..Side::default()
        },
        functions_identical: 0,
        functions: Vec::new(),
        metrics_error: None,
    };
    let (base_text, cand_text) = match (base.text, cand.text) {
        (None, None) => {
            object.status = Status::Failed;
            return Ok(object);
        }
        (Some(b), Some(c)) if b == c => {
            object.functions_identical = functions(&String::from_utf8_lossy(&b)).len();
            return Ok(object);
        }
        (b, c) => (b.unwrap_or_default(), c.unwrap_or_default()),
    };
    object.status = Status::Changed;
    let (base_text, cand_text) = (
        String::from_utf8_lossy(&base_text),
        String::from_utf8_lossy(&cand_text),
    );
    let metrics = |text: &str, path: &Path| -> Result<Option<Metrics>, String> {
        if text.is_empty() {
            return Ok(None);
        }
        metrics(&tools.candidate, job.target, path).map(Some)
    };
    let (base_metrics, cand_metrics) = match (
        metrics(&base_text, &base_path),
        metrics(&cand_text, &cand_path),
    ) {
        (Ok(b), Ok(c)) => (b, c),
        (Err(e), _) | (_, Err(e)) => {
            object.metrics_error = Some(e);
            (None, None)
        }
    };
    for (side, m) in [
        (&mut object.baseline, &base_metrics),
        (&mut object.candidate, &cand_metrics),
    ] {
        if let Some(m) = m {
            side.text_bytes = Some(m.text_bytes);
            side.instructions = Some(m.instructions.len() as u64);
        }
    }
    let diff = diff_functions(
        &base_text,
        &cand_text,
        base_metrics.as_ref(),
        cand_metrics.as_ref(),
    );
    object.functions_identical = diff.identical;
    object.functions = diff.changes;
    Ok(object)
}

struct Compiled {
    text: Option<Vec<u8>>,
    error: Option<String>,
    retired: Option<u64>,
}

fn compile(tools: &Tools, keel: &Path, job: &Job, out: &Path) -> Result<Compiled> {
    fs::create_dir_all(out.parent().unwrap())?;
    let mut argv: Vec<String> = Vec::new();
    if tools.icount {
        if cfg!(target_os = "macos") {
            argv.extend(["/usr/bin/time".into(), "-l".into()]);
        } else {
            argv.extend(
                ["perf", "stat", "-x", ",", "-e", "instructions:u", "--"].map(String::from),
            );
        }
    }
    let sysroot = tools.sysroots.iter().find(|(t, _)| *t == job.target);
    argv.extend(job.cc(keel, sysroot.map(|(_, dir)| dir.as_path()), out));
    let output = build::run(&argv, &job.work.join(&job.suite.manifest.build.dir), None)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let retired = tools
        .icount
        .then(|| counters::parse(&stderr).get("instructions").copied())
        .flatten();
    if !output.status.success() {
        return Ok(Compiled {
            text: None,
            error: Some(build::tail(&output, 5).trim().to_string()),
            retired,
        });
    }
    Ok(Compiled {
        text: Some(fs::read(out).with_context(|| out.display().to_string())?),
        error: None,
        retired,
    })
}

/// The instructions of one assembly file, as `keel asm-metrics` reports them.
#[derive(Debug, PartialEq, Eq)]
struct Metrics {
    text_bytes: u64,
    /// (1-based source line, encoded size), in line order.
    instructions: Vec<(usize, u64)>,
}

fn metrics(keel: &Path, target: Target, asm: &Path) -> Result<Metrics, String> {
    let output = Command::new(keel)
        .args(["asm-metrics", "--target", target.triple()])
        .arg(asm)
        .output()
        .map_err(|e| format!("running {}: {e}", keel.display()))?;
    if !output.status.success() {
        return Err(format!(
            "asm-metrics failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_metrics(&output.stdout)
}

/// Parses both `asm-metrics` schemas: x86-64 `regions` and AArch64 `instructions`.
fn parse_metrics(json: &[u8]) -> Result<Metrics, String> {
    #[derive(Deserialize)]
    struct Raw {
        text_bytes: u64,
        #[serde(default)]
        regions: Option<Vec<Region>>,
        #[serde(default)]
        instructions: Option<Vec<Location>>,
    }
    #[derive(Deserialize)]
    struct Region {
        kind: String,
        line: usize,
        size: u64,
    }
    #[derive(Deserialize)]
    struct Location {
        line: usize,
    }
    let raw: Raw = serde_json::from_slice(json).map_err(|e| format!("asm-metrics output: {e}"))?;
    let mut instructions: Vec<(usize, u64)> = match (raw.regions, raw.instructions) {
        (Some(regions), _) => regions
            .into_iter()
            .filter(|r| r.kind == "instruction")
            .map(|r| (r.line, r.size))
            .collect(),
        (None, Some(locations)) => locations.into_iter().map(|l| (l.line, 4)).collect(),
        (None, None) => return Err("asm-metrics output has no instructions".into()),
    };
    instructions.sort_unstable();
    Ok(Metrics {
        text_bytes: raw.text_bytes,
        instructions,
    })
}

/// A function in keel's assembly: its symbol label through `L<name>_end:`.
#[derive(Debug, PartialEq, Eq)]
struct Function<'a> {
    name: &'a str,
    /// 1-based, inclusive.
    first: usize,
    last: usize,
}

/// The label a line defines, without quotes.
fn label(line: &str) -> Option<&str> {
    let name = line.trim().strip_suffix(':')?;
    let name = name
        .strip_prefix('"')
        .and_then(|n| n.strip_suffix('"'))
        .unwrap_or(name);
    (!name.is_empty() && !name.contains(char::is_whitespace)).then_some(name)
}

/// The functions of keel's assembly output. Keel opens every function with
/// its symbol, then `L<name>_begin:` (`.L` on ELF), and closes it with
/// `L<name>_end:`.
fn functions(text: &str) -> Vec<Function<'_>> {
    let mut found = Vec::new();
    let mut previous: Option<(usize, &str)> = None;
    let mut open: Option<(usize, &str, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let this = label(line);
        if let (Some(here), Some((start, name, end))) = (this, &open)
            && here == end
        {
            found.push(Function {
                name,
                first: *start,
                last: number,
            });
            open = None;
        } else if let (Some(here), None, Some((start, symbol))) = (this, &open, previous) {
            let local = here
                .strip_prefix(".L")
                .or_else(|| here.strip_prefix('L'))
                .and_then(|n| n.strip_suffix("_begin"));
            if let Some(name) = local
                && (symbol == name || symbol.strip_prefix('_') == Some(name))
            {
                let prefix = &here[..here.len() - name.len() - "_begin".len()];
                open = Some((start, name, format!("{prefix}{name}_end")));
            }
        }
        previous = this.map(|l| (number, l));
    }
    found
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with(';') || t.starts_with('#') || t.starts_with("//")
}

fn without_comments<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    lines.filter(|l| !is_comment(l)).collect()
}

struct FunctionDiff {
    identical: usize,
    changes: Vec<FunctionChange>,
}

/// Name for the lines outside every function (data, section directives).
const OUTSIDE: &str = "(outside functions)";

fn diff_functions(
    base: &str,
    cand: &str,
    base_metrics: Option<&Metrics>,
    cand_metrics: Option<&Metrics>,
) -> FunctionDiff {
    let base_lines: Vec<&str> = base.lines().collect();
    let cand_lines: Vec<&str> = cand.lines().collect();
    let base_fns = functions(base);
    let cand_fns = functions(cand);
    let size = |m: Option<&Metrics>, f: &Function| {
        m.map(|m| {
            let start = m.instructions.partition_point(|(line, _)| *line < f.first);
            let end = m.instructions.partition_point(|(line, _)| *line <= f.last);
            let inside = &m.instructions[start..end];
            Size {
                instructions: inside.len() as u64,
                bytes: inside.iter().map(|(_, size)| size).sum(),
            }
        })
    };
    let by_name: HashMap<&str, &Function> = cand_fns.iter().map(|f| (f.name, f)).collect();
    let mut identical = 0;
    let mut changes = Vec::new();
    for b in &base_fns {
        let Some(c) = by_name.get(b.name) else {
            changes.push(FunctionChange {
                name: b.name.into(),
                change: Change::Removed,
                baseline: size(base_metrics, b),
                candidate: None,
            });
            continue;
        };
        let (bb, cb) = (
            &base_lines[b.first - 1..b.last],
            &cand_lines[c.first - 1..c.last],
        );
        let change = if bb == cb {
            identical += 1;
            continue;
        } else if without_comments(bb.iter().copied()) == without_comments(cb.iter().copied()) {
            Change::CommentOnly
        } else {
            Change::Changed
        };
        changes.push(FunctionChange {
            name: b.name.into(),
            change,
            baseline: size(base_metrics, b),
            candidate: size(cand_metrics, c),
        });
    }
    let base_names: std::collections::HashSet<&str> = base_fns.iter().map(|f| f.name).collect();
    for c in cand_fns.iter().filter(|c| !base_names.contains(c.name)) {
        changes.push(FunctionChange {
            name: c.name.into(),
            change: Change::Added,
            baseline: None,
            candidate: size(cand_metrics, c),
        });
    }
    if outside(&base_lines, &base_fns) != outside(&cand_lines, &cand_fns) {
        changes.push(FunctionChange {
            name: OUTSIDE.into(),
            change: Change::Changed,
            baseline: None,
            candidate: None,
        });
    }
    changes.sort_by(|a, b| {
        let magnitude = |c: &FunctionChange| c.delta(|s| s.bytes).map_or(0, i64::abs);
        (
            a.change == Change::CommentOnly,
            std::cmp::Reverse(magnitude(a)),
            &a.name,
        )
            .cmp(&(
                b.change == Change::CommentOnly,
                std::cmp::Reverse(magnitude(b)),
                &b.name,
            ))
    });
    FunctionDiff { identical, changes }
}

/// The lines outside every function, without comments.
fn outside<'a>(lines: &[&'a str], functions: &[Function]) -> Vec<&'a str> {
    let mut inside = vec![false; lines.len()];
    for f in functions {
        inside[f.first - 1..f.last].fill(true);
    }
    let rest = lines
        .iter()
        .zip(inside)
        .filter(|(_, inside)| !inside)
        .map(|(l, _)| *l);
    without_comments(rest)
}

fn summarize(objects: &[Object]) -> Vec<Summary> {
    let mut summary: Vec<Summary> = Vec::new();
    for o in objects {
        let index = match summary
            .iter()
            .position(|s| s.target == o.target && s.config == o.config)
        {
            Some(i) => i,
            None => {
                summary.push(Summary {
                    target: o.target,
                    config: o.config.clone(),
                    baseline_retired: Some(0),
                    candidate_retired: Some(0),
                    ..Summary::default()
                });
                summary.len() - 1
            }
        };
        let s = &mut summary[index];
        s.objects += 1;
        match o.status {
            Status::Identical => s.identical += 1,
            Status::Changed => s.changed += 1,
            Status::Failed => s.failed += 1,
        }
        for f in &o.functions {
            match f.change {
                Change::Changed => s.functions_changed += 1,
                Change::CommentOnly => s.functions_comment_only += 1,
                Change::Added => s.functions_added += 1,
                Change::Removed => s.functions_removed += 1,
            }
        }
        let delta = |b: Option<u64>, c: Option<u64>| c.unwrap_or(0) as i64 - b.unwrap_or(0) as i64;
        s.text_bytes_delta += delta(o.baseline.text_bytes, o.candidate.text_bytes);
        s.instructions_delta += delta(o.baseline.instructions, o.candidate.instructions);
        let add = |total: &mut Option<u64>, n: Option<u64>| {
            *total = total.zip(n).map(|(t, n)| t + n);
        };
        add(&mut s.baseline_retired, o.baseline.retired);
        add(&mut s.candidate_retired, o.candidate.retired);
    }
    summary
}

fn signed(n: i64) -> String {
    if n > 0 {
        format!("+{n}")
    } else {
        n.to_string()
    }
}

fn print_report(root: &Path, report: &Report, out: &Path, show: usize) {
    for (name, b) in [
        ("baseline ", &report.baseline),
        ("candidate", &report.candidate),
    ] {
        println!(
            "{} {}  {}",
            paint(name, Paint::Bold),
            b.path,
            paint(&b.sha256[..12], Paint::Dim)
        );
    }
    println!();
    let icount = report.summary.iter().any(|s| s.baseline_retired > Some(0));
    let mut header = [
        "",
        "config",
        "objects",
        "identical",
        "changed",
        "failed",
        "functions",
        "bytes",
        "insts",
    ]
    .map(|h| Cell::new(h, Paint::Bold))
    .to_vec();
    if icount {
        header.push(Cell::new("retired", Paint::Bold));
    }
    let rows = report
        .summary
        .iter()
        .map(|s| {
            let changed = if s.changed > 0 {
                Paint::Yellow
            } else {
                Paint::Plain
            };
            let mut row = vec![
                Cell::plain(s.target),
                Cell::plain(&s.config),
                Cell::plain(s.objects.to_string()),
                Cell::plain(s.identical.to_string()),
                Cell::new(s.changed.to_string(), changed),
                Cell::plain(s.failed.to_string()),
                Cell::plain(s.functions_changed.to_string()),
                Cell::plain(signed(s.text_bytes_delta)),
                Cell::plain(signed(s.instructions_delta)),
            ];
            if icount {
                row.push(Cell::plain(
                    match (s.baseline_retired, s.candidate_retired) {
                        (Some(b), Some(c)) if b > 0 => {
                            format!("{:+.2}%", (c as f64 / b as f64 - 1.0) * 100.0)
                        }
                        _ => "n/a".into(),
                    },
                ));
            }
            row
        })
        .collect();
    print_table(header, rows);

    for o in &report.objects {
        match o.status {
            Status::Identical => continue,
            Status::Failed => {
                println!(
                    "\n{} {} {} {} {}",
                    paint("both failed", Paint::Red),
                    o.target,
                    o.config,
                    o.suite,
                    o.source
                );
                if let Some(e) = &o.candidate.error {
                    println!("  {}", paint(e.lines().last().unwrap_or(""), Paint::Dim));
                }
                continue;
            }
            Status::Changed => {}
        }
        println!(
            "\n{} {} {} {}",
            paint(o.target, Paint::Heading),
            o.config,
            o.suite,
            o.source
        );
        for (side, s) in [("baseline", &o.baseline), ("candidate", &o.candidate)] {
            if let Some(e) = &s.error {
                println!(
                    "  {} {}",
                    paint(format!("{side} failed:"), Paint::Red),
                    e.lines().last().unwrap_or("")
                );
            }
        }
        if let Some(e) = &o.metrics_error {
            println!("  {}", paint(e, Paint::Red));
        }
        let listed = &o.functions[..o.functions.len().min(show)];
        let width = listed
            .iter()
            .map(|f| f.name.chars().count())
            .max()
            .unwrap_or(0);
        for f in listed {
            let delta = |field: fn(&Size) -> u64, unit: &str| {
                f.delta(field)
                    .map_or(String::new(), |d| format!("{:>8} {unit}", signed(d)))
            };
            let change = match f.change {
                Change::Changed => "changed",
                Change::CommentOnly => "comments",
                Change::Added => "added",
                Change::Removed => "removed",
            };
            let line = format!(
                "  {:<width$}  {}{}{}",
                f.name,
                paint(format!("{change:<8}"), Paint::Dim),
                delta(|s| s.bytes, "B"),
                delta(|s| s.instructions, "insts"),
            );
            println!("{}", line.trim_end());
        }
        if o.functions.len() > show {
            println!(
                "  {}",
                paint(
                    format!("and {} more functions", o.functions.len() - show),
                    Paint::Dim
                )
            );
        }
    }

    let total: usize = report.summary.iter().map(|s| s.objects).sum();
    let changed: usize = report.summary.iter().map(|s| s.changed).sum();
    let shown = out.strip_prefix(root).unwrap_or(out);
    println!();
    if changed == 0 {
        println!("{} all {total} compared objects identical", ok_mark());
    } else {
        println!("{} {changed} of {total} objects changed", fail_mark());
    }
    println!(
        "{}",
        paint(format!("assembly in {}", shown.display()), Paint::Dim)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARWIN: &str = "\t.section\t__TEXT,__text
\t.globl\t_sum
\t.p2align\t2
_sum:
Lsum_begin:
\t; regalloc: v0=x9
\t.cfi_startproc
\tmov\tx9, x0
Lsum_site1_begin:
\tadd\tx0, x9, #1
\tret
Lsum_end:
\t.cfi_endproc
\t.p2align\t2
_helper:
Lhelper_begin:
\tret
Lhelper_end:
_g:
\t.byte\t5
";

    const ELF: &str = "\t.intel_syntax noprefix
\t.globl \"sum\"
\t.type\t\"sum\",@function
\"sum\":
.Lsum_begin:
\t# v2 = iconst.i32 0
\txor\teax, eax
\tjg\t\".Lsum_block2\"
.Lsum_block2:
\tret
.Lsum_end:
\t.cfi_endproc
";

    #[test]
    fn finds_functions_on_both_targets() {
        assert_eq!(
            functions(DARWIN),
            [
                Function {
                    name: "sum",
                    first: 4,
                    last: 12
                },
                Function {
                    name: "helper",
                    first: 15,
                    last: 18
                },
            ]
        );
        assert_eq!(
            functions(ELF),
            [Function {
                name: "sum",
                first: 4,
                last: 11
            }]
        );
        // A `_begin` label that does not follow its symbol opens nothing.
        assert!(functions("_g:\n\t.byte 1\nLsum_begin:\nLsum_end:\n").is_empty());
    }

    fn metrics_for(text: &str, size: u64) -> Metrics {
        let instructions = text
            .lines()
            .enumerate()
            .filter(|(_, l)| l.starts_with('\t') && !l.trim_start().starts_with(['.', ';', '#']))
            .map(|(i, _)| (i + 1, size))
            .collect::<Vec<_>>();
        Metrics {
            text_bytes: instructions.iter().map(|(_, s)| s).sum(),
            instructions,
        }
    }

    #[test]
    fn classifies_function_changes() {
        let comments = DARWIN.replace("v0=x9", "v1=x9");
        let diff = diff_functions(DARWIN, &comments, None, None);
        assert_eq!(diff.identical, 1);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].change, Change::CommentOnly);

        let changed = DARWIN
            .replace(
                "\tadd\tx0, x9, #1\n",
                "\tadd\tx0, x9, #1\n\tadd\tx0, x0, #1\n",
            )
            .replace("_helper:\nLhelper_begin:", "_other:\nLother_begin:")
            .replace("Lhelper_end:", "Lother_end:")
            .replace("\t.byte\t5", "\t.byte\t6");
        let (base, cand) = (metrics_for(DARWIN, 4), metrics_for(&changed, 4));
        let diff = diff_functions(DARWIN, &changed, Some(&base), Some(&cand));
        assert_eq!(diff.identical, 0);
        let summary: Vec<(&str, Change, Option<i64>, Option<i64>)> = diff
            .changes
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.change,
                    c.delta(|s| s.bytes),
                    c.delta(|s| s.instructions),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("helper", Change::Removed, Some(-4), Some(-1)),
                ("other", Change::Added, Some(4), Some(1)),
                ("sum", Change::Changed, Some(4), Some(1)),
                (OUTSIDE, Change::Changed, None, None),
            ]
        );
        assert_eq!(
            diff.changes[2].baseline,
            Some(Size {
                instructions: 3,
                bytes: 12
            })
        );
    }

    #[test]
    fn parses_both_metrics_schemas() {
        let x86 = br#"{"schema_version":2,"text_bytes":9,"regions":[
            {"kind":"instruction","line":7,"size":2},
            {"kind":"alignment","line":8,"size":3},
            {"kind":"instruction","line":5,"size":4}]}"#;
        assert_eq!(
            parse_metrics(x86).unwrap(),
            Metrics {
                text_bytes: 9,
                instructions: vec![(5, 4), (7, 2)]
            }
        );
        let a64 = br#"{"schema_version":1,"text_bytes":8,"instruction_bytes":8,
            "instructions":[{"line":3,"offset":0,"word":1},{"line":4,"offset":4,"word":2}]}"#;
        assert_eq!(
            parse_metrics(a64).unwrap(),
            Metrics {
                text_bytes: 8,
                instructions: vec![(3, 4), (4, 4)]
            }
        );
        assert!(parse_metrics(br#"{"text_bytes":0}"#).is_err());
    }
}

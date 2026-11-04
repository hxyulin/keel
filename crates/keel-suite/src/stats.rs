//! `cargo suite stats`: compiles every suite translation unit with
//! `--opt-report` and `--codegen-report`, and totals the reports per
//! optimizer pass, per inlining outcome and per backend counter.

use crate::build::{self, Env, InputMode};
use crate::codegen_diff::{self, Job, Target};
use crate::manifest::{Compiler, Configs};
use crate::style::{Cell, Paint, bar, fail_mark, ok_mark, paint, print_table};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct Options {
    /// Suites to compile (default: all).
    suites: Vec<String>,
    /// The keel binary (default: a release build of this checkout).
    #[arg(long)]
    keel: Option<PathBuf>,
    /// Keel configuration from suite/configs.toml.
    #[arg(short, long, default_value = "keel-O")]
    config: String,
    /// Target (default: this host's).
    #[arg(long, value_enum)]
    target: Option<Target>,
    /// Header root (with usr/include) for x86_64 when this host is not x86-64 Linux.
    #[arg(long)]
    x86_64_sysroot: Option<PathBuf>,
    /// SDK root for aarch64 when this host is not an Apple silicon Mac.
    #[arg(long)]
    aarch64_sysroot: Option<PathBuf>,
    /// List the functions this pass changed, largest instruction change first.
    #[arg(long)]
    pass: Option<String>,
    /// Functions to list with `--pass`.
    #[arg(long, default_value_t = 20)]
    show: usize,
    /// Also write the totals as JSON to this file.
    #[arg(long)]
    json: Option<PathBuf>,
    /// Where to put the outputs and reports (default: target/keel-suite/stats).
    #[arg(long)]
    out: Option<PathBuf>,
}

/// The parts of a schema-2 `--opt-report` that the totals use.
#[derive(Deserialize)]
struct OptReport {
    schema_version: u32,
    invocations: Vec<Invocation>,
}

#[derive(Deserialize)]
struct Invocation {
    pipeline_index: usize,
    pass: String,
    function: Option<String>,
    changed: bool,
    pass_ns: u64,
    before: Counts,
    after: Counts,
    inline_sites: Vec<InlineSite>,
}

#[derive(Deserialize)]
struct Counts {
    blocks: i64,
    instructions: i64,
    block_parameters: i64,
}

#[derive(Deserialize)]
struct InlineSite {
    category: String,
    outcome: String,
}

/// The parts of a schema-1 `--codegen-report` that the totals use.
#[derive(Deserialize)]
struct CodegenReport {
    schema_version: u32,
    functions: Vec<CodegenFunction>,
}

#[derive(Deserialize)]
struct CodegenFunction {
    counters: Counters,
}

/// Backend counters by name, in the order the report lists them, so that
/// counters a newer keel adds need no change here.
struct Counters(Vec<(String, u64)>);

impl<'de> Deserialize<'de> for Counters {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Counters;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of counter names to counts")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Counters, A::Error> {
                let mut counters = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    counters.push(entry);
                }
                Ok(Counters(counters))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// One translation unit's reports, or why it has none.
struct Unit {
    suite: String,
    source: String,
    report: Result<(OptReport, CodegenReport), String>,
}

#[derive(Serialize)]
pub struct Report {
    schema_version: u32,
    keel: String,
    target: &'static str,
    config: String,
    units: usize,
    failed: Vec<Failure>,
    passes: Vec<PassTotal>,
    inline: Vec<InlineTotal>,
    backend: Vec<BackendTotal>,
}

#[derive(Serialize)]
struct Failure {
    suite: String,
    source: String,
    error: String,
}

#[derive(Serialize, Default, Debug, PartialEq, Eq)]
struct PassTotal {
    pass: String,
    /// The first pipeline position the pass runs at.
    first_index: usize,
    invocations: usize,
    /// Invocations that reported a change.
    changed: usize,
    /// Distinct functions a function-scope invocation changed.
    functions_changed: usize,
    /// Module-scope invocations that reported a change.
    modules_changed: usize,
    /// After minus before, summed over changed invocations.
    instructions_delta: i64,
    blocks_delta: i64,
    block_parameters_delta: i64,
    /// Wall-clock time in the pass, summed over invocations.
    pass_ns: u64,
    /// The same totals for each suite the pass changed.
    by_suite: BTreeMap<String, SuiteTotal>,
}

#[derive(Serialize, Default, Debug, PartialEq, Eq)]
struct SuiteTotal {
    changed: usize,
    functions_changed: usize,
    instructions_delta: i64,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
struct InlineTotal {
    /// `ordinary` or `alwaysinline`.
    category: String,
    /// `inlined`, or the first check that failed.
    outcome: String,
    sites: usize,
}

/// One `--codegen-report` counter, summed over every function.
#[derive(Serialize, Debug, PartialEq, Eq)]
struct BackendTotal {
    counter: String,
    total: u64,
    /// Functions with a nonzero count.
    functions: usize,
    /// The total for each suite.
    by_suite: BTreeMap<String, u64>,
}

/// One function a pass changed, for `--pass`.
#[derive(Debug, PartialEq, Eq)]
struct Site {
    suite: String,
    source: String,
    function: String,
    changed: usize,
    instructions_delta: i64,
}

pub fn run(root: &Path, options: Options, jobs: usize, input_mode: InputMode) -> Result<bool> {
    if input_mode != InputMode::Native {
        bail!("stats preprocesses for its target; use the native input mode");
    }
    let target = match options.target {
        Some(t) => t,
        None => match keel_ir::Target::host().map(|t| t.triple()) {
            Some("aarch64-apple-darwin") => Target::Aarch64,
            Some("x86_64-unknown-linux-gnu") => Target::X86_64,
            _ => bail!("choose a target with --target"),
        },
    };
    let sysroot = codegen_diff::sysroots(
        &[target],
        options.aarch64_sysroot.as_deref(),
        options.x86_64_sysroot.as_deref(),
    )?
    .pop()
    .map(|(_, dir)| dir);
    let keel = match &options.keel {
        Some(path) => path
            .canonicalize()
            .with_context(|| path.display().to_string())?,
        None => build::build_keel(root, true)?,
    };
    let configs = Configs::load(root)?;
    let config = configs.select(std::slice::from_ref(&options.config), &[])?;
    if config[0].compiler != Compiler::Keel {
        bail!(
            "configuration `{}` does not compile with keel",
            options.config
        );
    }
    let all = crate::manifest::load_suites(root)?;
    let chosen = crate::choose(&all, &options.suites)?;
    let env = Env {
        root: root.to_path_buf(),
        keel: keel.clone(),
        jobs,
        input_mode,
    };
    let mut prepared = Vec::new();
    for suite in &chosen {
        prepared.push((*suite, build::sources(&env, suite)?));
    }
    let out = options
        .out
        .clone()
        .unwrap_or_else(|| root.join("target/keel-suite/stats"));
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| out.display().to_string())?;
    }

    let work = codegen_diff::units(&prepared, &[target], &config);
    let progress = bar(work.len(), "stats".into());
    let units = build::parallel(&work, jobs, &progress, |job| {
        compile(root, &keel, sysroot.as_deref(), &out, job)
    });
    progress.finish_and_clear();
    let mut units = units?;
    units.sort_by(|a, b| (&a.suite, &a.source).cmp(&(&b.suite, &b.source)));

    let report = totals(&units, keel.display().to_string(), target, &options.config);
    print_report(&report);
    if let Some(file) = &options.json {
        fs::write(file, serde_json::to_string_pretty(&report)? + "\n")
            .with_context(|| file.display().to_string())?;
    }
    if let Some(pass) = &options.pass {
        if !report.passes.iter().any(|p| &p.pass == pass) {
            bail!("no pass `{pass}` ran in configuration `{}`", options.config);
        }
        print_sites(pass, &sites(&units, pass), options.show);
    }
    Ok(report.failed.is_empty())
}

/// Compiles one translation unit with `--opt-report` and reads the report.
fn compile(
    root: &Path,
    keel: &Path,
    sysroot: Option<&Path>,
    out: &Path,
    job: &Job,
) -> Result<Unit> {
    let dir = out.join(&job.suite.name);
    fs::create_dir_all(&dir)?;
    let stem = job.source.file_stem().unwrap().to_string_lossy();
    let output = dir.join(format!("unit-{}-{stem}.s", job.unit));
    let report = dir.join(format!("unit-{}-{stem}.json", job.unit));
    let codegen = dir.join(format!("unit-{}-{stem}.codegen.json", job.unit));
    let mut argv = job.cc(keel, sysroot, &output);
    argv.extend(["--opt-report".into(), report.display().to_string()]);
    argv.extend(["--codegen-report".into(), codegen.display().to_string()]);
    let result = build::run(&argv, &job.work.join(&job.suite.manifest.build.dir), None)?;
    let report = if result.status.success() {
        let read = |path: &Path| fs::read(path).with_context(|| path.display().to_string());
        let opt = parse(&read(&report)?).with_context(|| report.display().to_string())?;
        let backend =
            parse_codegen(&read(&codegen)?).with_context(|| codegen.display().to_string())?;
        Ok((opt, backend))
    } else {
        Err(build::tail(&result, 5).trim().to_string())
    };
    Ok(Unit {
        suite: job.suite.name.clone(),
        source: job.relative(root),
        report,
    })
}

fn parse(json: &[u8]) -> Result<OptReport> {
    let report: OptReport = serde_json::from_slice(json)?;
    if report.schema_version != 2 {
        bail!(
            "optimization report schema {} (expected 2)",
            report.schema_version
        );
    }
    Ok(report)
}

fn parse_codegen(json: &[u8]) -> Result<CodegenReport> {
    let report: CodegenReport = serde_json::from_slice(json)?;
    if report.schema_version != 1 {
        bail!(
            "codegen report schema {} (expected 1)",
            report.schema_version
        );
    }
    Ok(report)
}

/// The backend counters over every compiled unit, in report order.
fn backend_totals(units: &[Unit]) -> Vec<BackendTotal> {
    let mut totals: Vec<BackendTotal> = Vec::new();
    for unit in units {
        let Ok((_, report)) = &unit.report else {
            continue;
        };
        for function in &report.functions {
            for (name, count) in &function.counters.0 {
                let index = match totals.iter().position(|t| &t.counter == name) {
                    Some(index) => index,
                    None => {
                        totals.push(BackendTotal {
                            counter: name.clone(),
                            total: 0,
                            functions: 0,
                            by_suite: BTreeMap::new(),
                        });
                        totals.len() - 1
                    }
                };
                let total = &mut totals[index];
                total.total += count;
                total.functions += usize::from(*count != 0);
                *total.by_suite.entry(unit.suite.clone()).or_default() += count;
            }
        }
    }
    totals
}

fn totals(units: &[Unit], keel: String, target: Target, config: &str) -> Report {
    let mut passes: BTreeMap<&str, PassTotal> = BTreeMap::new();
    let mut functions: BTreeMap<&str, BTreeSet<(&str, &str, &str)>> = BTreeMap::new();
    let mut inline: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut failed = Vec::new();
    for unit in units {
        let report = match &unit.report {
            Ok((report, _)) => report,
            Err(error) => {
                failed.push(Failure {
                    suite: unit.suite.clone(),
                    source: unit.source.clone(),
                    error: error.clone(),
                });
                continue;
            }
        };
        for inv in &report.invocations {
            let total = passes.entry(&inv.pass).or_insert_with(|| PassTotal {
                pass: inv.pass.clone(),
                first_index: inv.pipeline_index,
                ..PassTotal::default()
            });
            total.first_index = total.first_index.min(inv.pipeline_index);
            total.invocations += 1;
            total.pass_ns += inv.pass_ns;
            for site in &inv.inline_sites {
                *inline.entry((&site.category, &site.outcome)).or_default() += 1;
            }
            if !inv.changed {
                continue;
            }
            let delta = |f: fn(&Counts) -> i64| f(&inv.after) - f(&inv.before);
            total.changed += 1;
            total.instructions_delta += delta(|c| c.instructions);
            total.blocks_delta += delta(|c| c.blocks);
            total.block_parameters_delta += delta(|c| c.block_parameters);
            let suite = total.by_suite.entry(unit.suite.clone()).or_default();
            suite.changed += 1;
            suite.instructions_delta += delta(|c| c.instructions);
            match &inv.function {
                Some(function) => {
                    let key = (unit.suite.as_str(), unit.source.as_str(), function.as_str());
                    if functions.entry(&inv.pass).or_default().insert(key) {
                        total.functions_changed += 1;
                        suite.functions_changed += 1;
                    }
                }
                None => total.modules_changed += 1,
            }
        }
    }
    let mut passes: Vec<PassTotal> = passes.into_values().collect();
    passes.sort_by(|a, b| (a.first_index, &a.pass).cmp(&(b.first_index, &b.pass)));
    Report {
        schema_version: 1,
        keel,
        target: target.triple(),
        config: config.to_string(),
        units: units.len(),
        failed,
        passes,
        inline: inline
            .into_iter()
            .map(|((category, outcome), sites)| InlineTotal {
                category: category.to_string(),
                outcome: outcome.to_string(),
                sites,
            })
            .collect(),
        backend: backend_totals(units),
    }
}

/// The functions `pass` changed, largest instruction change first.
fn sites(units: &[Unit], pass: &str) -> Vec<Site> {
    let mut sites: BTreeMap<(&str, &str, &str), (usize, i64)> = BTreeMap::new();
    for unit in units {
        let Ok((report, _)) = &unit.report else {
            continue;
        };
        for inv in &report.invocations {
            let Some(function) = inv.function.as_deref().filter(|_| inv.changed) else {
                continue;
            };
            if inv.pass != pass {
                continue;
            }
            let site = sites
                .entry((&unit.suite, &unit.source, function))
                .or_default();
            site.0 += 1;
            site.1 += inv.after.instructions - inv.before.instructions;
        }
    }
    let mut sites: Vec<Site> = sites
        .into_iter()
        .map(|((suite, source, function), (changed, delta))| Site {
            suite: suite.into(),
            source: source.into(),
            function: function.into(),
            changed,
            instructions_delta: delta,
        })
        .collect();
    sites.sort_by_key(|s| std::cmp::Reverse(s.instructions_delta.unsigned_abs()));
    sites
}

fn signed(n: i64) -> String {
    if n > 0 {
        format!("+{n}")
    } else {
        n.to_string()
    }
}

fn print_report(report: &Report) {
    println!(
        "{} {}  {} {}  {} units",
        paint("keel", Paint::Bold),
        report.keel,
        report.target,
        report.config,
        report.units
    );
    println!();
    if report.passes.is_empty() {
        println!("  {}", paint("no optimizer passes ran", Paint::Dim));
    }
    let header = [
        "pass",
        "runs",
        "changed",
        "functions",
        "modules",
        "insts",
        "blocks",
        "params",
        "time",
    ]
    .map(|h| Cell::new(h, Paint::Bold))
    .to_vec();
    let rows = report
        .passes
        .iter()
        .map(|p| {
            let fired = if p.changed == 0 {
                Paint::Dim
            } else {
                Paint::Plain
            };
            vec![
                Cell::new(&p.pass, fired),
                Cell::plain(p.invocations.to_string()),
                Cell::new(p.changed.to_string(), fired),
                Cell::plain(p.functions_changed.to_string()),
                Cell::plain(p.modules_changed.to_string()),
                Cell::plain(signed(p.instructions_delta)),
                Cell::plain(signed(p.blocks_delta)),
                Cell::plain(signed(p.block_parameters_delta)),
                Cell::plain(crate::style::ms(p.pass_ns as f64 / 1e6)),
            ]
        })
        .collect();
    if !report.passes.is_empty() {
        print_table(header, rows);
    }

    if !report.inline.is_empty() {
        println!();
        println!("{}", paint("inline sites", Paint::Heading));
        let header = ["outcome", "ordinary", "alwaysinline"]
            .map(|h| Cell::new(h, Paint::Bold))
            .to_vec();
        let mut outcomes: Vec<&str> = report
            .inline
            .iter()
            .map(|i| i.outcome.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        outcomes.sort_by_key(|o| *o != "inlined");
        let count = |outcome: &str, category: &str| {
            report
                .inline
                .iter()
                .find(|i| i.outcome == outcome && i.category == category)
                .map_or(0, |i| i.sites)
        };
        let rows = outcomes
            .iter()
            .map(|o| {
                vec![
                    Cell::plain(*o),
                    Cell::plain(count(o, "ordinary").to_string()),
                    Cell::plain(count(o, "alwaysinline").to_string()),
                ]
            })
            .collect();
        print_table(header, rows);
    }

    if !report.backend.is_empty() {
        println!();
        println!("{}", paint("backend", Paint::Heading));
        let suites: Vec<&str> = report
            .backend
            .iter()
            .flat_map(|b| b.by_suite.keys().map(String::as_str))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let header = ["counter", "total", "functions"]
            .into_iter()
            .chain(suites.iter().copied())
            .map(|h| Cell::new(h, Paint::Bold))
            .collect();
        let rows = report
            .backend
            .iter()
            .map(|b| {
                let fired = if b.total == 0 {
                    Paint::Dim
                } else {
                    Paint::Plain
                };
                [
                    Cell::new(&b.counter, fired),
                    Cell::new(b.total.to_string(), fired),
                    Cell::plain(b.functions.to_string()),
                ]
                .into_iter()
                .chain(
                    suites
                        .iter()
                        .map(|s| Cell::plain(b.by_suite.get(*s).copied().unwrap_or(0).to_string())),
                )
                .collect()
            })
            .collect();
        print_table(header, rows);
    }

    println!();
    for f in &report.failed {
        println!(
            "{} {} {}: {}",
            paint("failed", Paint::Red),
            f.suite,
            f.source,
            f.error.lines().last().unwrap_or("")
        );
    }
    if report.failed.is_empty() {
        println!("{} all {} units compiled", ok_mark(), report.units);
    } else {
        println!(
            "{} {} of {} units failed to compile",
            fail_mark(),
            report.failed.len(),
            report.units
        );
    }
}

fn print_sites(pass: &str, sites: &[Site], show: usize) {
    println!();
    println!(
        "{}",
        paint(
            format!("{pass} changed {} functions", sites.len()),
            Paint::Heading
        )
    );
    let header = ["function", "suite", "source", "changed", "insts"]
        .map(|h| Cell::new(h, Paint::Bold))
        .to_vec();
    let rows = sites
        .iter()
        .take(show)
        .map(|s| {
            vec![
                Cell::plain(&s.function),
                Cell::plain(&s.suite),
                Cell::plain(&s.source),
                Cell::plain(s.changed.to_string()),
                Cell::plain(signed(s.instructions_delta)),
            ]
        })
        .collect();
    print_table(header, rows);
    if sites.len() > show {
        println!(
            "  {}",
            paint(format!("and {} more", sites.len() - show), Paint::Dim)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invocation(index: usize, pass: &str, function: Option<&str>, changed: bool) -> String {
        let function = function.map_or("null".to_string(), |f| format!("\"{f}\""));
        format!(
            r#"{{"pipeline_index":{index},"pass":"{pass}","target":"aarch64-neon","scope":"function","function":{function},"changed":{changed},"pass_ns":1000,"verification_enabled":false,"verification_ns":0,"restoration_ns":0,"restoration_verification_ns":0,"restoration_changed":null,"before":{{"functions":1,"blocks":4,"instructions":20,"block_parameters":2,"stack_declarations":0,"stack_declaration_bytes":0}},"after":{{"functions":1,"blocks":3,"instructions":{after},"block_parameters":1,"stack_declarations":0,"stack_declaration_bytes":0}},"inline_sites":[]}}"#,
            after = if changed { 17 } else { 20 },
        )
    }

    fn unit(suite: &str, source: &str, invocations: &[String], sites: &str) -> Unit {
        let mut json = format!(
            r#"{{"schema_version":2,"invocations":[{}]}}"#,
            invocations.join(",")
        );
        if !sites.is_empty() {
            json = json.replacen(
                r#""inline_sites":[]"#,
                &format!(r#""inline_sites":[{sites}]"#),
                1,
            );
        }
        unit_with_backend(suite, source, &json, &[])
    }

    /// A unit whose codegen report lists one function per `(spills, moves)`.
    fn unit_with_backend(suite: &str, source: &str, opt: &str, functions: &[(u64, u64)]) -> Unit {
        let functions: Vec<String> = functions
            .iter()
            .enumerate()
            .map(|(n, (spills, moves))| {
                format!(
                    r#"{{"name":"f{n}","counters":{{"spill_stores":{spills},"register_moves":{moves},"cover_fired":0}}}}"#
                )
            })
            .collect();
        let codegen = format!(
            r#"{{"schema_version":1,"target":"aarch64-apple-darwin","counters":{{}},"functions":[{}]}}"#,
            functions.join(",")
        );
        Unit {
            suite: suite.into(),
            source: source.into(),
            report: Ok((
                parse(opt.as_bytes()).unwrap(),
                parse_codegen(codegen.as_bytes()).unwrap(),
            )),
        }
    }

    #[test]
    fn totals_backend_counters_in_report_order() {
        let empty = r#"{"schema_version":2,"invocations":[]}"#;
        let units = vec![
            unit_with_backend("lua", "a.c", empty, &[(2, 5), (0, 1)]),
            unit_with_backend("lz4", "b.c", empty, &[(3, 0)]),
            Unit {
                suite: "lz4".into(),
                source: "c.c".into(),
                report: Err("error: unsupported".into()),
            },
        ];
        let backend = backend_totals(&units);
        let names: Vec<&str> = backend.iter().map(|b| b.counter.as_str()).collect();
        assert_eq!(names, ["spill_stores", "register_moves", "cover_fired"]);
        assert_eq!(
            backend[0],
            BackendTotal {
                counter: "spill_stores".into(),
                total: 5,
                functions: 2,
                by_suite: BTreeMap::from([("lua".into(), 2), ("lz4".into(), 3)]),
            }
        );
        assert_eq!((backend[1].total, backend[1].functions), (6, 2));
        assert_eq!((backend[2].total, backend[2].functions), (0, 0));
    }

    #[test]
    fn totals_passes_in_pipeline_order() {
        let site = |outcome: &str| {
            format!(
                r#"{{"caller":"f","callee":"g","site":3,"category":"ordinary","outcome":"{outcome}"}}"#
            )
        };
        let units = vec![
            unit(
                "lua",
                "a.c",
                &[
                    invocation(0, "inline", None, true),
                    invocation(1, "sccp", Some("f"), true),
                    invocation(2, "dce", Some("f"), false),
                    invocation(3, "sccp", Some("f"), true),
                ],
                &[site("inlined"), site("noinline"), site("inlined")].join(","),
            ),
            unit("lz4", "b.c", &[invocation(1, "sccp", Some("f"), true)], ""),
            Unit {
                suite: "lz4".into(),
                source: "c.c".into(),
                report: Err("error: unsupported".into()),
            },
        ];
        let report = totals(&units, "keel".into(), Target::Aarch64, "keel-O");
        let names: Vec<&str> = report.passes.iter().map(|p| p.pass.as_str()).collect();
        assert_eq!(names, ["inline", "sccp", "dce"]);
        let sccp = &report.passes[1];
        assert_eq!(
            (sccp.invocations, sccp.changed, sccp.functions_changed),
            (3, 3, 2),
            "a function changed twice counts once"
        );
        assert_eq!(
            (
                sccp.instructions_delta,
                sccp.blocks_delta,
                sccp.block_parameters_delta
            ),
            (-9, -3, -3)
        );
        assert_eq!(sccp.by_suite["lua"].functions_changed, 1);
        assert_eq!(sccp.by_suite["lz4"].instructions_delta, -3);
        assert_eq!(report.passes[0].modules_changed, 1);
        let dce = &report.passes[2];
        assert_eq!((dce.changed, dce.instructions_delta), (0, 0));
        assert!(dce.by_suite.is_empty());
        assert_eq!(
            report.inline,
            [
                InlineTotal {
                    category: "ordinary".into(),
                    outcome: "inlined".into(),
                    sites: 2
                },
                InlineTotal {
                    category: "ordinary".into(),
                    outcome: "noinline".into(),
                    sites: 1
                },
            ]
        );
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.units, 3);

        let sccp_sites = sites(&units, "sccp");
        assert_eq!(sccp_sites.len(), 2);
        assert_eq!(
            sccp_sites[0],
            Site {
                suite: "lua".into(),
                source: "a.c".into(),
                function: "f".into(),
                changed: 2,
                instructions_delta: -6,
            }
        );
    }

    #[test]
    fn rejects_other_report_schemas() {
        let error = parse(br#"{"schema_version":3,"invocations":[]}"#)
            .err()
            .unwrap();
        assert!(error.to_string().contains("schema 3"));
        let error = parse_codegen(br#"{"schema_version":2,"functions":[]}"#)
            .err()
            .unwrap();
        assert!(error.to_string().contains("schema 2"));
    }
}

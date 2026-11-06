//! `cargo suite ab`: times the suite benchmarks built by two keel compilers
//! against each other and gives a verdict by the acceptance rule.
//!
//! Both compilers build every chosen suite with one configuration, and the
//! suites' tests and benchmark outputs are checked before any timing. Each
//! further placement reassembles both builds with never-executed padding in
//! front of every function, chosen per function name and placement, so each
//! function's offset in its cache line and its distance to the others change.
//! `suite/c/ab_timing.py` then runs the baseline, the candidate and an
//! identical copy of the baseline (the control) in mirrored blocks, under
//! `perf stat` on Linux and `/usr/bin/time -l` on macOS. This module turns
//! those runs into per-placement ratios and the verdict.

use crate::build::{self, Env, InputMode};
use crate::counters;
use crate::manifest::{self, Bench, Compiler, Config, Configs, Suite};
use crate::run as suite_run;
use crate::style::{Cell, Paint, paint, print_table, spinner};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The candidate is accepted at a cycles geomean of -0.5% or better.
const ACCEPT_GEOMEAN: f64 = 0.995;
/// It is rejected if a workload is slower than +1.5% at every placement.
const WORKLOAD_LIMIT: f64 = 1.015;
/// Padding before a function stays below a cache line.
const PAD_LIMIT: usize = 64;
/// The placeholders `ab_timing.py` replaces with the image and counter file.
const BIN: &str = "@BIN@";
const COUNTERS: &str = "@COUNTERS@";

const BASELINE: usize = 0;
const CANDIDATE: usize = 1;
const CONTROL: usize = 2;
const ARMS: [&str; 3] = ["baseline", "candidate", "control"];

#[derive(clap::Args)]
pub struct Options {
    /// The keel binary or git revision to compare against.
    #[arg(long)]
    baseline: String,
    /// The keel binary or git revision under test.
    #[arg(long)]
    candidate: String,
    /// Suites or `suite/benchmark`s to time, comma-separated (default: all).
    #[arg(long, value_delimiter = ',')]
    apps: Vec<String>,
    /// The keel configuration both compilers build with.
    #[arg(short, long, default_value = "keel-O")]
    config: String,
    /// Also pass `--isel LIST` to the candidate, so one binary can compare
    /// selection choices, for example `all,-x86-mask-test`.
    #[arg(long, value_name = "LIST")]
    candidate_isel: Option<String>,
    /// Code placements; each further one pads every function differently.
    #[arg(long, default_value_t = 4)]
    placements: usize,
    /// Measured rounds per placement; a round runs each image twice.
    #[arg(short = 'n', long, default_value_t = 5)]
    rounds: u32,
    /// Unmeasured rounds before them.
    #[arg(short, long, default_value_t = 1)]
    warmup: u32,
    /// More perf events to count on Linux, comma-separated.
    #[arg(long, value_delimiter = ',')]
    counters: Vec<String>,
    /// CPU to run the benchmarks on (Linux).
    #[arg(long)]
    cpu: Option<usize>,
    /// CPU for the timing controller (Linux).
    #[arg(long)]
    controller_cpu: Option<usize>,
    /// Do not lock the executables' pages in memory (Linux).
    #[arg(long)]
    no_lock: bool,
    /// Skip the suites' test commands; benchmark outputs are still checked.
    #[arg(long)]
    no_tests: bool,
    /// Seed of the run order.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Python interpreter for the timing loop.
    #[arg(long, default_value = "python3")]
    python: PathBuf,
}

/// One of the two compilers.
#[derive(Serialize)]
struct Keel {
    /// The revision or file it was given as.
    label: String,
    path: PathBuf,
    sha256: String,
}

#[derive(Serialize)]
struct Plan {
    schema: u32,
    platform: &'static str,
    slots: &'static str,
    lock: bool,
    cpu: Option<usize>,
    controller_cpu: Option<usize>,
    counter: Vec<String>,
    rounds: u32,
    warmup: u32,
    seed: u64,
    work: String,
    arms: [&'static str; 3],
    apps: Vec<PlanApp>,
}

#[derive(Serialize)]
struct PlanApp {
    suite: String,
    /// The image of each arm at each placement.
    placements: Vec<BTreeMap<&'static str, String>>,
    benches: Vec<PlanBench>,
}

#[derive(Serialize)]
struct PlanBench {
    name: String,
    argv: Vec<String>,
    cwd: String,
    stdin: Option<String>,
    expected: Expected,
}

#[derive(Serialize)]
struct Expected {
    status: i32,
    stdout_sha256: String,
    stderr_sha256: String,
}

#[derive(Deserialize)]
struct Runs {
    runs: Vec<Timed>,
}

/// One timed run, as `ab_timing.py` writes it.
#[derive(Deserialize)]
struct Timed {
    suite: String,
    bench: String,
    round: usize,
    placement: usize,
    arm: String,
    wall_ns: u64,
    counters: String,
}

/// Each arm's sum of one metric over the two runs of a block:
/// baseline, candidate and control.
type Block = [f64; 3];

/// One workload's blocks of cycles, instructions and wall time, by
/// placement and then round.
struct Series {
    name: String,
    metrics: [Vec<Vec<Block>>; 3],
}

/// A ratio to the baseline over all placements.
#[derive(Serialize, Clone, Debug, PartialEq)]
struct Change {
    /// The geometric mean of the placements' ratios.
    ratio: f64,
    /// The median over rounds of each placement's block ratio.
    placements: Vec<f64>,
}

impl Change {
    fn low(&self) -> f64 {
        self.placements
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
    }

    fn high(&self) -> f64 {
        self.placements
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The largest distance of a placement from no change.
    fn reach(&self) -> f64 {
        self.placements
            .iter()
            .map(|r| (r - 1.0).abs())
            .fold(0.0, f64::max)
    }
}

#[derive(Serialize)]
struct Workload {
    name: String,
    cycles: Change,
    control: Change,
    instructions: Option<Change>,
    wall: Change,
    /// The baseline's median cycles at each placement over the first's.
    baseline_placements: Vec<f64>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Decision {
    Accept,
    Neutral,
    Reject,
}

#[derive(Serialize)]
struct Verdict {
    /// The cycles geomean over workloads.
    geomean: f64,
    /// The control's geomean over workloads.
    control: f64,
    /// The control's largest distance from no change, overall or at one placement.
    control_spread: f64,
    /// Workloads slower than the limit at every placement.
    blocking: Vec<String>,
    decision: Decision,
    reason: String,
}

#[derive(Serialize)]
struct Report {
    schema: u32,
    baseline: Keel,
    candidate: Keel,
    config: String,
    host: String,
    metric: &'static str,
    placements: usize,
    rounds: u32,
    warmup: u32,
    /// Suites whose baseline and candidate images are byte-identical.
    identical_images: Vec<String>,
    workloads: Vec<Workload>,
    verdict: Verdict,
    trailer: String,
}

pub fn run(root: &Path, options: Options, jobs: usize, input_mode: InputMode) -> Result<bool> {
    if options.placements == 0 || options.rounds == 0 {
        bail!("timing needs at least one placement and one round");
    }
    let dir = root.join("target/keel-suite/ab");
    let baseline = resolve(root, &dir, &options.baseline)?;
    let candidate = resolve(root, &dir, &options.candidate)?;
    let configs = Configs::load(root)?;
    let config = configs.get(&options.config)?;
    if config.compiler != Compiler::Keel {
        bail!("`{}` is not a keel configuration", config.name);
    }
    let all = manifest::load_suites(root)?;
    let chosen = crate::choose_benches(&all, &options.apps)?;
    if chosen.is_empty() {
        bail!("no benchmarks chosen");
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let out = dir.join("runs").join(stamp.to_string());
    fs::create_dir_all(&out)?;

    let mut apps = Vec::new();
    let mut identical = Vec::new();
    for (suite, benches) in &chosen {
        let (app, same) = prepare(
            root,
            suite,
            benches,
            [&baseline, &candidate],
            config,
            &options,
            jobs,
            input_mode,
        )?;
        if same {
            identical.push(suite.name.clone());
        }
        apps.push(app);
    }

    let linux = cfg!(target_os = "linux");
    let mut events = vec!["cycles:u".to_string(), "instructions:u".to_string()];
    events.extend(options.counters.iter().cloned());
    let events: Vec<&str> = events.iter().map(String::as_str).collect();
    let plan = Plan {
        schema: 1,
        platform: if linux { "linux" } else { "macos" },
        slots: if linux { "fixed-inode" } else { "copies" },
        lock: linux && !options.no_lock,
        cpu: options.cpu,
        controller_cpu: options.controller_cpu,
        counter: counters::command(&events, COUNTERS),
        rounds: options.rounds,
        warmup: options.warmup,
        seed: options.seed,
        work: out.join("slots").display().to_string(),
        arms: ARMS,
        apps,
    };
    let plan_file = out.join("plan.json");
    let runs_file = out.join("runs.json");
    fs::write(&plan_file, serde_json::to_string_pretty(&plan)?)?;
    println!();
    println!(
        "{}",
        paint(
            format!(
                "timing {} placements x {} rounds ({} warmup), plan {}",
                options.placements,
                options.rounds,
                options.warmup,
                shown(root, &plan_file)
            ),
            Paint::Dim
        )
    );
    let status = Command::new(&options.python)
        .arg(root.join("suite/c/ab_timing.py"))
        .args([&plan_file, &runs_file])
        .current_dir(root)
        .status()
        .with_context(|| format!("running {}", options.python.display()))?;
    if !status.success() {
        bail!("the timing loop failed ({})", suite_run::status(status));
    }

    let runs: Runs = serde_json::from_str(&fs::read_to_string(&runs_file)?)
        .with_context(|| format!("reading {}", runs_file.display()))?;
    let order: Vec<String> = plan
        .apps
        .iter()
        .flat_map(|app| {
            app.benches
                .iter()
                .map(|b| format!("{}/{}", app.suite, b.name))
        })
        .collect();
    let series = collect(
        &runs.runs,
        &order,
        options.placements,
        options.rounds as usize,
    )?;
    let workloads: Vec<Workload> = series.iter().map(summarize).collect::<Result<_>>()?;
    let verdict = verdict(&workloads);
    let metric = if linux {
        "cycles:u"
    } else {
        "cycles (time -l, user and kernel)"
    };
    let host = host();
    let trailer = trailer(
        [&baseline, &candidate],
        &host,
        metric,
        &config.name,
        &options,
        &workloads,
        &verdict,
    );
    print_report(&workloads, &verdict, &identical, &trailer);
    let report = Report {
        schema: 1,
        baseline,
        candidate,
        config: config.name.clone(),
        host,
        metric,
        placements: options.placements,
        rounds: options.rounds,
        warmup: options.warmup,
        identical_images: identical,
        workloads,
        verdict,
        trailer,
    };
    let report_file = out.join("report.json");
    fs::write(&report_file, serde_json::to_string_pretty(&report)? + "\n")?;
    println!(
        "{}",
        paint(format!("saved {}", shown(root, &report_file)), Paint::Dim)
    );
    Ok(true)
}

fn shown(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}

/// A keel binary, or a release build of a revision, cached by commit.
fn resolve(root: &Path, dir: &Path, spec: &str) -> Result<Keel> {
    let file = Path::new(spec);
    if file.is_file() {
        let path = file.canonicalize()?;
        let sha256 = file_sha256(&path)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let label = format!("{name} (sha256 {})", &sha256[..12]);
        return Ok(Keel {
            label,
            path,
            sha256,
        });
    }
    let commit = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{spec}^{{commit}}"),
        ],
    )
    .with_context(|| format!("`{spec}` is neither a keel binary nor a git revision"))?;
    let dest = dir.join(format!("keel-{}", &commit[..12]));
    let path = dest.join("keel");
    if !path.is_file() {
        build_revision(root, &commit, &dest)?;
    }
    Ok(Keel {
        label: commit[..10].to_string(),
        sha256: file_sha256(&path)?,
        path,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        bail!("`git {}` failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Builds the keel driver of `commit` in its own source and target directory.
fn build_revision(root: &Path, commit: &str, dest: &Path) -> Result<()> {
    let source = dest.join("src");
    if source.exists() {
        fs::remove_dir_all(&source)?;
    }
    fs::create_dir_all(&source)?;
    let progress = spinner(format!("building keel at {}", &commit[..10]));
    let mut archive = Command::new("git")
        .args(["archive", commit])
        .current_dir(root)
        .stdout(Stdio::piped())
        .spawn()?;
    let unpacked = Command::new("tar")
        .args(["-x", "-f", "-"])
        .current_dir(&source)
        .stdin(archive.stdout.take().unwrap())
        .status()?;
    if !archive.wait()?.success() || !unpacked.success() {
        bail!("unpacking {commit} failed");
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let target = dest.join("target");
    let built = Command::new(cargo)
        .args(["build", "-q", "--release", "-p", "keel"])
        .current_dir(&source)
        .env("CARGO_TARGET_DIR", &target)
        .status()?;
    progress.finish_and_clear();
    if !built.success() {
        bail!("building keel at {commit} failed");
    }
    fs::copy(target.join("release/keel"), dest.join("keel"))?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    Ok(hex(&Sha256::digest(fs::read(path)?)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds a suite with both compilers, checks it, relinks the placements
/// and records the benchmarks' expected outputs. Also says whether the two
/// images are byte-identical.
#[allow(clippy::too_many_arguments)]
fn prepare(
    root: &Path,
    suite: &Suite,
    benches: &[&Bench],
    keels: [&Keel; 2],
    config: &Config,
    options: &Options,
    jobs: usize,
    input_mode: InputMode,
) -> Result<(PlanApp, bool)> {
    suite_run::heading(suite);
    let env = |keel: &Keel| Env {
        root: root.to_path_buf(),
        keel: keel.path.clone(),
        jobs,
        input_mode,
    };
    let sources = build::sources(&env(keels[0]), suite)?;
    // Equal-length names keep the two build directories' paths the same length.
    let mut bins = Vec::new();
    for (keel, prefix) in keels.iter().zip(["ab-base", "ab-cand"]) {
        let mut flags = config.flags.clone();
        if let Some(list) = options
            .candidate_isel
            .as_ref()
            .filter(|_| prefix == "ab-cand")
        {
            flags.extend(["--isel".into(), list.clone()]);
        }
        let config = Config {
            name: format!("{prefix}-{}", config.name),
            compiler: Compiler::Keel,
            flags,
        };
        let built = build::build(&env(keel), suite, &sources, &config)?;
        println!(
            "  {} built with {}",
            crate::style::ok_mark(),
            paint(&keel.label, Paint::Dim)
        );
        bins.push(built.bin);
    }
    let bins = [bins[0].clone(), bins[1].clone()];
    let identical = fs::read(&bins[0])? == fs::read(&bins[1])?;
    if let Some(test) = suite.manifest.test.as_ref().filter(|_| !options.no_tests) {
        let mut expected: Vec<_> = test.run.iter().map(|_| None).collect();
        suite_run::run_tests(suite, &sources, "baseline", &bins[0], None, &mut expected)?;
        let reference = test
            .compare
            .as_ref()
            .map(|_| ("the baseline", bins[0].as_path()));
        suite_run::run_tests(
            suite,
            &sources,
            "candidate",
            &bins[1],
            reference,
            &mut expected,
        )?;
        println!("  {} tests passed", crate::style::ok_mark());
    }
    let mut placements = vec![bins.clone()];
    for k in 1..options.placements {
        placements.push(relink(suite, &sources, &bins, k, jobs)?);
    }
    let mut planned = Vec::new();
    for bench in benches {
        planned.push(plan_bench(suite, bench, &sources.work, &bins)?);
    }
    println!(
        "  {} {} checked, {} placements",
        crate::style::ok_mark(),
        benches.len(),
        placements.len()
    );
    let placements = placements
        .iter()
        .map(|[base, cand]| {
            let base = base.display().to_string();
            BTreeMap::from([
                (ARMS[BASELINE], base.clone()),
                (ARMS[CANDIDATE], cand.display().to_string()),
                (ARMS[CONTROL], base),
            ])
        })
        .collect();
    let app = PlanApp {
        suite: suite.name.clone(),
        placements,
        benches: planned,
    };
    Ok((app, identical))
}

/// Reassembles and relinks both builds with each function padded for
/// `placement`.
fn relink(
    suite: &Suite,
    sources: &build::Sources,
    bins: &[PathBuf; 2],
    placement: usize,
    jobs: usize,
) -> Result<[PathBuf; 2]> {
    // Steps of the functions' own alignment: 16 bytes on x86-64 and one
    // instruction on AArch64.
    let (fill, step) = if cfg!(target_arch = "x86_64") {
        ("0xcc", 16)
    } else {
        ("0", 4)
    };
    let units: Vec<_> = (0..sources.pre.len()).collect();
    let mut padded = Vec::new();
    for bin in bins {
        let dir = bin.parent().unwrap();
        let objects = build::parallel(&units, jobs, &indicatif::ProgressBar::hidden(), |i| {
            let asm = dir.join(format!("unit-{i}.s"));
            let text = fs::read_to_string(&asm).with_context(|| {
                format!(
                    "reading {}; placements need the configuration's assembly output",
                    asm.display()
                )
            })?;
            let text = pad_functions(&text, placement, fill, step)
                .with_context(|| asm.display().to_string())?;
            let stem = format!("unit-{i}.place{placement}");
            let asm = dir.join(format!("{stem}.s"));
            let object = dir.join(format!("{stem}.o"));
            fs::write(&asm, text)?;
            let argv = ["clang", "-c", &path(&asm), "-o", &path(&object)].map(String::from);
            build::run_checked(&argv, dir, None)?;
            Ok(object)
        })?;
        let name = format!("{}.place{placement}", suite.manifest.build.binary);
        let out = dir.join(name);
        build::link(suite, objects, &out, dir)?;
        if fs::read(&out)? == fs::read(bin)? {
            bail!("placement {placement} did not change {}", bin.display());
        }
        padded.push(out);
    }
    Ok([padded[0].clone(), padded[1].clone()])
}

fn path(file: &Path) -> String {
    file.display().to_string()
}

/// Puts never-executed `fill` in front of the alignment of every function in
/// keel's assembly. A function is an entry label followed by its
/// `L<name>_begin` label (`.L` on ELF). Its padding is a multiple of `step`
/// below `PAD_LIMIT`, chosen from its name and `placement` alone, so both
/// builds pad a function the same way.
fn pad_functions(asm: &str, placement: usize, fill: &str, step: usize) -> Result<String> {
    let lines: Vec<&str> = asm.lines().collect();
    let mut pads = BTreeMap::new();
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        let Some(name) = line
            .strip_suffix("_begin:")
            .and_then(|l| l.strip_prefix(".L").or_else(|| l.strip_prefix('L')))
        else {
            continue;
        };
        if i < 2 || !lines[i - 1].trim_end().ends_with(':') {
            continue;
        }
        let align = (0..i - 1)
            .rev()
            .map_while(|j| {
                let line = lines[j].trim();
                let directive = line.starts_with('.') && !line.starts_with(".section");
                let comment = line.starts_with(';') || line.starts_with('#');
                (directive || comment).then_some(j)
            })
            .find(|&j| {
                let line = lines[j].trim();
                line.starts_with(".p2align") || line.starts_with(".align")
            })
            .with_context(|| format!("function {name} has no alignment directive"))?;
        let digest = Sha256::digest(format!("{placement}:{name}"));
        pads.insert(align, usize::from(digest[0]) % (PAD_LIMIT / step) * step);
    }
    let mut out = String::with_capacity(asm.len() + pads.len() * 24);
    for (i, line) in lines.iter().enumerate() {
        if let Some(&bytes) = pads.get(&i).filter(|&&bytes| bytes > 0) {
            out.push_str(&format!("\t.skip {bytes}, {fill}\n"));
        }
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

/// Runs a benchmark once with each build, checks the candidate prints what
/// the baseline prints, and gives the plan's command and expected output.
fn plan_bench(suite: &Suite, bench: &Bench, work: &Path, bins: &[PathBuf; 2]) -> Result<PlanBench> {
    let name = format!("{}/{}", suite.name, bench.name);
    let once = |bin: &Path| {
        let (argv, stdin) = suite_run::invocation(suite, bench, work, bin)?;
        build::run(&argv, work, stdin.as_deref())
    };
    let expected = once(&bins[0])?;
    if !expected.status.success() {
        bail!("{name} with the baseline {}", suite_run::failure(&expected));
    }
    if let Some(diff) = suite_run::difference(&expected, &once(&bins[1])?) {
        bail!("{name} printed differently with the candidate:\n{diff}");
    }
    let (argv, stdin) = suite_run::invocation(suite, bench, work, Path::new(BIN))?;
    Ok(PlanBench {
        name: bench.name.clone(),
        argv,
        cwd: work.display().to_string(),
        stdin: stdin.map(|s| s.display().to_string()),
        expected: Expected {
            status: 0,
            stdout_sha256: hex(&Sha256::digest(&expected.stdout)),
            stderr_sha256: hex(&Sha256::digest(&expected.stderr)),
        },
    })
}

/// Sums each arm's runs of a block, for every workload in `order`.
fn collect(
    runs: &[Timed],
    order: &[String],
    placements: usize,
    rounds: usize,
) -> Result<Vec<Series>> {
    let mut series: Vec<Series> = order
        .iter()
        .map(|name| Series {
            name: name.clone(),
            metrics: std::array::from_fn(|_| vec![vec![[0.0; 3]; rounds]; placements]),
        })
        .collect();
    let mut counts: BTreeMap<(usize, usize, usize, usize), usize> = BTreeMap::new();
    for timed in runs {
        let name = format!("{}/{}", timed.suite, timed.bench);
        let Some(index) = order.iter().position(|n| *n == name) else {
            bail!("the timing loop ran `{name}`, which the plan does not have");
        };
        let Some(arm) = ARMS.iter().position(|a| *a == timed.arm) else {
            bail!("unknown arm `{}`", timed.arm);
        };
        if timed.placement >= placements || timed.round >= rounds {
            bail!(
                "{name}: placement {} round {} is out of range",
                timed.placement,
                timed.round
            );
        }
        let counts_of_run = counters::parse(&timed.counters);
        let Some(&cycles) = counts_of_run.get("cycles") else {
            bail!(
                "{name}: no cycle count; check that the counter tool works (perf_event_paranoid on Linux)"
            );
        };
        let values = [
            Some(cycles as f64),
            counts_of_run.get("instructions").map(|&n| n as f64),
            Some(timed.wall_ns as f64),
        ];
        for (metric, value) in values.into_iter().enumerate() {
            series[index].metrics[metric][timed.placement][timed.round][arm] +=
                value.unwrap_or(f64::NAN);
        }
        *counts
            .entry((index, timed.placement, timed.round, arm))
            .or_default() += 1;
    }
    for (index, s) in series.iter().enumerate() {
        for placement in 0..placements {
            for round in 0..rounds {
                for (arm, label) in ARMS.iter().enumerate() {
                    if counts.get(&(index, placement, round, arm)) != Some(&2) {
                        bail!(
                            "{}: placement {placement} round {round} does not have two {label} runs",
                            s.name
                        );
                    }
                }
            }
        }
    }
    Ok(series)
}

/// Each placement's median block ratio of `arm` to the baseline.
fn change(blocks: &[Vec<Block>], arm: usize) -> Option<Change> {
    let placements = blocks
        .iter()
        .map(|rounds| {
            let base: Vec<f64> = rounds.iter().map(|b| b[BASELINE]).collect();
            let other: Vec<f64> = rounds.iter().map(|b| b[arm]).collect();
            suite_run::paired_ratio(&other, &base).map(|r| r.median)
        })
        .collect::<Option<Vec<f64>>>()?;
    Some(Change {
        ratio: geomean(&placements),
        placements,
    })
}

fn summarize(series: &Series) -> Result<Workload> {
    let [cycles, instructions, wall] = &series.metrics;
    let missing = || anyhow::anyhow!("{}: a run has no positive count", series.name);
    let medians: Vec<f64> = cycles
        .iter()
        .map(|rounds| {
            suite_run::median(&mut rounds.iter().map(|b| b[BASELINE]).collect::<Vec<_>>())
        })
        .collect();
    Ok(Workload {
        name: series.name.clone(),
        cycles: change(cycles, CANDIDATE).ok_or_else(missing)?,
        control: change(cycles, CONTROL).ok_or_else(missing)?,
        instructions: change(instructions, CANDIDATE),
        wall: change(wall, CANDIDATE).ok_or_else(missing)?,
        baseline_placements: medians.iter().map(|m| m / medians[0]).collect(),
    })
}

fn geomean(ratios: &[f64]) -> f64 {
    (ratios.iter().map(|r| r.ln()).sum::<f64>() / ratios.len() as f64).exp()
}

fn percent(ratio: f64) -> String {
    format!("{:+.2}%", (ratio - 1.0) * 100.0)
}

/// The acceptance rule: accept at a cycles geomean of -0.5% or better
/// unless a workload is slower than +1.5% at every placement and beyond its
/// control; a geomean inside the identical-control spread is neutral.
fn verdict(workloads: &[Workload]) -> Verdict {
    let ratios: Vec<f64> = workloads.iter().map(|w| w.cycles.ratio).collect();
    let geomean_ratio = geomean(&ratios);
    let controls: Vec<f64> = workloads.iter().map(|w| w.control.ratio).collect();
    let control = geomean(&controls);
    let placements = workloads.first().map_or(0, |w| w.control.placements.len());
    let mut control_spread = (control - 1.0).abs();
    for placement in 0..placements {
        let at: Vec<f64> = workloads
            .iter()
            .map(|w| w.control.placements[placement])
            .collect();
        control_spread = control_spread.max((geomean(&at) - 1.0).abs());
    }
    let blocking: Vec<String> = workloads
        .iter()
        .filter(|w| {
            let low = w.cycles.low();
            low > WORKLOAD_LIMIT && low - 1.0 > w.control.reach()
        })
        .map(|w| w.name.clone())
        .collect();
    let shown = percent(geomean_ratio);
    let (decision, reason) = if !blocking.is_empty() {
        (
            Decision::Reject,
            format!(
                "{} slower than {} at every placement",
                blocking.join(", "),
                percent(WORKLOAD_LIMIT)
            ),
        )
    } else if (geomean_ratio - 1.0).abs() <= control_spread {
        (
            Decision::Neutral,
            format!(
                "geomean {shown} is inside the identical-control spread of ±{:.2}%",
                control_spread * 100.0
            ),
        )
    } else if geomean_ratio <= ACCEPT_GEOMEAN {
        (
            Decision::Accept,
            format!("geomean {shown} is at or below {}", percent(ACCEPT_GEOMEAN)),
        )
    } else {
        (
            Decision::Reject,
            format!("geomean {shown} is above {}", percent(ACCEPT_GEOMEAN)),
        )
    };
    Verdict {
        geomean: geomean_ratio,
        control,
        control_spread,
        blocking,
        decision,
        reason,
    }
}

/// The CPU and operating system.
fn host() -> String {
    let output = |argv: &[&str]| {
        Command::new(argv[0])
            .args(&argv[1..])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    if cfg!(target_os = "macos") {
        format!(
            "{}, macOS {}",
            output(&["sysctl", "-n", "machdep.cpu.brand_string"]),
            output(&["sw_vers", "-productVersion"])
        )
    } else {
        let cpu = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|l| l.starts_with("model name"))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, model)| model.trim().to_string())
            })
            .unwrap_or_else(|| output(&["uname", "-m"]));
        format!("{cpu}, Linux {}", output(&["uname", "-r"]))
    }
}

/// The commit trailer block of a runtime performance change.
fn trailer(
    keels: [&Keel; 2],
    host: &str,
    metric: &str,
    config: &str,
    options: &Options,
    workloads: &[Workload],
    verdict: &Verdict,
) -> String {
    let worst = workloads
        .iter()
        .max_by(|a, b| a.cycles.ratio.total_cmp(&b.cycles.ratio))
        .map(|w| {
            format!(
                "{} {} (placements {} to {})",
                w.name,
                percent(w.cycles.ratio),
                percent(w.cycles.low()),
                percent(w.cycles.high())
            )
        })
        .unwrap_or_default();
    [
        format!("AB-Baseline: {}", keels[0].label),
        match &options.candidate_isel {
            Some(list) => format!("AB-Candidate: {} --isel {list}", keels[1].label),
            None => format!("AB-Candidate: {}", keels[1].label),
        },
        format!("AB-Host: {host}"),
        format!(
            "AB-Protocol: {config}, {} workloads, {} function-padded placements x {} rounds, ABBA with an identical-image control, {metric}",
            workloads.len(),
            options.placements,
            options.rounds
        ),
        format!(
            "AB-Result: geomean {}, control spread ±{:.2}%, worst {worst}",
            percent(verdict.geomean),
            verdict.control_spread * 100.0
        ),
        format!(
            "AB-Verdict: {}",
            serde_json::to_value(verdict.decision)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default()
        ),
    ]
    .join("\n")
}

fn print_report(workloads: &[Workload], verdict: &Verdict, identical: &[String], trailer: &str) {
    println!();
    let range = |c: &Change| format!("{} to {}", percent(c.low()), percent(c.high()));
    let header = [
        "workload",
        "cycles",
        "placements",
        "control",
        "instructions",
        "wall",
        "baseline by placement",
    ]
    .map(|h| Cell::new(h, Paint::Dim))
    .to_vec();
    let rows = workloads
        .iter()
        .map(|w| {
            let paint_of = if w.cycles.ratio < 1.0 - verdict.control_spread {
                Paint::Green
            } else if w.cycles.ratio > 1.0 + verdict.control_spread {
                Paint::Red
            } else {
                Paint::Plain
            };
            let baseline = Change {
                ratio: 1.0,
                placements: w.baseline_placements.clone(),
            };
            vec![
                Cell::plain(w.name.clone()),
                Cell::new(percent(w.cycles.ratio), paint_of),
                Cell::plain(range(&w.cycles)),
                Cell::plain(format!(
                    "{} (±{:.2}%)",
                    percent(w.control.ratio),
                    w.control.reach() * 100.0
                )),
                Cell::plain(
                    w.instructions
                        .as_ref()
                        .map_or("-".into(), |c| percent(c.ratio)),
                ),
                Cell::plain(percent(w.wall.ratio)),
                Cell::plain(range(&baseline)),
            ]
        })
        .collect();
    print_table(header, rows);
    println!();
    if !identical.is_empty() {
        println!(
            "{}",
            paint(
                format!("byte-identical images: {}", identical.join(", ")),
                Paint::Dim
            )
        );
    }
    println!(
        "cycles geomean {}, control {} (spread ±{:.2}%)",
        percent(verdict.geomean),
        percent(verdict.control),
        verdict.control_spread * 100.0
    );
    let (mark, word) = match verdict.decision {
        Decision::Accept => (Paint::Green, "accept"),
        Decision::Neutral => (Paint::Yellow, "neutral"),
        Decision::Reject => (Paint::Red, "reject"),
    };
    println!("verdict: {}: {}", paint(word, mark), verdict.reason);
    println!();
    println!("{}", paint("commit trailer", Paint::Heading));
    println!("{trailer}");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workload whose candidate and control are at fixed ratios at every
    /// placement and round, around a baseline that drifts between rounds.
    fn blocks(candidate: &[f64], control: &[f64]) -> Vec<Vec<Block>> {
        candidate
            .iter()
            .zip(control)
            .map(|(&cand, &ctrl)| {
                (0..5)
                    .map(|round| {
                        let base = 1000.0 + 37.0 * round as f64;
                        [base, base * cand, base * ctrl]
                    })
                    .collect()
            })
            .collect()
    }

    fn workload(name: &str, candidate: &[f64], control: &[f64]) -> Workload {
        let cycles = blocks(candidate, control);
        let series = Series {
            name: name.into(),
            metrics: [cycles.clone(), cycles.clone(), cycles],
        };
        summarize(&series).unwrap()
    }

    const MACHO: &str = "\t.section\t__TEXT,__text\n\t.globl\t_g\n\t.p2align\t2\n_g:\nLg_begin:\n\t.p2align\t4\nLg_block1:\n\tret\nLg_end:\n\t.section\t__TEXT,__text\n\t.globl\t_w\n\t.weak_definition _w\n\t.p2align\t2\n_w:\nLw_begin:\n\tret\n";
    const ELF: &str = "\t.section\t\".text\",\"ax\",@progbits\n\t.p2align\t4\n\t.type\t\"t\",@function\n\"t\":\n.Lt_begin:\n\tret\n";

    /// The padding in front of each line that has some.
    fn skips(padded: &str) -> Vec<(String, usize)> {
        let lines: Vec<_> = padded.lines().collect();
        lines
            .windows(2)
            .filter_map(|w| {
                let bytes = w[0].strip_prefix("\t.skip ")?.split(',').next()?;
                Some((w[1].trim().to_string(), bytes.parse().unwrap()))
            })
            .collect()
    }

    #[test]
    fn functions_are_padded_before_their_alignment_by_name() {
        let mut seen = std::collections::BTreeSet::new();
        for placement in 1..32 {
            let padded = pad_functions(MACHO, placement, "0", 4).unwrap();
            let pads = skips(&padded);
            // Only the two entry alignments, never the block alignment inside g.
            assert!(pads.iter().all(|(line, bytes)| line == ".p2align\t2"
                && bytes % 4 == 0
                && *bytes < PAD_LIMIT));
            assert_eq!(padded.lines().count(), MACHO.lines().count() + pads.len());
            // A function's padding does not depend on the rest of the unit.
            let alone = pad_functions(
                &MACHO[MACHO
                    .find("\t.section\t__TEXT,__text\n\t.globl\t_w")
                    .unwrap()..],
                placement,
                "0",
                4,
            )
            .unwrap();
            assert!(padded.ends_with(&alone), "placement {placement}");
            seen.insert(pads.iter().map(|(_, bytes)| *bytes).collect::<Vec<_>>());
            let elf = pad_functions(ELF, placement, "0xcc", 16).unwrap();
            assert!(
                skips(&elf)
                    .iter()
                    .all(|(line, bytes)| line == ".p2align\t4" && bytes % 16 == 0)
            );
        }
        // Placements pad the functions differently.
        assert!(seen.len() > 16, "{seen:?}");
    }

    #[test]
    fn functions_without_alignment_are_errors_and_data_stays() {
        assert!(
            pad_functions(
                "\t.section\t__TEXT,__text\n_g:\nLg_begin:\n\tret\n",
                1,
                "0",
                4
            )
            .is_err()
        );
        let data = "\t.data\n_x:\n\t.byte 1\n";
        assert_eq!(pad_functions(data, 1, "0", 4).unwrap(), data);
    }

    #[test]
    fn placement_ratios_cancel_round_drift() {
        let w = workload("a/b", &[0.98, 1.0, 1.02, 1.0], &[1.0, 1.001, 0.999, 1.0]);
        assert_eq!(w.cycles.placements.len(), 4);
        assert!((w.cycles.placements[0] - 0.98).abs() < 1e-12);
        assert!((w.cycles.low() - 0.98).abs() < 1e-12);
        assert!((w.cycles.high() - 1.02).abs() < 1e-12);
        assert!((w.control.reach() - 0.001).abs() < 1e-9);
        assert_eq!(w.baseline_placements, vec![1.0; 4]);
    }

    #[test]
    fn identical_compilers_are_neutral() {
        let quiet = [1.002, 0.998, 1.001, 0.999];
        let workloads = [
            workload("a/x", &[1.001, 0.999, 1.0, 1.0], &quiet),
            workload("b/y", &[0.999, 1.002, 1.0, 0.999], &quiet),
        ];
        let v = verdict(&workloads);
        assert_eq!(v.decision, Decision::Neutral, "{}", v.reason);
        assert!(v.blocking.is_empty());
    }

    #[test]
    fn a_geomean_gain_beyond_the_threshold_is_accepted() {
        let quiet = [1.001, 0.999, 1.0, 1.0];
        let workloads = [
            workload("a/x", &[0.99, 0.985, 0.99, 0.992], &quiet),
            // One placement-driven slowdown does not block acceptance.
            workload("b/y", &[1.03, 0.99, 0.98, 0.99], &quiet),
            workload("c/z", &[0.99, 0.99, 0.99, 0.99], &quiet),
        ];
        let v = verdict(&workloads);
        assert_eq!(v.decision, Decision::Accept, "{}", v.reason);
    }

    #[test]
    fn a_slowdown_at_every_placement_rejects() {
        let quiet = [1.001, 0.999, 1.0, 1.0];
        let workloads = [
            workload("a/x", &[0.97, 0.97, 0.97, 0.97], &quiet),
            workload("b/y", &[1.02, 1.03, 1.016, 1.02], &quiet),
        ];
        let v = verdict(&workloads);
        assert_eq!(v.decision, Decision::Reject);
        assert_eq!(v.blocking, ["b/y"]);
        // A slowdown no larger than the control's own noise does not block.
        let noisy = [1.03, 0.97, 1.0, 1.0];
        let workloads = [
            workload("a/x", &[0.97, 0.97, 0.97, 0.97], &quiet),
            workload("b/y", &[1.02, 1.03, 1.016, 1.02], &noisy),
        ];
        assert!(verdict(&workloads).blocking.is_empty());
    }

    #[test]
    fn a_small_gain_outside_the_control_is_not_accepted() {
        let quiet = [1.0005, 0.9995, 1.0, 1.0];
        let workloads = [workload("a/x", &[0.997, 0.997, 0.997, 0.997], &quiet)];
        let v = verdict(&workloads);
        assert_eq!(v.decision, Decision::Reject, "{}", v.reason);
        assert!(v.reason.contains("above -0.50%"), "{}", v.reason);
    }

    #[test]
    fn runs_are_summed_per_block_and_checked_complete() {
        let order = vec!["s/b".to_string()];
        let mut runs = Vec::new();
        for round in 0..2 {
            for (arm, cycles) in [("baseline", 100), ("candidate", 90), ("control", 101)] {
                for _ in 0..2 {
                    runs.push(Timed {
                        suite: "s".into(),
                        bench: "b".into(),
                        round,
                        placement: 0,
                        arm: arm.into(),
                        wall_ns: 1000,
                        counters: format!(
                            "{cycles},,cycles:u,1000,100.00,,\n{},,instructions:u,1000,100.00,,\n",
                            cycles * 2
                        ),
                    });
                }
            }
        }
        let series = collect(&runs, &order, 1, 2).unwrap();
        assert_eq!(series[0].metrics[0][0][1], [200.0, 180.0, 202.0]);
        assert_eq!(series[0].metrics[1][0][0], [400.0, 360.0, 404.0]);
        let w = summarize(&series[0]).unwrap();
        assert!((w.cycles.ratio - 0.9).abs() < 1e-12);
        assert!((w.instructions.unwrap().ratio - 0.9).abs() < 1e-12);
        runs.pop();
        let error = collect(&runs, &order, 1, 2).err().unwrap().to_string();
        assert!(error.contains("two control runs"), "{error}");
        runs[0].counters = "<not counted>,,cycles:u,0,0.00,,\n".into();
        assert!(collect(&runs, &order, 1, 2).is_err());
    }
}

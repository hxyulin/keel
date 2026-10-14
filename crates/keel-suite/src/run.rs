//! Running suites' tests and timing their benchmarks.

use crate::build::{self, BuildStat, Built, Env, run, tail};
use crate::manifest::{Bench, Config, Configs, Suite};
use crate::style::{Paint, duration, fail_mark, ms, ok_mark, paint, spinner};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

pub fn heading(suite: &Suite) {
    println!(
        "{}  {}",
        paint(&suite.name, Paint::Heading),
        paint(&suite.manifest.description, Paint::Dim)
    );
}

fn built_line(config: &Config, built: &Built, files: usize) {
    let s = &built.stat;
    let detail = if built.cached {
        "cached".to_string()
    } else if s.keel_ms > 0.0 {
        format!(
            "{} (keel {}, assembler {}, summed over {})",
            ms(s.wall_ms),
            ms(s.keel_ms),
            ms(s.asm_ms),
            plural(files, "file")
        )
    } else {
        ms(s.wall_ms)
    };
    println!(
        "  {} built {:<12} {}",
        paint("·", Paint::Dim),
        config.name,
        paint(detail, Paint::Dim)
    );
}

/// Builds each suite in each configuration and runs its tests. Returns
/// whether all passed.
pub fn test(env: &Env, suites: &[&Suite], configs: &[Config], all: &Configs) -> Result<bool> {
    let mut passed = true;
    for suite in suites {
        heading(suite);
        let Some(test) = &suite.manifest.test else {
            println!("  {}", paint("no tests", Paint::Dim));
            continue;
        };
        let sources = build::sources(env, suite)?;
        let dir = sources.work.join(test.dir.as_deref().unwrap_or(""));
        let reference = match &test.compare {
            Some(name) => {
                let config = all.get(name)?;
                let built = build::build(env, suite, &sources, config)?;
                built_line(config, &built, sources.pre.len());
                Some((config.name.clone(), built.bin))
            }
            None => None,
        };
        // The reference's output, per command, computed once.
        let mut expected: Vec<Option<Output>> = vec![None; test.run.len()];
        for config in configs {
            let mut start = Instant::now();
            let result = build::build(env, suite, &sources, config).and_then(|built| {
                built_line(config, &built, sources.pre.len());
                start = Instant::now();
                let vars = suite.vars(&sources.work, Some(&built.bin));
                let stdin = test.stdin.as_deref().map(|s| vars.word(s)).transpose()?;
                for (i, command) in test.run.iter().enumerate() {
                    let argv = vars.expand(command)?;
                    let progress = spinner(format!("{} {}: {command}", suite.name, config.name));
                    let output = run(&argv, &dir, stdin.as_deref().map(Path::new));
                    progress.finish_and_clear();
                    let output = output?;
                    if !output.status.success() {
                        bail!("`{command}` {}{}", failure(&output), tail(&output, 20));
                    }
                    if let Some((name, bin)) = &reference {
                        if expected[i].is_none() {
                            let vars = suite.vars(&sources.work, Some(bin));
                            let argv = vars.expand(command)?;
                            expected[i] = Some(run(&argv, &dir, stdin.as_deref().map(Path::new))?);
                        }
                        let want = expected[i].as_ref().unwrap();
                        if let Some(diff) = difference(want, &output) {
                            bail!("`{command}` printed differently from {name}:\n{diff}");
                        }
                    }
                }
                Ok(())
            });
            let elapsed = paint(duration(start.elapsed()), Paint::Dim);
            match result {
                Ok(()) => {
                    let what = match &reference {
                        Some((name, _)) => format!("same output as {name}"),
                        None => format!("{} passed", plural(test.run.len(), "command")),
                    };
                    println!("  {} {:<14} {what}  {elapsed}", ok_mark(), config.name);
                }
                Err(e) => {
                    passed = false;
                    println!("  {} {:<14} {elapsed}", fail_mark(), config.name);
                    for line in format!("{e:#}").lines() {
                        println!("    {}", paint(line, Paint::Red));
                    }
                }
            }
        }
    }
    Ok(passed)
}

/// `exit 1`, or the signal that killed the process.
pub fn status(status: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        _ => status.to_string(),
    }
}

/// Why a command failed: its status, and the first line it printed to
/// standard error.
fn failure(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    match stderr.lines().find(|l| !l.trim().is_empty()) {
        Some(line) => format!("failed ({}): {}", status(output.status), line.trim()),
        None => format!("failed ({})", status(output.status)),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The first line where two outputs differ, if they do.
fn difference(want: &Output, got: &Output) -> Option<String> {
    if want.status.code() != got.status.code() {
        return Some(format!(
            "    {} instead of {}",
            status(got.status),
            status(want.status)
        ));
    }
    for (stream, want, got) in [
        ("stdout", &want.stdout, &got.stdout),
        ("stderr", &want.stderr, &got.stderr),
    ] {
        if want == got {
            continue;
        }
        let (want, got) = (String::from_utf8_lossy(want), String::from_utf8_lossy(got));
        let mut want_lines = want.lines();
        let mut got_lines = got.lines();
        for line in 1.. {
            let (w, g) = (want_lines.next(), got_lines.next());
            if w != g {
                return Some(format!(
                    "    {stream} line {line}:\n    expected: {}\n    got:      {}",
                    w.unwrap_or("<end>"),
                    g.unwrap_or("<end>")
                ));
            }
        }
    }
    None
}

/// A saved benchmark run.
#[derive(Serialize, Deserialize)]
pub struct Results {
    pub target: String,
    pub host: String,
    /// The keel commit, `-dirty` if the tree had changes.
    pub keel: String,
    /// Seconds since the Unix epoch.
    pub date: u64,
    pub baseline: String,
    /// The configurations, in table order.
    pub configs: Vec<String>,
    pub builds: Vec<BuildStat>,
    pub benches: Vec<BenchStat>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BenchStat {
    pub suite: String,
    pub bench: String,
    pub config: String,
    pub samples_ms: Vec<f64>,
    pub mean_ms: f64,
    pub stddev_ms: f64,
    pub min_ms: f64,
    /// Why there are no samples: the build printed something else, or failed.
    #[serde(default)]
    pub error: Option<String>,
}

pub struct Timing {
    pub runs: u32,
    pub warmup: u32,
}

/// Builds each suite in every configuration, checks each benchmark prints
/// the same in all, and times it. Prints a table per suite.
pub fn bench(
    env: &Env,
    suites: &[(&Suite, Vec<&Bench>)],
    configs: &[Config],
    baseline: &str,
    timing: &Timing,
    results: &mut Results,
) -> Result<()> {
    // The reference output is the baseline's, else the first configuration's.
    let reference = configs.iter().position(|c| c.name == baseline).unwrap_or(0);
    for (suite, benches) in suites {
        heading(suite);
        let sources = build::sources(env, suite)?;
        let mut builds: Vec<Built> = Vec::new();
        for config in configs {
            let built = build::build(env, suite, &sources, config)?;
            built_line(config, &built, sources.pre.len());
            results.builds.push(built.stat.clone());
            builds.push(built);
        }
        for bench in benches {
            let mut order: Vec<usize> = (0..configs.len()).collect();
            order.swap(0, reference);
            // The first output that succeeded, and whose it is.
            let mut expected: Option<(Output, &str)> = None;
            let mut stats = vec![None; configs.len()];
            for &i in &order {
                let (config, built) = (&configs[i], &builds[i]);
                let label = format!("{}/{} {}", suite.name, bench.name, config.name);
                let vars = suite.vars(&sources.work, Some(&built.bin));
                let argv = vars.expand(&bench.run)?;
                let stdin = bench.stdin.as_deref().map(|s| vars.word(s)).transpose()?;
                let stdin = stdin.map(PathBuf::from);
                let progress = spinner(format!("{label}: checking output"));
                let output = run(&argv, &sources.work, stdin.as_deref())?;
                let error = if !output.status.success() {
                    Some(failure(&output))
                } else if let Some((want, name)) = &expected {
                    difference(want, &output)
                        .map(|diff| format!("prints differently from {name}:\n{diff}"))
                } else {
                    expected = Some((output, &config.name));
                    None
                };
                let mut samples = Vec::new();
                if error.is_none() {
                    for n in 0..timing.warmup + timing.runs {
                        progress.set_message(if n < timing.warmup {
                            format!("{label}: warming up")
                        } else {
                            format!("{label}: run {}/{}", n - timing.warmup + 1, timing.runs)
                        });
                        let t = time(&argv, &sources.work, stdin.as_deref())?;
                        if n >= timing.warmup {
                            samples.push(t);
                        }
                    }
                }
                progress.finish_and_clear();
                if let Some(e) = &error {
                    println!("  {} {label} {}", fail_mark(), paint(e, Paint::Red));
                }
                stats[i] = Some(summarize(suite, bench, config, samples, error));
            }
            results.benches.extend(stats.into_iter().flatten());
        }
        crate::report::print_suite(results, &suite.name);
    }
    Ok(())
}

/// One run's wall-clock time in milliseconds, its output discarded.
fn time(argv: &[String], dir: &Path, stdin: Option<&Path>) -> Result<f64> {
    let stdin = match stdin {
        Some(file) => Stdio::from(std::fs::File::open(file)?),
        None => Stdio::null(),
    };
    let start = Instant::now();
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    if !status.success() {
        bail!("`{}` failed while timed ({status})", argv.join(" "));
    }
    Ok(elapsed)
}

fn summarize(
    suite: &Suite,
    bench: &Bench,
    config: &Config,
    samples: Vec<f64>,
    error: Option<String>,
) -> BenchStat {
    let n = samples.len().max(1) as f64;
    let mean = samples.iter().sum::<f64>() / n;
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
    BenchStat {
        suite: suite.name.clone(),
        bench: bench.name.clone(),
        config: config.name.clone(),
        min_ms: samples.iter().copied().reduce(f64::min).unwrap_or(0.0),
        mean_ms: mean,
        stddev_ms: variance.sqrt(),
        samples_ms: samples,
        error,
    }
}

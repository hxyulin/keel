//! Running suites' tests and timing their benchmarks.

use crate::build::{self, BuildStat, Built, Env, run, tail};
use crate::manifest::{Bench, Config, Configs, Suite};
use crate::style::{Paint, duration, fail_mark, ms, ok_mark, paint, spinner};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Output;
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
    /// CPU model, operating-system version and SDK identity for comparisons.
    #[serde(default)]
    pub environment: String,
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
    /// Zero-based execution position within each measured round.
    #[serde(default)]
    pub positions: Vec<usize>,
    /// Missing in older files, whose samples were collected in separate batches.
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub paired_ratio: Option<RatioInterval>,
    /// Identity of the benchmark recipe, scripts, stdin and generated inputs.
    #[serde(default)]
    pub workload_sha256: String,
    /// Exit status, stdout and stderr accepted by every measured invocation.
    #[serde(default)]
    pub output_sha256: String,
    #[serde(default)]
    pub binary_sha256: String,
    /// Why there are no samples: the build printed something else, or failed.
    #[serde(default)]
    pub error: Option<String>,
}

pub const IDENTICAL_CONTROL: &str = "__identical-control";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RatioInterval {
    pub median: f64,
    /// Percentile bootstrap interval; absent with fewer than five pairs.
    pub bootstrap95: Option<[f64; 2]>,
    pub pairs: usize,
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
    if configs.is_empty() || timing.runs == 0 {
        bail!("benchmarking requires configurations and at least one measured round");
    }
    if configs.iter().any(|c| c.name == IDENTICAL_CONTROL) {
        bail!("configuration name `{IDENTICAL_CONTROL}` is reserved for the timing control");
    }
    // If the baseline is not selected, timings have no baseline ratio.
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
        let control = builds[reference]
            .bin
            .with_file_name(".keel-suite-identical-control");
        std::fs::copy(&builds[reference].bin, &control)?;
        for bench in benches {
            let mut invocations = Vec::new();
            for built in &builds {
                invocations.push(invocation(suite, bench, &sources.work, &built.bin)?);
            }
            invocations.push(invocation(suite, bench, &sources.work, &control)?);
            let (argv, stdin) = &invocations[reference];
            let expected = run(argv, &sources.work, stdin.as_deref())?;
            if !expected.status.success() {
                bail!(
                    "{}/{} reference {} {}",
                    suite.name,
                    bench.name,
                    configs[reference].name,
                    failure(&expected)
                );
            }
            let workload_sha256 = workload_digest(
                suite,
                bench,
                &sources.work,
                &invocations[reference],
                &builds[reference].bin,
            )?;
            let output_sha256 = output_digest(&expected);
            let mut binary_paths: Vec<&Path> = builds.iter().map(|b| b.bin.as_path()).collect();
            binary_paths.push(&control);
            let binary_sha256: Vec<String> = binary_paths
                .iter()
                .map(|p| {
                    let mut hasher = Sha256::new();
                    hash_input_tree(&mut hasher, p)?;
                    Ok(digest_hex(hasher))
                })
                .collect::<Result<_>>()?;
            let mut errors = Vec::new();
            for (argv, stdin) in &invocations {
                let output = run(argv, &sources.work, stdin.as_deref())?;
                let error = if output.status.success() {
                    difference(&expected, &output)
                } else {
                    Some(failure(&output))
                };
                errors.push(error);
            }
            let active: Vec<usize> = (0..invocations.len())
                .filter(|&i| errors[i].is_none())
                .collect();
            let mut samples = vec![Vec::new(); invocations.len()];
            let mut positions = vec![Vec::new(); invocations.len()];
            let progress = spinner(format!("{}/{}: interleaved rounds", suite.name, bench.name));
            for round in 0..timing.warmup + timing.runs {
                let measured = round >= timing.warmup;
                let round_index = if measured {
                    round - timing.warmup
                } else {
                    round
                };
                for (position, i) in round_order(&active, round_index as usize)
                    .into_iter()
                    .enumerate()
                {
                    let (argv, stdin) = &invocations[i];
                    let start = Instant::now();
                    let output = run(argv, &sources.work, stdin.as_deref())?;
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    if let Some(error) = difference(&expected, &output) {
                        bail!(
                            "{}/{} changed output during timing: {error}",
                            suite.name,
                            bench.name
                        );
                    }
                    if measured {
                        samples[i].push(elapsed);
                        positions[i].push(position);
                    }
                }
            }
            progress.finish_and_clear();
            if workload_digest(
                suite,
                bench,
                &sources.work,
                &invocations[reference],
                &builds[reference].bin,
            )? != workload_sha256
            {
                bail!("{}/{} inputs changed during timing", suite.name, bench.name);
            }
            for (path, expected_hash) in binary_paths.iter().zip(&binary_sha256) {
                let mut hasher = Sha256::new();
                hash_input_tree(&mut hasher, path)?;
                if digest_hex(hasher) != *expected_hash {
                    bail!("{} changed during timing", path.display());
                }
            }
            let reference_samples = samples[reference].clone();
            for i in 0..invocations.len() {
                let config = if i == configs.len() {
                    IDENTICAL_CONTROL
                } else {
                    &configs[i].name
                };
                if let Some(error) = &errors[i] {
                    println!(
                        "  {} {}/{} {config}: {error}",
                        fail_mark(),
                        suite.name,
                        bench.name
                    );
                }
                let paired_ratio = if configs[reference].name == baseline {
                    paired_ratio(&samples[i], &reference_samples)
                } else {
                    None
                };
                let mut stat = summarize(
                    suite,
                    bench,
                    config,
                    std::mem::take(&mut samples[i]),
                    errors[i].take(),
                );
                stat.positions = std::mem::take(&mut positions[i]);
                stat.protocol = Some("rotating-reversed-rounds-v1".into());
                stat.paired_ratio = paired_ratio;
                stat.workload_sha256 = workload_sha256.clone();
                stat.output_sha256 = output_sha256.clone();
                stat.binary_sha256 = binary_sha256[i].clone();
                results.benches.push(stat);
            }
        }
        crate::report::print_suite(results, &suite.name);
    }
    Ok(())
}

fn digest_hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn output_digest(output: &Output) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, status(output.status).as_bytes());
    hash_part(&mut hasher, &output.stdout);
    hash_part(&mut hasher, &output.stderr);
    digest_hex(hasher)
}

fn hash_input_tree(hasher: &mut Sha256, path: &Path) -> Result<()> {
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        entries.sort();
        for entry in entries {
            hash_part(
                hasher,
                entry.file_name().unwrap_or_default().as_encoded_bytes(),
            );
            hash_input_tree(hasher, &entry)?;
        }
    } else {
        hash_part(hasher, &std::fs::read(path)?);
    }
    Ok(())
}

fn workload_digest(
    suite: &Suite,
    bench: &Bench,
    work: &Path,
    invocation: &(Vec<String>, Option<PathBuf>),
    bin: &Path,
) -> Result<String> {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, suite.text.as_bytes());
    hash_part(&mut hasher, bench.run.as_bytes());
    hash_input_tree(&mut hasher, &suite.dir)?;
    let generated = work.join("gen");
    if generated.exists() {
        hash_input_tree(&mut hasher, &generated)?;
    }
    for argument in &invocation.0 {
        if Path::new(argument) == bin {
            continue;
        }
        let path = work.join(argument);
        if path.exists() {
            hash_input_tree(&mut hasher, &path)?;
        }
    }
    if let Some(stdin) = &invocation.1 {
        hash_input_tree(&mut hasher, stdin)?;
    }
    Ok(digest_hex(hasher))
}

fn invocation(
    suite: &Suite,
    bench: &Bench,
    work: &Path,
    bin: &Path,
) -> Result<(Vec<String>, Option<PathBuf>)> {
    let vars = suite.vars(work, Some(bin));
    let argv = vars.expand(&bench.run)?;
    let stdin = bench
        .stdin
        .as_deref()
        .map(|s| vars.word(s))
        .transpose()?
        .map(PathBuf::from);
    Ok((argv, stdin))
}

/// Every participant occupies every position once per rotation, followed by
/// reversed rotations to balance pair order across complete cycles.
fn round_order(active: &[usize], round: usize) -> Vec<usize> {
    if active.is_empty() {
        return Vec::new();
    }
    let mut order = active.to_vec();
    order.rotate_left(round % active.len());
    if (round / active.len()) % 2 == 1 {
        order.reverse();
    }
    order
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

pub fn paired_ratio(samples: &[f64], baseline: &[f64]) -> Option<RatioInterval> {
    if samples.is_empty()
        || samples.len() != baseline.len()
        || samples
            .iter()
            .chain(baseline)
            .any(|s| !s.is_finite() || *s <= 0.0)
    {
        return None;
    }
    let mut ratios: Vec<f64> = samples.iter().zip(baseline).map(|(s, b)| s / b).collect();
    let estimate = median(&mut ratios);
    let bootstrap95 = if ratios.len() < 5 {
        None
    } else {
        let mut state = 24_517_000_u64;
        let mut boot = Vec::with_capacity(4000);
        let mut draw = vec![0.0; ratios.len()];
        for _ in 0..4000 {
            for value in &mut draw {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *value = ratios[state as usize % ratios.len()];
            }
            boot.push(median(&mut draw));
        }
        boot.sort_by(f64::total_cmp);
        Some([boot[99], boot[3899]])
    };
    Some(RatioInterval {
        median: estimate,
        bootstrap95,
        pairs: ratios.len(),
    })
}

fn summarize(
    suite: &Suite,
    bench: &Bench,
    config: &str,
    samples: Vec<f64>,
    error: Option<String>,
) -> BenchStat {
    let n = samples.len().max(1) as f64;
    let mean = samples.iter().sum::<f64>() / n;
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
    BenchStat {
        suite: suite.name.clone(),
        bench: bench.name.clone(),
        config: config.into(),
        positions: Vec::new(),
        protocol: None,
        paired_ratio: None,
        workload_sha256: String::new(),
        output_sha256: String::new(),
        binary_sha256: String::new(),
        min_ms: samples.iter().copied().reduce(f64::min).unwrap_or(0.0),
        mean_ms: mean,
        stddev_ms: variance.sqrt(),
        samples_ms: samples,
        error,
    }
}

#[cfg(test)]
mod timing_tests {
    use super::*;

    #[test]
    fn rotations_balance_positions_and_pair_order() {
        let active = [0, 2, 3, 4];
        let mut positions = [[0; 4]; 5];
        let mut precedes = 0;
        for round in 0..active.len() * 2 {
            let order = round_order(&active, round);
            for (position, &index) in order.iter().enumerate() {
                positions[index][position] += 1;
            }
            precedes += usize::from(
                order.iter().position(|&i| i == 0) < order.iter().position(|&i| i == 4),
            );
        }
        for index in active {
            assert_eq!(positions[index], [2; 4], "participant {index}");
        }
        assert_eq!(precedes, active.len());
        assert!(round_order(&[], 1).is_empty());
    }

    #[test]
    fn paired_ratios_cancel_shared_round_drift() {
        let baseline = [10., 20., 40., 80., 160.];
        let candidate = baseline.map(|v| v * 1.25);
        let ratio = paired_ratio(&candidate, &baseline).unwrap();
        assert_eq!(ratio.median, 1.25);
        assert_eq!(ratio.bootstrap95, Some([1.25, 1.25]));
        assert_eq!(ratio.pairs, 5);
    }

    #[test]
    fn intervals_require_valid_pairs_and_enough_samples() {
        assert!(paired_ratio(&[], &[]).is_none());
        assert!(paired_ratio(&[1.], &[1., 2.]).is_none());
        assert!(paired_ratio(&[1.], &[0.]).is_none());
        assert!(paired_ratio(&[f64::NAN], &[1.]).is_none());
        assert!(paired_ratio(&[f64::INFINITY], &[1.]).is_none());
        assert!(paired_ratio(&[-1.], &[1.]).is_none());
        assert!(
            paired_ratio(&[1., 2.], &[1., 2.])
                .unwrap()
                .bootstrap95
                .is_none()
        );
        let noisy = paired_ratio(&[0.8, 1.2, 0.7, 1.3, 1.0], &[1.; 5]).unwrap();
        let [lo, hi] = noisy.bootstrap95.unwrap();
        assert!(lo < 1.0 && hi > 1.0, "noise must not establish a direction");
    }

    #[test]
    fn old_samples_are_not_claimed_to_be_paired() {
        let json = r#"{"suite":"s","bench":"b","config":"c","samples_ms":[1.0],"mean_ms":1.0,"stddev_ms":0.0,"min_ms":1.0}"#;
        let stat: BenchStat = serde_json::from_str(json).unwrap();
        assert!(stat.protocol.is_none());
        assert!(stat.positions.is_empty());
        assert!(stat.paired_ratio.is_none());
    }

    #[test]
    fn runtime_input_identity_covers_nested_file_contents() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("keel-suite-inputs-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(root.join("corpus")).unwrap();
        let input = root.join("corpus/input.json");
        std::fs::write(&input, "first").unwrap();
        let digest = || {
            let mut hasher = Sha256::new();
            hash_input_tree(&mut hasher, &root).unwrap();
            digest_hex(hasher)
        };
        let before = digest();
        assert_eq!(
            before,
            digest(),
            "unchanged inputs must have stable identity"
        );
        std::fs::write(input, "other").unwrap();
        assert_ne!(
            before,
            digest(),
            "same-length runtime input changes must invalidate comparisons"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

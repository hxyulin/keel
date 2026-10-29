//! Result tables, and saving, loading and comparing runs.

use crate::run::{BenchStat, IDENTICAL_CONTROL, Results};
use crate::style::{Cell, Paint, bytes, ms, paint, print_table, ratio};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

fn find<'a>(results: &'a Results, suite: &str, bench: &str, config: &str) -> Option<&'a BenchStat> {
    results
        .benches
        .iter()
        .find(|b| b.suite == suite && b.bench == bench && b.config == config && b.error.is_none())
}

/// The suite's benchmark names, in run order.
fn bench_names(results: &Results, suite: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for b in results.benches.iter().filter(|b| b.suite == suite) {
        if !names.contains(&b.bench) {
            names.push(b.bench.clone());
        }
    }
    names
}

/// Each configuration's time for each benchmark, its ratio to the
/// baseline, their geometric mean, and the build's time and code size.
pub fn print_suite(results: &Results, suite: &str) {
    let benches = bench_names(results, suite);
    let mut header = vec![Cell::plain("")];
    header.extend(benches.iter().map(|b| Cell::new(b, Paint::Bold)));
    header.extend(["geomean", "build", "code"].map(|h| Cell::new(h, Paint::Bold)));
    let mut rows = Vec::new();
    for config in &results.configs {
        let is_baseline = *config == results.baseline;
        let name_paint = if is_baseline {
            Paint::Bold
        } else {
            Paint::Plain
        };
        let mut row = vec![Cell::new(config, name_paint)];
        let mut ratios = Vec::new();
        for bench in &benches {
            let base = find(results, suite, bench, &results.baseline);
            let cell = match find(results, suite, bench, config) {
                None => Cell::new("failed", Paint::Red),
                Some(stat) => {
                    let noisy = stat.stddev_ms > 0.05 * stat.mean_ms;
                    let mut cell = Cell::plain(ms(stat.mean_ms))
                        .then(if noisy { "±" } else { " " }, Paint::Yellow);
                    match base {
                        Some(base) if !is_baseline => {
                            let r = stat
                                .paired_ratio
                                .as_ref()
                                .map_or(stat.mean_ms / base.mean_ms, |p| p.median);
                            ratios.push(r);
                            let (text, color) = ratio(r);
                            // Ratios describe relative speed, not a regression verdict.
                            let color = if stat
                                .paired_ratio
                                .as_ref()
                                .and_then(|p| p.bootstrap95)
                                .is_some_and(|[lo, hi]| lo > 1.0 || hi < 1.0)
                            {
                                color
                            } else {
                                Paint::Dim
                            };
                            cell = cell.then(format!(" {text}"), color);
                            if let Some([lo, hi]) =
                                stat.paired_ratio.as_ref().and_then(|p| p.bootstrap95)
                            {
                                cell = cell.then(format!(" [{lo:.2}, {hi:.2}]"), Paint::Dim);
                            }
                        }
                        Some(_) => cell = cell.then(" 1.00×", Paint::Dim),
                        None => {}
                    }
                    cell
                }
            };
            row.push(cell);
        }
        row.push(if ratios.is_empty() {
            Cell::new(if is_baseline { "1.00×" } else { "" }, Paint::Dim)
        } else {
            let geomean = (ratios.iter().map(|r| r.ln()).sum::<f64>() / ratios.len() as f64).exp();
            let (text, paint) = ratio(geomean);
            Cell::new(text, paint)
        });
        let build = results
            .builds
            .iter()
            .find(|b| b.suite == suite && b.config == *config);
        row.push(Cell::new(
            build.map_or(String::new(), |b| ms(b.wall_ms)),
            Paint::Dim,
        ));
        row.push(Cell::new(
            build
                .and_then(|b| b.text_bytes)
                .map_or(String::new(), bytes),
            Paint::Dim,
        ));
        rows.push(row);
    }
    print_table(header, rows);
    if results
        .benches
        .iter()
        .any(|b| b.suite == suite && b.stddev_ms > 0.05 * b.mean_ms)
    {
        println!(
            "  {}",
            paint("± more than 5% standard deviation", Paint::Yellow)
        );
    }
    if results
        .benches
        .iter()
        .any(|b| b.suite == suite && b.protocol.is_some())
    {
        println!("  Ratios are paired medians; brackets show percentile bootstrap 95% intervals.");
        println!(
            "  Fewer than five pairs have no interval; ratios are descriptive, not slowdown verdicts."
        );
        for bench in &benches {
            if let Some(control) =
                find(results, suite, bench, IDENTICAL_CONTROL).and_then(|b| b.paired_ratio.as_ref())
            {
                let interval = control
                    .bootstrap95
                    .map_or("no interval".into(), |[lo, hi]| {
                        format!("[{lo:.2}, {hi:.2}]")
                    });
                println!(
                    "  {bench} identical-binary control: {:.2}× {interval}",
                    control.median
                );
            }
        }
    }
    println!();
}

fn suites(results: &Results) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for b in &results.benches {
        if !names.contains(&b.suite) {
            names.push(b.suite.clone());
        }
    }
    names
}

pub fn describe(results: &Results) -> String {
    format!(
        "{} on {}, keel {}, {}",
        paint(&results.target, Paint::Bold),
        results.host,
        results.keel,
        date(results.date)
    )
}

pub fn show(results: &Results) {
    println!("{}\n", describe(results));
    for suite in suites(results) {
        println!("{}", paint(&suite, Paint::Heading));
        print_suite(results, &suite);
    }
}

fn results_dir(root: &Path) -> PathBuf {
    root.join("target/keel-suite/results")
}

/// Saves a run as `<target>/<date>.json` and `<target>/latest.json`.
pub fn save(root: &Path, results: &Results) -> Result<PathBuf> {
    let dir = results_dir(root).join(&results.target);
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(results)?;
    let file = dir.join(format!("{}.json", results.date));
    fs::write(&file, &json)?;
    fs::write(dir.join("latest.json"), &json)?;
    Ok(file)
}

/// A run from a results file, or the latest run for a target triple.
pub fn load(root: &Path, which: &str) -> Result<Results> {
    let path = Path::new(which);
    let file = if path.is_file() {
        path.to_path_buf()
    } else {
        let latest = results_dir(root).join(which).join("latest.json");
        if !latest.is_file() {
            let saved: Vec<String> = fs::read_dir(results_dir(root))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect();
            bail!(
                "`{which}` is neither a results file nor a target with saved runs (saved: {})",
                if saved.is_empty() {
                    "none".into()
                } else {
                    saved.join(", ")
                }
            );
        }
        latest
    };
    let text = fs::read_to_string(&file).with_context(|| file.display().to_string())?;
    serde_json::from_str(&text).with_context(|| format!("reading {}", file.display()))
}

/// Compares two runs. On one target, times are compared directly (B/A).
/// Across targets the machines differ, so what is compared is each
/// configuration's ratio to its own run's baseline.
pub fn compare(a: &Results, b: &Results) {
    let across = a.target != b.target;
    println!("{} {}", paint("A", Paint::Bold), describe(a));
    println!("{} {}", paint("B", Paint::Bold), describe(b));
    if across {
        println!(
            "{}\n",
            paint(
                format!(
                    "different targets: each cell is the ratio to that run's {}",
                    a.baseline
                ),
                Paint::Dim
            )
        );
    } else {
        println!("{}\n", paint("each cell is A → B, and B/A", Paint::Dim));
    }
    for suite in suites(a) {
        let benches: Vec<String> = bench_names(a, &suite)
            .into_iter()
            .filter(|n| bench_names(b, &suite).contains(n))
            .collect();
        if benches.is_empty() {
            continue;
        }
        println!("{}", paint(&suite, Paint::Heading));
        let mut header = vec![Cell::plain("")];
        header.extend(benches.iter().map(|n| Cell::new(n, Paint::Bold)));
        let mut rows = Vec::new();
        for config in a.configs.iter().filter(|c| b.configs.contains(c)) {
            let mut row = vec![Cell::plain(config)];
            for bench in &benches {
                let (x, y) = (
                    find(a, &suite, bench, config),
                    find(b, &suite, bench, config),
                );
                let cell = match (x, y) {
                    (Some(x), Some(y)) if across => {
                        let relative = |r: &Results, s: &BenchStat| {
                            find(r, &suite, bench, &r.baseline).map(|base| s.mean_ms / base.mean_ms)
                        };
                        match (relative(a, x), relative(b, y)) {
                            (Some(rx), Some(ry)) => {
                                let ((tx, px), (ty, py)) = (ratio(rx), ratio(ry));
                                Cell::new(tx, px).then(" → ", Paint::Dim).then(ty, py)
                            }
                            _ => Cell::new("no baseline", Paint::Dim),
                        }
                    }
                    (Some(x), Some(y)) => {
                        let (text, paint) = ratio(y.mean_ms / x.mean_ms);
                        Cell::new(
                            format!("{} → {} ", ms(x.mean_ms), ms(y.mean_ms)),
                            Paint::Dim,
                        )
                        .then(text, paint)
                    }
                    _ => Cell::new("failed", Paint::Red),
                };
                row.push(cell);
            }
            rows.push(row);
        }
        print_table(header, rows);
        println!();
    }
}

/// `YYYY-MM-DD HH:MM UTC` for seconds since the epoch.
fn date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let (h, m) = (secs % 86400 / 3600, secs % 3600 / 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

#[cfg(test)]
mod tests {
    #[test]
    fn date() {
        assert_eq!(super::date(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::date(1_791_000_000), "2026-10-03 04:00 UTC");
    }
}

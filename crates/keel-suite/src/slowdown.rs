//! Conservative runtime comparisons between separately collected Keel revisions.
use crate::run::{BenchStat, IDENTICAL_CONTROL, Results, paired_ratio};
use anyhow::{Result, bail};

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}

/// Independent resampling: rounds from different runs are not paired.
fn interval(a: &[f64], b: &[f64]) -> Option<[f64; 2]> {
    if a.len() < 10 || b.len() < 10 || a.iter().chain(b).any(|x| !x.is_finite() || *x <= 0.0) {
        return None;
    }
    let mut state = 24_517_001_u64;
    let mut draw = |values: &[f64]| {
        let mut sample: Vec<f64> = values
            .iter()
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                values[state as usize % values.len()]
            })
            .collect();
        median(&mut sample)
    };
    let mut ratios = Vec::with_capacity(4000);
    for _ in 0..4000 {
        let x = draw(a);
        ratios.push(draw(b) / x);
    }
    ratios.sort_by(f64::total_cmp);
    Some([ratios[99], ratios[3899]])
}

fn measured_samples(stat: &BenchStat) -> bool {
    stat.error.is_none()
        && stat.protocol.as_deref() == Some("rotating-reversed-rounds-v1")
        && stat.samples_ms.len() >= 10
        && stat.positions.len() == stat.samples_ms.len()
        && stat.samples_ms.iter().all(|x| x.is_finite() && *x > 0.0)
}

fn same_workload(a: &BenchStat, b: &BenchStat) -> bool {
    !a.workload_sha256.is_empty()
        && a.workload_sha256 == b.workload_sha256
        && !a.output_sha256.is_empty()
        && a.output_sha256 == b.output_sha256
}

fn control_ok(results: &Results, stat: &BenchStat, tolerance: f64) -> bool {
    let find = |config: &str| {
        results
            .benches
            .iter()
            .find(|c| c.suite == stat.suite && c.bench == stat.bench && c.config == config)
    };
    let Some(control) = find(IDENTICAL_CONTROL) else {
        return false;
    };
    let Some(baseline) = find(&results.baseline) else {
        return false;
    };
    if !measured_samples(control)
        || !measured_samples(baseline)
        || !same_workload(control, stat)
        || !same_workload(baseline, stat)
        || control.samples_ms.len() != stat.samples_ms.len()
        || baseline.binary_sha256.is_empty()
        || control.binary_sha256 != baseline.binary_sha256
    {
        return false;
    }
    paired_ratio(&control.samples_ms, &baseline.samples_ms)
        .and_then(|r| r.bootstrap95)
        .is_some_and(|[low, high]| low >= 1.0 / (1.0 + tolerance) && high <= 1.0 + tolerance)
}

/// Reports runtime regressions only for compatible measurements with quiet controls.
/// Inconclusive cases remain visible and do not fail the command.
pub fn check(a: &Results, b: &Results, threshold: f64) -> Result<bool> {
    if !threshold.is_finite() || threshold <= 0.0 {
        bail!("threshold must be a positive finite fraction (0.05 means 5%)");
    }
    if a.target != b.target || a.host != b.host {
        bail!("regression checks require the same host and target");
    }
    if a.environment.is_empty() || a.environment != b.environment {
        bail!("regression checks require recorded, matching CPU and OS environments");
    }
    let mut passed = true;
    let mut compared = 0;
    for x in a.benches.iter().filter(|s| s.config != IDENTICAL_CONTROL) {
        let Some(y) = b
            .benches
            .iter()
            .find(|s| s.suite == x.suite && s.bench == x.bench && s.config == x.config)
        else {
            continue;
        };
        let label = format!("{}/{} {}", x.suite, x.bench, x.config);
        let builds = a
            .builds
            .iter()
            .find(|s| s.suite == x.suite && s.config == x.config)
            .zip(
                b.builds
                    .iter()
                    .find(|s| s.suite == y.suite && s.config == y.config),
            );
        let compatible = builds.is_some_and(|(u, v)| {
            !u.input_sha256.is_empty()
                && u.input_sha256 == v.input_sha256
                && u.input_mode == v.input_mode
                && u.config_flags == v.config_flags
                && u.jobs == v.jobs
        });
        if !compatible
            || !measured_samples(x)
            || !measured_samples(y)
            || !same_workload(x, y)
            || a.baseline != b.baseline
        {
            println!(
                "{label}: inconclusive (failed, old or incompatible workload, output, inputs, flags, jobs or timing protocol)"
            );
            continue;
        }
        if !control_ok(a, x, threshold.min(0.05)) || !control_ok(b, y, threshold.min(0.05)) {
            println!("{label}: inconclusive (missing or noisy identical-binary control)");
            continue;
        }
        let Some([low, high]) = interval(&x.samples_ms, &y.samples_ms) else {
            println!("{label}: inconclusive (need at least 10 valid samples per run)");
            continue;
        };
        compared += 1;
        let regression = low > 1.0 + threshold;
        passed &= !regression;
        println!(
            "{label}: {} (candidate/baseline 95% interval {low:.3}–{high:.3})",
            if regression {
                "slowdown; repeat the measurement to confirm"
            } else {
                "no slowdown established"
            }
        );
    }
    println!(
        "{compared} runtime comparisons; separate runs cannot rule out host drift; build timings are descriptive only"
    );
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_runs_detect_slowdown_without_pairing_rounds() {
        assert_eq!(interval(&[10.0; 10], &[12.0; 10]), Some([1.2, 1.2]));
        assert_eq!(interval(&[10.0; 10], &[10.0; 10]), Some([1.0, 1.0]));
    }
    #[test]
    fn insufficient_or_invalid_samples_are_inconclusive() {
        assert!(interval(&[10.0; 9], &[12.0; 10]).is_none());
        assert!(interval(&[0.0; 10], &[12.0; 10]).is_none());
        assert!(interval(&[f64::NAN; 10], &[12.0; 10]).is_none());
    }

    fn fixture(candidate_ms: f64) -> Results {
        let mut result: Results = serde_json::from_value(serde_json::json!({
            "target": "test-target", "host": "test-host", "environment": "test CPU and OS",
            "keel": "test-revision", "date": 0, "baseline": "clang",
            "configs": ["clang", "keel"], "builds": [], "benches": []
        }))
        .unwrap();
        for (config, ms) in [
            ("clang", 10.0),
            ("keel", candidate_ms),
            (IDENTICAL_CONTROL, 10.0),
        ] {
            result.benches.push(serde_json::from_value(serde_json::json!({
                "suite": "app", "bench": "work", "config": config,
                "samples_ms": vec![ms; 10], "mean_ms": ms, "stddev_ms": 0.0, "min_ms": ms,
                "positions": vec![0; 10], "protocol": "rotating-reversed-rounds-v1",
                "workload_sha256": "same-workload", "output_sha256": "same-output", "binary_sha256": "fixed-image"
            })).unwrap());
            if config != IDENTICAL_CONTROL {
                result.builds.push(
                    serde_json::from_value(serde_json::json!({
                        "suite": "app", "config": config, "wall_ms": 0.0,
                        "compile_ms": 0.0, "keel_ms": 0.0, "asm_ms": 0.0,
                        "text_bytes": null, "input_sha256": "same-inputs", "jobs": 1
                    }))
                    .unwrap(),
                );
            }
        }
        result
    }

    #[test]
    fn compatible_measurements_establish_only_practical_slowdowns() {
        assert!(!check(&fixture(10.0), &fixture(12.0), 0.05).unwrap());
        assert!(check(&fixture(10.0), &fixture(10.4), 0.05).unwrap());
        assert!(check(&fixture(10.0), &fixture(8.0), 0.05).unwrap());
    }

    #[test]
    fn changed_workload_output_and_protocol_are_inconclusive() {
        for field in ["workload_sha256", "output_sha256", "protocol"] {
            let baseline = fixture(10.0);
            let mut candidate = fixture(12.0);
            let stat = &mut candidate.benches[1];
            match field {
                "workload_sha256" => stat.workload_sha256 = "different".into(),
                "output_sha256" => stat.output_sha256 = "different".into(),
                _ => stat.protocol = Some("unknown-protocol".into()),
            }
            assert!(check(&baseline, &candidate, 0.05).unwrap(), "{field}");
        }
        let mut old = fixture(12.0);
        old.benches[1].workload_sha256.clear();
        assert!(check(&fixture(10.0), &old, 0.05).unwrap());
    }

    #[test]
    fn noisy_or_incomplete_controls_cannot_establish_a_slowdown() {
        let mut candidate = fixture(12.0);
        // A stored narrow summary must not override the actual noisy control.
        candidate.benches[2].paired_ratio = paired_ratio(&[10.0; 10], &[10.0; 10]);
        candidate.benches[2].samples_ms = vec![12.0; 10];
        assert!(check(&fixture(10.0), &candidate, 0.05).unwrap());
        candidate.benches[2].samples_ms = vec![10.0; 10];
        candidate.benches[2].positions.pop();
        assert!(check(&fixture(10.0), &candidate, 0.05).unwrap());
        candidate.benches[2].positions.push(0);
        candidate.benches[2].binary_sha256 = "different-image".into();
        assert!(check(&fixture(10.0), &candidate, 0.05).unwrap());
        candidate.benches.pop();
        assert!(check(&fixture(10.0), &candidate, 0.05).unwrap());
    }

    #[test]
    fn environment_and_invalid_thresholds_are_rejected() {
        let mut candidate = fixture(12.0);
        candidate.environment = "different CPU".into();
        assert!(check(&fixture(10.0), &candidate, 0.05).is_err());
        candidate.environment.clear();
        assert!(check(&fixture(10.0), &candidate, 0.05).is_err());
        for threshold in [0.0, -0.1, f64::NAN, f64::INFINITY] {
            assert!(check(&fixture(10.0), &fixture(12.0), threshold).is_err());
        }
    }
}

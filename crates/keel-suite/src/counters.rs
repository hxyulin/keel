//! Hardware counters of one child process: `/usr/bin/time -l` on macOS and
//! `perf stat -x ,` on Linux.

use std::collections::BTreeMap;

/// Event counts by name, without perf's `:u` modifier: `cycles` and
/// `instructions` on both systems, and any other requested perf events.
pub type Counts = BTreeMap<String, u64>;

/// The counts in `time -l` or `perf stat -x ,` output. Events that perf did
/// not count, or counted for only part of the run (multiplexed), are left out.
pub fn parse(text: &str) -> Counts {
    let mut counts = Counts::new();
    for line in text.lines() {
        let trimmed = line.trim();
        for (suffix, name) in [
            ("instructions retired", "instructions"),
            ("cycles elapsed", "cycles"),
        ] {
            if let Some(Ok(n)) = trimmed
                .strip_suffix(suffix)
                .map(|count| count.trim().parse())
            {
                counts.insert(name.into(), n);
            }
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 5 {
            continue;
        }
        let (Ok(value), Ok(running)) = (fields[0].trim().parse(), fields[4].trim().parse::<f64>())
        else {
            continue;
        };
        if running < 100.0 {
            continue;
        }
        let event = fields[2].trim();
        let event = event.split_once(':').map_or(event, |(name, _)| name);
        counts.insert(event.into(), value);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_time_output() {
        let time = "        1.02 real         0.95 user         0.05 sys
            12345678  maximum resident set size
          4567890123  instructions retired
          1234567890  cycles elapsed
";
        let counts = parse(time);
        assert_eq!(counts["instructions"], 4_567_890_123);
        assert_eq!(counts["cycles"], 1_234_567_890);
        assert_eq!(counts.len(), 2);
        assert!(parse("keel: error\n").is_empty());
    }

    #[test]
    fn parses_perf_output_and_drops_partial_counts() {
        let perf = "# started on Wed Nov  4 23:31:05 2026

keel: warning: something
254729,,cycles:u,264110,100.00,,
144805,,instructions:u,264110,100.00,,
2557,,branch-misses:u,264110,61.20,,
<not counted>,,branches:u,0,0.00,,
";
        let counts = parse(perf);
        assert_eq!(counts["cycles"], 254_729);
        assert_eq!(counts["instructions"], 144_805);
        assert_eq!(counts.len(), 2);
    }
}

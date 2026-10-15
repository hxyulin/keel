#!/usr/bin/env python3
"""Add bootstrap confidence intervals for geometric means in a paired pass report.

Resample paired ratios independently within each fixed workload. Each bootstrap
sample takes the median ratio per workload and their equally weighted geometric
mean. This measures timing uncertainty on this corpus, not uncertainty about
unmeasured workloads, other machines, or systematic measurement bias.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import statistics


def aggregate(rows, rng, samples):
    ratios = []
    for row in rows:
        paired = row["samples_ms"]
        if len(paired["after"]) != len(paired["before"]):
            raise ValueError("mismatched paired timing counts")
        if any(not math.isfinite(v) or v <= 0 for v in paired["after"] + paired["before"]):
            raise ValueError("expected positive finite timings")
        values = [a / b for a, b in zip(paired["after"], paired["before"])]
        if not values or any(not math.isfinite(v) or v <= 0 for v in values):
            raise ValueError("expected nonempty, positive paired timing ratios")
        ratios.append(values)
    if not ratios:
        raise ValueError("expected at least one workload")

    def geomean(values):
        return math.exp(statistics.mean(math.log(v) for v in values))

    point = geomean([statistics.median(v) for v in ratios])
    bootstrap = sorted(
        geomean([statistics.median(rng.choices(v, k=len(v))) for v in ratios])
        for _ in range(samples)
    )
    return {
        "after_over_before": point,
        "ratio_95_interval": [bootstrap[int(samples * 0.025)], bootstrap[int(samples * 0.975)]],
        "workloads": len(rows),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=20000)
    parser.add_argument("--seed", type=int, default=20261015)
    args = parser.parse_args()
    if args.samples < 1000:
        parser.error("use at least 1000 bootstrap samples")
    report = json.loads(args.report.read_text())
    rng = random.Random(args.seed)
    results = {
        name: {kind: aggregate(variant[kind], rng, args.samples) for kind in ["runtime", "compile"]}
        for name, variant in report["variants"].items()
    }
    report["aggregate_confidence"] = {
        "method": __doc__.strip(),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "bootstrap_samples": args.samples,
        "seed": args.seed,
        "variants": results,
    }
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()

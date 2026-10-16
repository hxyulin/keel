"""Summarize retained contract/AVX reports; never drops individual workloads."""
import argparse
import hashlib
import json
import math
import pathlib
import random
import re
import statistics

from measure_passes import summary


def aggregate(rows):
    rng = random.Random(829)
    ratios = [[a/b for a, b in zip(row["samples_ms"]["after"], row["samples_ms"]["before"])] for row in rows]
    def geo(xs):
        return math.exp(statistics.mean(math.log(x) for x in xs))
    estimate = geo([statistics.median(x) for x in ratios])
    boots = sorted(geo([statistics.median(rng.choices(x, k=len(x))) for x in ratios]) for _ in range(2000))
    return dict(after_over_before=estimate, ratio_95_interval=[boots[49], boots[1949]])


def apps(report):
    variant = next(iter(report["variants"].values()))
    return {suite: dict(runtime=aggregate([r for r in variant["runtime"] if r["suite"] == suite]),
                        compile=next(r for r in variant["compile"] if r["suite"] == suite),
                        code=next(r for r in variant["code"] if r["suite"] == suite))
            for suite in ["lua", "quickjs", "sqlite"]}


def rss(report):
    rows = []
    for row in report:
        samples = {"before": [], "after": []}
        peaks = {}
        for name, observations in row["samples"].items():
            values = [s["max_rss_bytes"] if "max_rss_bytes" in s else int(re.search(r"(\d+)  maximum resident set size", s["time_output"]).group(1)) for s in observations]
            peaks[name] = dict(median_bytes=statistics.median(values), min_bytes=min(values), max_bytes=max(values), samples_bytes=values)
            samples[name] = [s["wall_ms"] for s in observations]
        rows.append(dict(suite=row["suite"], input=row["input"], sha256=row["sha256"], rss=peaks, wall=summary(samples, random.Random(42))))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report_dir", type=pathlib.Path)
    args = parser.parse_args()
    result = {"method": "All workload rows retained in host apps.json. Application runtime aggregate: geometric mean of workload paired median ratios; 95% bootstrap resamples pairs within each fixed workload, 2000 draws. Conditional on this machine/corpus; no unmeasured-workload inference. Identical code precludes attributing observed runtime differences to code optimization.", "hosts": {}}
    for host in ["machost", "linuxhost"]:
        directory = args.report_dir / host
        def read(name):
            return json.loads((directory / name).read_text())
        entry = dict(apps=apps(read("apps.json")), rss=rss(read("rss/rss.json")), stress=read("stress/stress.json"))
        if (directory / "runtime-control/runtime-control.json").exists():
            control = read("runtime-control/runtime-control.json")
            entry["same_path_control"] = {suite: aggregate([r for r in control if r["suite"] == suite]) for suite in ["lua", "quickjs", "sqlite"]}
        if (directory / "avx-apps.json").exists():
            entry["avx_apps"] = apps(read("avx-apps.json"))
            entry["avx_kernels"] = read("avx/avx.json")
        result["hosts"][host] = entry
    result["input_hashes"] = {str(p.relative_to(args.report_dir)): hashlib.sha256(p.read_bytes()).hexdigest()
                              for p in sorted(args.report_dir.rglob("*.json")) if p.name != "summary.json"}
    (args.report_dir / "summary.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Extract compact publication evidence without dropping timing samples."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("report", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--build-log", type=Path, action="append", default=[])
    p.add_argument("--campaign-log", type=Path,
                   help="optional JSONL record of the campaign invocation")
    p.add_argument("--build-environment", type=Path,
                   help="JSON with separately recorded Rust version/cache configuration")
    a = p.parse_args()
    report = json.loads(a.report.read_text())
    if not report.get("completed") or not report.get("correctness_passed"):
        p.error("only completed correctness-gated reports can be published")
    result = {k: report[k] for k in ["schema", "environment", "protocol", "correctness_passed",
              "correctness", "ir_sha256", "objects_sha256", "c_to_object", "c_orders_including_warmups",
              "commands", "load_end"]}
    # Clarify the existing Rust harness's check: emitted files are verified
    # references, while timed buffers are compared to them using FNV64.
    result["protocol"] = dict(report["protocol"])
    result["protocol"]["ir_boundary"] = report["protocol"]["ir_boundary"].replace(
        "byte identity checked by existing harness",
        "FNV64 verifier-on/off fingerprints checked per sample by existing harness")
    result["protocol"]["c_boundary"] = report["protocol"]["c_boundary"].replace(
        "frontend, configured optimization",
        "frontend (including Keel frontend-IR verification), configured optimization")
    result["protocol"]["host_control"] = (
        "no enforced quiet-host gate, core pinning, "
        "frequency control or scheduling isolation; load snapshots only")
    result["raw_report_path"] = str(a.report.resolve())
    result["raw_report_sha256"] = hashlib.sha256(a.report.read_bytes()).hexdigest()
    result["support"] = {variant: {"total_functions": len(rows),
        "supported_functions": [r["function"] for r in rows if r["error"] is None],
        "rejections": [r for r in rows if r["error"] is not None]} for variant, rows in report["support"].items()}
    result["ir_to_object"] = report["ir_to_object"]
    if a.build_log:
        result["builds"] = [json.loads(line) for path in a.build_log
                            for line in path.read_text().splitlines() if line]
    if a.campaign_log:
        result["campaign_lock"] = [json.loads(line) for line in
                                   a.campaign_log.read_text().splitlines() if line]
    if a.build_environment:
        result["build_environment"] = json.loads(a.build_environment.read_text())
    for entry in result["ir_to_object"]["inputs"]:
        for config in entry["configs"].values():
            config["median_ms"] = statistics.median(config["total_ns"]) / 1e6
    a.output.parent.mkdir(parents=True, exist_ok=True)
    with a.output.open("x") as out:
        json.dump(result, out, indent=2)
        out.write("\n")


if __name__ == "__main__":
    main()

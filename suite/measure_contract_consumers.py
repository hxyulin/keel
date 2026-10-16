"""Supplement measure_passes.py: provenance, code parity, RSS and bounded IR scaling.

Run only with separate, committed release compilers, after builds settle.
All subprocess commands/results are retained losslessly in gzip JSONL.
"""
import argparse
import gzip
import hashlib
import json
import os
import pathlib
import platform
import random
import statistics
import subprocess
import time

from measure_passes import summary

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOG = []


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def run(cmd, check=True):
    start = time.perf_counter()
    p = subprocess.run(list(map(str, cmd)), capture_output=True)
    ms = (time.perf_counter() - start) * 1000
    LOG.append(dict(command=list(map(str, cmd)), returncode=p.returncode,
                    stdout=p.stdout.decode(errors="replace"), stderr=p.stderr.decode(errors="replace"), wall_ms=ms))
    if check and p.returncode:
        raise RuntimeError(LOG[-1])
    return p, ms


def text(cmd):
    return run(cmd)[0].stdout.decode().strip()


def pairs(before, after):
    samples = {"before": [], "after": []}
    for i in range(13):
        for name in (["before", "after"] if i % 2 == 0 else ["after", "before"]):
            _, ms = run(before if name == "before" else after)
            if i >= 2:
                samples[name].append(ms)
    return summary(samples, random.Random(42))


def provenance(args):
    rows = {}
    for name, binary, checkout in [("before", args.before, args.before_checkout), ("after", args.after, args.after_checkout)]:
        rows[name] = dict(binary=str(binary), sha256=sha(binary), checkout=str(checkout),
                          revision=text(["git", "-C", checkout, "rev-parse", "HEAD"]),
                          crates_tree=text(["git", "-C", checkout, "rev-parse", "HEAD:crates"]),
                          status=text(["git", "-C", checkout, "status", "--porcelain"]))
        assert not rows[name]["status"], rows[name]
    env = {"platform": platform.platform(), "hostname": platform.node(),
           "rustc": text(["rustc", "-Vv"]), "cargo": text(["cargo", "-V"]),
           "clang": text(["clang", "--version"]), "uptime": text(["uptime"]),
           "processes": text(["ps", "-axo", "pid,pcpu,comm"]),
           "environment": {k: os.environ.get(k) for k in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "MACOSX_DEPLOYMENT_TARGET"]}}
    if platform.system() == "Linux":
        env["cpu"] = text(["lscpu"])
        env["affinity"] = sorted(os.sched_getaffinity(0))
        env["governors"] = {str(p): p.read_text().strip() for p in pathlib.Path("/sys/devices/system/cpu").glob("cpu*/cpufreq/scaling_governor")}
    else:
        env["cpu"] = text(["sysctl", "-n", "machdep.cpu.brand_string"])
        env["power"] = text(["pmset", "-g", "custom"])
        env["affinity"] = "macOS: no supported same-core pinning; default scheduler"
    inputs = {str(p): sha(p) for p in sorted((ROOT / "suite").glob("*/*")) if p.is_file()}
    return dict(compilers=rows, environment=env, datasets=inputs,
                production_equivalence=text(["git", "diff", "a0a5eddb", "6e6d2c2", "--", "crates"]),
                harness_revision=text(["git", "-C", ROOT, "rev-parse", "HEAD"]), harness_sha256=sha(__file__))


def inspect_apps(args):
    report = json.loads(args.apps.read_text())
    work = pathlib.Path(report["work_dir"])
    variant = next(iter(report["variants"]))
    result = []
    for suite, binary in [("lua", "lua"), ("quickjs", "qjs"), ("sqlite", "sqlite3")]:
        row = dict(suite=suite, configs={}, objects=[])
        for name in ["baseline", variant]:
            path = work / name / suite / binary
            if platform.system() == "Darwin":
                # otool's hex dump includes only section contents after its two headers.
                section = "\n".join(text(["otool", "-s", "__TEXT", "__text", path]).splitlines()[2:]).encode()
                sizes = text(["size", "-m", path])
            else:
                dump = args.work / f"{suite}-{name}.text"
                run(["objcopy", "--dump-section", f".text={dump}", path])
                section = dump.read_bytes()
                sizes = text(["size", "-A", path])
            row["configs"][name] = dict(path=str(path), sha256=sha(path), bytes=path.stat().st_size,
                                        text_sha256=hashlib.sha256(section).hexdigest(), sections=sizes)
        for obj in sorted((work / "baseline" / suite).glob("*.o")):
            other = work / variant / suite / obj.name
            row["objects"].append(dict(name=obj.name, before_sha256=sha(obj), after_sha256=sha(other),
                                       before_bytes=obj.stat().st_size, after_bytes=other.stat().st_size,
                                       assembly_identical=obj.with_suffix(".s").read_bytes() == other.with_suffix(".s").read_bytes()))
        row["text_identical"] = len({r["text_sha256"] for r in row["configs"].values()}) == 1
        result.append(row)
    return result


def rss(args):
    # Separate end-to-end C-to-assembly CLI observations; no assembly or link.
    # One largest preprocessed TU per application, fixed input for both compilers.
    rows = []
    for suite in ["lua", "quickjs", "sqlite"]:
        source = max((args.prepared / suite / "pre").glob("*.i"), key=lambda p: p.stat().st_size)
        commands = {}
        for name, driver in [("before", args.before), ("after", args.after)]:
            commands[name] = ["/usr/bin/time", "-l" if platform.system() == "Darwin" else "-v", driver,
                              "cc", "--opt-tier", "full", source, "-o", "/dev/null"]
        samples = {"before": [], "after": []}
        for i in range(13):
            for name in (["before", "after"] if i % 2 == 0 else ["after", "before"]):
                p, ms = run(commands[name])
                if i >= 2:
                    samples[name].append(dict(wall_ms=ms, time_output=p.stderr.decode()))
        rows.append(dict(suite=suite, input=str(source), sha256=sha(source), bytes=source.stat().st_size, samples=samples))
    return rows


def generated(kind, n):
    if kind == "invoke":
        lines = ["decl @g(i32) -> i32 nounwind", "decl @personality()", "func @probe(i32) -> i32 personality @personality {", "block0(v0: i32):"]
        for i in range(n):
            if i:
                lines += [f"block{i}:"]
            lines += [f" v{i+1} = invoke @g(v{i}), block{i+1}, block{n+1}"]
        lines += [f"block{n}:", f" return v{n}", f"block{n+1}:", f" v{n+1}, v{n+2} = landingpad cleanup", f" return v{n+2}", "}"]
    else:
        lines = ["decl @stop(i32) noreturn", "decl @effect(i32)", "func @probe(i32) {", "block0(v0: i32):"]
        for i in range(n):
            if i:
                lines += [f"block{i}:"]
            lines += [f" v{2*i+1} = iconst.i32 {i}", f" v{2*i+2} = icmp eq v0, v{2*i+1}", f" brif v{2*i+2}, block{n+i+1}, block{i+1}"]
        lines += [f"block{n}:", " return"]
        for i in range(n):
            lines += [f"block{n+i+1}:", " call @stop(v0)", " call @effect(v0)", " return"]
        lines += ["}"]
    return "\n".join(lines) + "\n"


def stress(args):
    rows = []
    for kind in ["call", "invoke"]:
        for n in [1, 8, 32, 128]:
            source = args.work / f"{kind}-{n}.keel"
            source.write_text(generated(kind, n))
            commands = {}
            outputs = {}
            for name, driver in [("before", args.before), ("after", args.after)]:
                commands[name] = [driver, "compile", "--opt-tier", "full", "--emit-ir", source]
                p, _ = run(commands[name])
                dest = args.work / f"{kind}-{n}-{name}.ir"
                dest.write_bytes(p.stdout)
                outputs[name] = dict(path=str(dest), sha256=sha(dest), bytes=len(p.stdout),
                                     invokes=p.stdout.count(b"invoke @g"), suffix_effects=p.stdout.count(b"call @effect"))
            assert (outputs["before"]["invokes"] == n and outputs["after"]["invokes"] == 0) if kind == "invoke" else (outputs["before"]["suffix_effects"] == n and outputs["after"]["suffix_effects"] == 0)
            rows.append(dict(kind=kind, eligible_sites=n, source=str(source), sha256=sha(source),
                             outputs=outputs, timing=pairs(commands["before"], commands["after"])))
    return rows


def probes(args):
    rows = []
    trap = args.work / "trap.keel"
    trap.write_text("decl @before()\ndecl @stop() noreturn\ndecl @after()\nfunc @main() -> i32 {\nblock0:\n call @before()\n call @stop()\n call @after()\n v0 = iconst.i32 0\n return v0\n}\n")
    for contract_violation in [False, True]:
        driver = args.work / f"driver-{contract_violation}.c"
        driver.write_text('#include <unistd.h>\n#include <stdlib.h>\nvoid before(void){write(1,"before\\n",7);}\nvoid after(void){write(1,"after\\n",6);}\nvoid stop(void){write(1,"stop\\n",5);' + ('return;' if contract_violation else '_Exit(17);') + '}\n')
        for name, compiler in [("before", args.before), ("after", args.after)]:
            for tier in ["none", "basic", "full"]:
                asm = args.work / f"trap-{contract_violation}-{name}-{tier}.s"
                run([compiler, "compile", "--opt-tier", tier, trap, "-o", asm])
                exe = asm.with_suffix("")
                run(["clang", asm, driver, "-o", exe])
                p, _ = run([exe], check=False)
                cut = name == "after" and tier != "none"
                if not contract_violation:
                    assert p.returncode == 17 and p.stdout == b"before\nstop\n"
                elif cut:
                    assert p.returncode in [-4, -5] and p.stdout == b"before\nstop\n"
                else:
                    assert p.returncode == 0 and p.stdout == b"before\nstop\nafter\n"
                rows.append(dict(kind="noreturn", violation=contract_violation, config=name, tier=tier, returncode=p.returncode, stdout=p.stdout.decode(), assembly_sha256=sha(asm)))
    source = ROOT / "docs/review-evidence/function-contract-consumers/invoke.keel"
    driver = args.work / "invoke-driver.c"
    driver.write_text("int probe(int); int g(int x){return x+1;} int personality(void){return 0;} int main(void){return probe(41)==42?0:1;}\n")
    for name, compiler in [("before", args.before), ("after", args.after)]:
        for tier in ["none", "basic", "full"]:
            asm = args.work / f"invoke-{name}-{tier}.s"
            p, _ = run([compiler, "compile", "--opt-tier", tier, source, "-o", asm], check=False)
            if platform.machine() == "x86_64":
                assert p.returncode and b"exceptions are not supported on x86-64 yet" in p.stderr
                rows.append(dict(kind="invoke", config=name, tier=tier, existing_backend_rejection=True))
            else:
                assert not p.returncode
                run(["clang", asm, driver, "-o", asm.with_suffix("")])
                run([asm.with_suffix("")])
                rows.append(dict(kind="invoke", config=name, tier=tier, returncode=0, assembly_sha256=sha(asm)))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["provenance", "inspect-apps", "rss", "stress", "probes"])
    parser.add_argument("--before", type=pathlib.Path, default=pathlib.Path("/tmp/keel-bench-target-f0d36f1/release/keel"))
    parser.add_argument("--after", type=pathlib.Path, default=pathlib.Path("/tmp/keel-bench-target-6e6d2c2/release/keel"))
    parser.add_argument("--before-checkout", type=pathlib.Path, default=pathlib.Path("/tmp/keel-bench-f0d36f1"))
    parser.add_argument("--after-checkout", type=pathlib.Path, default=pathlib.Path("/tmp/keel-bench-6e6d2c2"))
    parser.add_argument("--prepared", type=pathlib.Path)
    parser.add_argument("--apps", type=pathlib.Path)
    parser.add_argument("--work", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    try:
        result = {"provenance": provenance, "inspect-apps": inspect_apps, "rss": rss, "stress": stress, "probes": probes}[args.mode](args)
        (args.work / f"{args.mode}.json").write_text(json.dumps(result, indent=2) + "\n")
    finally:
        with gzip.open(args.work / f"{args.mode}-commands.jsonl.gz", "wt") as f:
            for row in LOG:
                f.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()

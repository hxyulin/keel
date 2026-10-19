#!/usr/bin/env python3
"""Same-IR comparison of the native Keel backend and the Cranelift bridge.

Order: environment record -> Keel IR generation -> correctness gate (every
configuration linked with a Clang-built driver and checked against two Clang
oracles) -> in-process compile timing from identical parsed IR -> exploratory
runtime timing. Timing never starts unless every correctness check passed.

Requires prebuilt binaries (no builds happen here). Writes only into a fresh
--out directory and refuses to overwrite an existing manifest.
"""
import argparse
import hashlib
import json
import math
import pathlib
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import time

CRATE = pathlib.Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
CONFIGS = ["keel-baseline", "keel-full", "clif-none", "clif-speed"]
# Runtime arms: each IR variant with its matching native configuration.
RUNTIME_ARMS = [
    ("raw", "keel-baseline"), ("raw", "clif-none"), ("raw", "clif-speed"),
    ("O", "keel-full"), ("O", "clif-none"), ("O", "clif-speed"),
]
# Calls per timed run; about 30 ms with Clang -O2 on an Apple M5 Max.
KERNEL_REPS = {
    "logic": 40000, "fnv1a": 8000, "crc32": 600, "sieve": 400, "matmul": 600,
    "dot": 15000, "saxpy": 100000, "divmix": 6000, "gcd": 80, "isort": 400,
    "qsort": 700, "interp": 100, "classify": 8000, "bits": 8000, "narrow": 25000,
    "fconv": 6000, "collatz": 300, "copy": 8000, "dispatch": 1500, "names": 10000,
    "stackobj": 40000,
}

ap = argparse.ArgumentParser(description=__doc__)
ap.add_argument("--out", type=pathlib.Path, required=True)
ap.add_argument("--keel", type=pathlib.Path, default=ROOT / "target/keel-cranelift/release/keel")
ap.add_argument("--bench", type=pathlib.Path,
                default=ROOT / "target/keel-cranelift/release/keel-cranelift-bench")
ap.add_argument("--bridge", type=pathlib.Path,
                default=ROOT / "target/keel-cranelift/release/keel-cranelift")
ap.add_argument("--clang", default="clang")
ap.add_argument("--warmup", type=int, default=5)
ap.add_argument("--reps", type=int, default=31)
ap.add_argument("--rounds", type=int, default=7, help="runtime rounds per kernel and arm")
ap.add_argument("--seed", type=int, default=20261019)
ap.add_argument("--lua-pre", type=pathlib.Path, default=ROOT / "target/suite/lua/pre",
                help="preprocessed Lua .i files for coverage and compile timing; skipped if absent")
ap.add_argument("--skip-runtime", action="store_true")
ap.add_argument("--git-root", type=pathlib.Path, default=ROOT,
                help="repository whose HEAD/status is recorded (for builds from a source snapshot)")
a = ap.parse_args()
out = a.out.resolve()
out.mkdir(parents=True, exist_ok=True)
if (out / "manifest.json").exists():
    ap.error(f"{out} already has a manifest; choose a fresh --out")
for d in ["ir", "objects", "bin", "correctness", "lua"]:
    (out / d).mkdir(exist_ok=True)

commands = []
manifest = {"purpose": "same-Keel-IR native vs Cranelift backend comparison; kernels scope",
            "commands": commands}


def run(*cmd, ok=(0,), capture=True):
    cmd = [str(c) for c in cmd]
    t0 = time.time()
    p = subprocess.run(cmd, cwd=ROOT, capture_output=capture)
    commands.append({"argv": cmd, "returncode": p.returncode, "seconds": round(time.time() - t0, 3)})
    if p.returncode not in ok:
        sys.stderr.write(p.stderr.decode(errors="replace") if capture else "")
        raise RuntimeError(f"command failed ({p.returncode}): {' '.join(cmd)}")
    return p.stdout.decode(errors="replace") if capture else ""


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def save():
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")


def lock_versions():
    text = (CRATE / "Cargo.lock").read_text()
    pkgs = re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', text)
    keep = ("cranelift", "regalloc2", "object", "target-lexicon", "gimli")
    return sorted({f"{n} {v}" for n, v in pkgs if n.startswith(keep)})


# --- environment --------------------------------------------------------
for b in [a.keel, a.bench, a.bridge]:
    if not b.is_file():
        ap.error(f"missing prebuilt binary {b}; see crates/keel-cranelift/README.md")
manifest["environment"] = {
    "source_root": str(ROOT),
    "git_root": str(a.git_root),
    "git_head": run("git", "-C", a.git_root, "rev-parse", "HEAD").strip(),
    "git_tree": run("git", "-C", a.git_root, "rev-parse", "HEAD^{tree}").strip(),
    "git_status": run("git", "-C", a.git_root, "status", "--porcelain", "--untracked-files=all", "--", "crates", "Cargo.toml", "Cargo.lock"),
    "uname": platform.uname()._asdict(),
    "sw_vers": run("sw_vers"),
    "cpu": run("sysctl", "-n", "machdep.cpu.brand_string").strip(),
    "hw": run("sysctl", "hw.model", "hw.memsize", "hw.ncpu", "hw.perflevel0.physicalcpu",
              "hw.perflevel1.physicalcpu"),
    "power": run("pmset", "-g", "batt", ok=(0, 1)),
    "loadavg_start": run("sysctl", "-n", "vm.loadavg").strip(),
    "rustc": run("rustc", "-vV"),
    "cargo": run("cargo", "-V").strip(),
    "clang": run(a.clang, "--version"),
    "cranelift_lock": lock_versions(),
    "binaries": {str(b): sha(b) for b in [a.keel, a.bench, a.bridge]},
    "sources": {str(p.relative_to(ROOT)): sha(p) for p in sorted(CRATE.rglob("*"))
                if p.is_file() and "target" not in p.parts and p.suffix in {".rs", ".c", ".keel", ".py", ".toml", ".lock", ".md"}},
}
manifest["protocol"] = {"warmup": a.warmup, "reps": a.reps, "rounds": a.rounds, "seed": a.seed,
                        "kernel_reps": KERNEL_REPS, "runtime_arms": RUNTIME_ARMS}
save()

# --- Keel IR from C ----------------------------------------------------------
kernels_c = CRATE / "kernels/kernels.c"
driver_c = CRATE / "kernels/driver.c"
ir = {}
ir["kernels.raw"] = out / "ir/kernels.raw.keel"
ir["kernels.O"] = out / "ir/kernels.O.keel"
run(a.keel, "cc", "--emit-ir", "--target", "aarch64-apple-darwin", kernels_c, "-o", ir["kernels.raw"])
run(a.keel, "cc", "-O", "--emit-ir", "--target", "aarch64-apple-darwin", kernels_c, "-o", ir["kernels.O"])
ir["bridge"] = out / "ir/bridge.keel"
shutil.copy(CRATE / "tests/fixtures/bridge.keel", ir["bridge"])

# --- correctness gate -----------------------------------------------------------
gate = {}
cc_driver = out / "bin/driver.o"
run(a.clang, "-O2", "-ffp-contract=off", "-c", driver_c, "-o", cc_driver)
outputs = {}
for level in ["-O0", "-O2"]:
    obj = out / f"objects/kernels.clang{level}.o"
    run(a.clang, level, "-ffp-contract=off", "-c", kernels_c, "-o", obj)
    exe = out / f"bin/clang{level}"
    run(a.clang, cc_driver, obj, "-o", exe)
    outputs[f"clang{level}"] = run(exe, "check")
for variant in ["raw", "O"]:
    for line in run(a.bench, "objects", "--out-dir", out / "objects", "--label", f"kernels.{variant}",
                    ir[f"kernels.{variant}"]).splitlines():
        rec = json.loads(line)
        exe = out / f"bin/{variant}-{rec['config']}"
        run(a.clang, cc_driver, rec["path"], "-o", exe)
        outputs[f"{variant}-{rec['config']}"] = run(exe, "check")
oracle = outputs["clang-O0"]
for name, text in outputs.items():
    (out / f"correctness/kernels.{name}.out").write_text(text)
    gate[f"kernels {name}"] = {"agrees_with_clang-O0": text == oracle, "lines": text.count("\n"),
                               "sha256": hashlib.sha256(text.encode()).hexdigest()}
bridge_driver = CRATE / "tests/fixtures/bridge_driver.c"
bridge_out = {}
for line in run(a.bench, "objects", "--out-dir", out / "objects", "--label", "bridge", ir["bridge"]).splitlines():
    rec = json.loads(line)
    exe = out / f"bin/bridge-{rec['config']}"
    run(a.clang, "-O1", bridge_driver, rec["path"], "-o", exe)
    bridge_out[rec["config"]] = run(exe)
    traps = {}
    for case in ["sdiv0", "udiv0", "srem0", "urem0", "sdivmin", "sdiv8min", "trap"]:
        p = subprocess.run([str(exe), "trap", case], capture_output=True)
        traps[case] = p.returncode  # negative: killed by that signal
    gate[f"bridge {rec['config']} traps"] = {"returncodes": traps,
                                             "all_abnormal": all(rc < 0 for rc in traps.values())}
for name, text in bridge_out.items():
    (out / f"correctness/bridge.{name}.out").write_text(text)
    gate[f"bridge {name}"] = {"agrees_with_keel-baseline": text == bridge_out["keel-baseline"],
                              "lines": text.count("\n")}
manifest["correctness"] = gate
failed = [k for k, v in gate.items() if not all(x for x in v.values() if isinstance(x, bool))]
manifest["correctness_passed"] = not failed
save()
if failed:
    sys.exit(f"correctness gate failed: {failed}; no timing performed")
print("correctness gate passed", file=sys.stderr)

# --- Lua coverage and compile-only subsets -------------------------------------
lua = {"available": a.lua_pre.is_dir()}
timing_inputs = [("kernels.raw", ir["kernels.raw"]), ("kernels.O", ir["kernels.O"]), ("bridge", ir["bridge"])]
if lua["available"]:
    lua["sources"] = str(a.lua_pre)
    lua["tus"] = {}
    support_lines = []
    for src in sorted(a.lua_pre.glob("*.i")):
        tu = src.stem
        for variant, flags in [("raw", []), ("O", ["-O"])]:
            keel_ir = out / f"lua/{tu}.{variant}.keel"
            run(a.keel, "cc", *flags, "--emit-ir", "--target", "aarch64-apple-darwin", src, "-o", keel_ir)
            if variant == "raw":
                support_lines += run(a.bench, "support", keel_ir).splitlines()
            sub = out / f"lua/{tu}.{variant}.subset.keel"
            rec = json.loads(run(a.bench, "subset", keel_ir, sub))
            lua["tus"][f"{tu}.{variant}"] = {"kept": rec["kept"], "rejected": rec["rejected"],
                                             "source_sha256": sha(src)}
            timing_inputs.append((f"lua.{tu}.{variant}", sub))
    (out / "lua/support.raw.jsonl").write_text("\n".join(support_lines) + "\n")
    rows = [json.loads(l) for l in support_lines]
    reasons = {}
    for r in rows:
        if r["error"]:
            m = re.search(r"@[^:]+: (variadic signatures|returns_twice signatures)", r["error"])
            key = m.group(1) if m else re.sub(r"^function @\S+: (block\d+, inst\d+ \(.*\): )?", "", r["error"])[:120]
            reasons[key] = reasons.get(key, 0) + 1
    lua["raw_functions"] = len(rows)
    lua["raw_supported"] = sum(r["error"] is None for r in rows)
    lua["raw_rejection_reasons"] = reasons
manifest["lua"] = lua
save()

# --- compile timing (in-process, same parsed IR, no IR passes) --------------------
compile_json = out / "compile-time.json"
run(a.bench, "time", "--warmup", a.warmup, "--reps", a.reps, "--out", compile_json,
    "--emit-dir", out / "objects",
    *[f"{label}={path}" for label, path in timing_inputs])
# The timed objects are byte-identical to the objects that passed the gate.
for variant in ["raw", "O"]:
    for config in CONFIGS:
        gated = out / f"objects/kernels.{variant}.{config}.o"
        assert gated.is_file()
compile_data = json.loads(compile_json.read_text())
manifest["objects"] = {p.name: sha(p) for p in sorted((out / "objects").glob("*.o"))}
manifest["loadavg_after_compile"] = run("sysctl", "-n", "vm.loadavg").strip()
save()

# --- static code size -------------------------------------------------------------
def text_size(obj):
    m = re.search(r"Section \(__TEXT, __text\): (\d+)", run("size", "-m", obj))
    return int(m.group(1)) if m else None


sizes = {f"{v}.{c}": text_size(out / f"objects/kernels.{v}.{c}.o") for v in ["raw", "O"] for c in CONFIGS}
sizes["clang-O2"] = text_size(out / "objects/kernels.clang-O2.o")
sizes["clang-O0"] = text_size(out / "objects/kernels.clang-O0.o")
manifest["kernels_text_bytes"] = sizes
save()

# --- exploratory runtime -----------------------------------------------------------
runtime_rows = []
if not a.skip_runtime:
    rng = random.Random(a.seed)
    arms = [f"{v}-{c}" for v, c in RUNTIME_ARMS] + ["clang-O2"]
    exes = {arm: out / f"bin/{arm}" for arm in arms}
    for rnd in range(a.rounds):
        for kernel, reps in KERNEL_REPS.items():
            order = arms[:]
            rng.shuffle(order)
            checks = {}
            for arm in order:
                line = run(exes[arm], "time", kernel, reps).split()
                runtime_rows.append({"round": rnd, "kernel": kernel, "arm": arm, "reps": reps,
                                     "ns": int(line[1]), "checksum": line[2]})
                checks[arm] = line[2]
            if len(set(checks.values())) != 1:
                sys.exit(f"timed checksum mismatch for {kernel}: {checks}")
    (out / "runtime.jsonl").write_text("".join(json.dumps(r) + "\n" for r in runtime_rows))
manifest["loadavg_end"] = run("sysctl", "-n", "vm.loadavg").strip()
save()

# --- summary ------------------------------------------------------------------------
def med(xs):
    return statistics.median(xs)


def gmean(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


lines = ["# Keel native backend vs Cranelift bridge: same Keel IR", ""]
env = manifest["environment"]
lines += [f"- HEAD `{env['git_head']}`; host {env['cpu']}, {env['sw_vers'].split()[3] if len(env['sw_vers'].split()) > 3 else ''}",
          f"- rustc: {env['rustc'].splitlines()[0]}; clang: {env['clang'].splitlines()[0]}",
          f"- Cranelift: {', '.join(x for x in env['cranelift_lock'] if x.startswith('cranelift-codegen '))}",
          f"- Correctness gate: {'PASSED' if manifest['correctness_passed'] else 'FAILED'} "
          f"({sum(1 for k in gate if k.startswith('kernels'))} kernel builds vs Clang -O0 oracle, "
          f"{sum(1 for k in gate if k.startswith('bridge') and 'traps' not in k)} bridge-fixture builds, trap cases)", ""]
lines += ["## Compile time: Keel IR -> Mach-O object, in-process (ms, median of "
          f"{compile_data['reps']} after {compile_data['warmup']} warmup)", "",
          "Parse+verify is shared and excluded from every total. Native stages: codegen (IR -> typed",
          "assembly program incl. register allocation) + object (integrated assembler encoding + Mach-O).",
          "Cranelift stages: setup + translate (Keel -> CLIF) + codegen (incl. machine-code emission) +",
          "object (definitions + Mach-O). No external assembler or linker runs in any configuration.", "",
          "| input | fns | Keel insts | parse+verify | keel-baseline | keel-full | clif-none (translate) | clif-speed (translate) | none/baseline | speed/full |",
          "|---|---|---|---|---|---|---|---|---|---|"]
tot = {c: [] for c in CONFIGS}
lua_tot = {c: 0.0 for c in CONFIGS}
for inp in compile_data["inputs"]:
    m = {c: med(inp["configs"][c]["total_ns"]) / 1e6 for c in CONFIGS}
    tr = {c: med(inp["configs"][c]["stages_ns"]["translate"]) / 1e6 for c in ["clif-none", "clif-speed"]}
    for c in CONFIGS:
        tot[c].append(m[c])
        if inp["label"].startswith("lua."):
            lua_tot[c] += m[c]
    if not inp["label"].startswith("lua."):
        lines.append(f"| {inp['label']} | {inp['functions']} | {inp['keel_instructions']} | "
                     f"{med(inp['parse_verify_ns']) / 1e6:.3f} | {m['keel-baseline']:.3f} | {m['keel-full']:.3f} | "
                     f"{m['clif-none']:.3f} ({tr['clif-none']:.3f}) | {m['clif-speed']:.3f} ({tr['clif-speed']:.3f}) | "
                     f"{m['clif-none'] / m['keel-baseline']:.2f} | {m['clif-speed'] / m['keel-full']:.2f} |")
if lua["available"]:
    for variant in ["raw", "O"]:
        sel = [i for i in compile_data["inputs"] if i["label"].startswith("lua.") and i["label"].endswith("." + variant)]
        s = {c: sum(med(i["configs"][c]["total_ns"]) for i in sel) / 1e6 for c in CONFIGS}
        st = {c: {k: sum(med(i["configs"][c]["stages_ns"][k]) for i in sel) / 1e6
                  for k in sel[0]["configs"][c]["stages_ns"]} for c in CONFIGS}
        fns = sum(i["functions"] for i in sel)
        insts = sum(i["keel_instructions"] for i in sel)
        lines.append(f"| Lua subset, {variant} IR, sum of {len(sel)} TUs | {fns} | {insts} | "
                     f"{sum(med(i['parse_verify_ns']) for i in sel) / 1e6:.3f} | {s['keel-baseline']:.3f} | {s['keel-full']:.3f} | "
                     f"{s['clif-none']:.3f} ({st['clif-none']['translate']:.3f}) | {s['clif-speed']:.3f} ({st['clif-speed']['translate']:.3f}) | "
                     f"{s['clif-none'] / s['keel-baseline']:.2f} | {s['clif-speed'] / s['keel-full']:.2f} |")
        manifest.setdefault("lua_compile_stage_sums_ms", {})[variant] = st
lines += ["", "Stage medians for every input are in compile-time.json (`stages_ns`).", ""]
lines += ["## Kernel __text bytes (static size, not a speed measure)", "",
          "| object | __text bytes |", "|---|---|"] + [f"| {k} | {v} |" for k, v in sizes.items()] + [""]
if runtime_rows:
    arms = [f"{v}-{c}" for v, c in RUNTIME_ARMS] + ["clang-O2"]
    by = {}
    for r in runtime_rows:
        by.setdefault((r["kernel"], r["arm"]), []).append(r["ns"] / r["reps"])
    lines += ["## Exploratory runtime (ns per kernel call, median of rounds; local desktop)", "",
              "Each value is the in-process median over rounds of one timed loop; process startup is excluded.",
              "Arms are interleaved in a seeded random order. No core pinning or page-placement control.", "",
              "| kernel | " + " | ".join(a if a != "clang-O2" else "clang-O2 (C->obj ref)" for a in arms) + " | raw none/keel | raw speed/keel | O none/keel-O | O speed/keel-O | max spread |",
              "|---|" + "---|" * (len(arms) + 5)]
    ratios = {k: [] for k in ["raw none/keel", "raw speed/keel", "O none/keel-O", "O speed/keel-O", "keel/clang", "keel-O/clang", "raw speed/clang", "O speed/clang"]}
    for kernel in KERNEL_REPS:
        mm = {arm: med(by[(kernel, arm)]) for arm in arms}
        spread = max((max(by[(kernel, arm)]) - min(by[(kernel, arm)])) / mm[arm] for arm in arms)
        rr = [mm["raw-clif-none"] / mm["raw-keel-baseline"], mm["raw-clif-speed"] / mm["raw-keel-baseline"],
              mm["O-clif-none"] / mm["O-keel-full"], mm["O-clif-speed"] / mm["O-keel-full"]]
        for key, v in zip(list(ratios)[:4], rr):
            ratios[key].append(v)
        ratios["keel/clang"].append(mm["raw-keel-baseline"] / mm["clang-O2"])
        ratios["keel-O/clang"].append(mm["O-keel-full"] / mm["clang-O2"])
        ratios["raw speed/clang"].append(mm["raw-clif-speed"] / mm["clang-O2"])
        ratios["O speed/clang"].append(mm["O-clif-speed"] / mm["clang-O2"])
        lines.append(f"| {kernel} | " + " | ".join(f"{mm[arm]:.0f}" for arm in arms) + " | "
                     + " | ".join(f"{x:.2f}" for x in rr) + f" | {spread:.1%} |")
    lines += ["", "Equal-kernel geometric means of per-kernel median ratios (lower = faster numerator):", ""]
    lines += [f"- {k}: {gmean(v):.3f} (range {min(v):.2f}-{max(v):.2f})" for k, v in ratios.items()]
    manifest["runtime_geomeans"] = {k: gmean(v) for k, v in ratios.items()}
    lines.append("")
lines += ["## Scope and caveats", "",
          "- Same Keel IR into both backends; no claim about C-to-object or whole-application builds.",
          "- Compile times are single-threaded in-process wall clock on a desktop; kernels are small, so",
          "  per-input times are sub-millisecond and dominated by per-function fixed costs.",
          "- Lua rows compile only the functions the bridge accepts (unsupported ones become declarations);",
          "  they measure code generation on real code, not a Lua build, and nothing from them is executed.",
          "- Runtime numbers are exploratory: one macOS desktop, no core pinning, no physical page control;",
          "  publication-quality runtime needs the fixed-page LinuxHost protocol, which was not run.",
          ""]
(out / "summary.md").write_text("\n".join(lines))
manifest["completed"] = True
save()
print(out / "summary.md")

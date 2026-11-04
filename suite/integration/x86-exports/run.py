#!/usr/bin/env python3
"""Build and run exported native x86-64 test programs on a Linux host.

Tests export packets with `KEEL_TEST_EXPORT_DIR=<dir>`. Each packet is a
directory holding its files and a `manifest.json`; see
crates/keel-test-support/src/native.rs for the format. This runner copies each
packet to a scratch directory, runs its build steps and its program, and
compares the exit status and standard output with the manifest. A packet whose
CPU features the host lacks, or whose build tool is missing, is skipped.

A packet of kind "cpl0" holds privileged payload objects. The runner passes a
copy of it to suite/integration/x86-cpl0/descriptor_campaign.py, which links
each object into a QEMU guest; these packets also run on hosts other than
x86-64 Linux. They are skipped when QEMU or GNU binutils are missing.

    python3 suite/integration/x86-exports/run.py <dir> [--cc clang] [--jobs N]

The exit status is 1 when any packet fails or no packet is found.
"""
from __future__ import annotations

import argparse
import collections
import concurrent.futures
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

VERSION = 1
CAMPAIGN = Path(__file__).resolve().parent.parent / "x86-cpl0" / "descriptor_campaign.py"
NOT_NATIVE = "requires x86-64 Linux"

# is_x86_feature_detected! names that /proc/cpuinfo spells differently.
CPUINFO_NAMES = {
    "sse3": "pni",
    "sse4.1": "sse4_1",
    "sse4.2": "sse4_2",
    "lzcnt": "abm",
    "cmpxchg16b": "cx16",
    "sha": "sha_ni",
}
FEATURES = {
    "sse2", "sse3", "ssse3", "sse4.1", "sse4.2", "popcnt", "avx", "avx2",
    "fma", "f16c", "bmi1", "bmi2", "lzcnt", "movbe", "pclmulqdq",
    "vpclmulqdq", "aes", "sha", "cmpxchg16b", "xsave", "xsaveopt", "xsavec",
    "avx512f", "avx512cd", "avx512bw", "avx512dq", "avx512vl",
}


@dataclass
class Tools:
    """The programs a packet may need, as names or paths."""
    cc: str
    qemu: str = "qemu-system-x86_64"
    binutils_prefix: str = ""
    native: bool = True

    def binutils(self) -> dict[str, str]:
        return {name: self.binutils_prefix + name for name in ("as", "ld", "objcopy", "objdump")}


@dataclass
class Result:
    packet: str
    status: str  # "pass", "fail" or "skip"
    detail: str = ""


def host_flags(cpuinfo: Path) -> set[str]:
    """The CPU flags of the first processor in a /proc/cpuinfo file."""
    for line in cpuinfo.read_text().splitlines():
        key, _, value = line.partition(":")
        if key.strip() == "flags":
            return set(value.split())
    return set()


def missing_features(features: list[str], flags: set[str]) -> list[str]:
    return [f for f in features if CPUINFO_NAMES.get(f, f) not in flags]


def load_manifest(path: Path) -> dict:
    manifest = json.loads(path.read_text())
    if manifest.get("version") != VERSION:
        raise ValueError(f"unsupported manifest version {manifest.get('version')!r}")
    for key in ("test", "name", "features", "files", "build", "run", "exit"):
        if key not in manifest:
            raise ValueError(f"manifest lacks {key!r}")
    kind = manifest.setdefault("kind", "native")
    if kind == "cpl0":
        if not manifest.get("campaign"):
            raise ValueError("cpl0 manifest lacks 'campaign'")
        return manifest
    if kind != "native":
        raise ValueError(f"unknown packet kind {kind!r}")
    unknown = sorted(set(manifest["features"]) - FEATURES)
    if unknown:
        raise ValueError(f"unknown CPU features {unknown}")
    if not manifest["run"] or any(not step for step in manifest["build"]):
        raise ValueError("empty build step or run command")
    return manifest


def find_packets(root: Path) -> list[Path]:
    return sorted(path.parent for path in root.rglob("manifest.json"))


def run_packet(packet: Path, root: Path, tools: Tools, flags: set[str], timeout: float,
               keep: Path | None) -> Result:
    label = str(packet.relative_to(root))
    try:
        manifest = load_manifest(packet / "manifest.json")
    except (OSError, ValueError) as error:
        return Result(label, "fail", f"bad manifest: {error}")
    if manifest["kind"] == "cpl0":
        needed = [tools.qemu, *tools.binutils().values()]
    else:
        if not tools.native:
            return Result(label, "skip", NOT_NATIVE)
        missing = missing_features(manifest["features"], flags)
        if missing:
            return Result(label, "skip", f"host lacks {missing[0]}")
        needed = [tools.cc if step[0] == "cc" else step[0] for step in manifest["build"]]
    for tool in needed:
        if shutil.which(tool) is None:
            return Result(label, "skip", f"{tool} not found")
    for name in manifest["files"]:
        if not (packet / name).is_file():
            return Result(label, "fail", f"missing file {name}")
    run_in = _run_campaign if manifest["kind"] == "cpl0" else _run_in
    if keep is not None:
        work = keep / label
        shutil.rmtree(work, ignore_errors=True)
        work.parent.mkdir(parents=True, exist_ok=True)
        return run_in(packet, work, manifest, label, tools, timeout)
    with tempfile.TemporaryDirectory(prefix="keel-x86-export-") as scratch:
        return run_in(packet, Path(scratch) / "packet", manifest, label, tools, timeout)


def _run_campaign(packet: Path, work: Path, manifest: dict, label: str, tools: Tools,
                  timeout: float) -> Result:
    """Runs a cpl0 packet through descriptor_campaign.py; exit 0 is a pass."""
    shutil.copytree(packet, work)
    (work / "manifest.json").unlink()
    campaign = manifest["campaign"]
    binutils = tools.binutils()
    argv = [sys.executable, str(CAMPAIGN), "--exports", str(work),
            "--qemu", tools.qemu, "--as", binutils["as"], "--ld", binutils["ld"],
            "--objcopy", binutils["objcopy"], "--objdump", binutils["objdump"]]
    if campaign != "observe":
        argv.append(f"--{campaign}")
    try:
        done = subprocess.run(argv, cwd=work, capture_output=True, text=True,
                              timeout=timeout * 10)
    except subprocess.TimeoutExpired as error:
        return Result(label, "fail", f"timed out after {timeout * 10:g} s: {error.cmd}")
    if done.returncode != 0:
        return Result(label, "fail", f"campaign {campaign} exited {done.returncode}\n"
                      f"{done.stdout}{done.stderr}")
    return Result(label, "pass")


def _run_in(packet: Path, work: Path, manifest: dict, label: str, tools: Tools,
            timeout: float) -> Result:
    cc = tools.cc
    shutil.copytree(packet, work)
    try:
        for step in manifest["build"]:
            argv = [cc, *step[1:]] if step[0] == "cc" else list(step)
            done = subprocess.run(argv, cwd=work, capture_output=True, text=True,
                                  timeout=timeout)
            if done.returncode != 0:
                return Result(label, "fail", f"build {argv} exited {done.returncode}\n"
                              f"{done.stdout}{done.stderr}")
        run = list(manifest["run"])
        done = subprocess.run(run, cwd=work, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        return Result(label, "fail", f"timed out after {timeout:g} s: {error.cmd}")
    stdout = done.stdout.decode(errors="replace")
    stderr = done.stderr.decode(errors="replace")
    if done.returncode != manifest["exit"]:
        return Result(label, "fail", f"{run} exited {done.returncode}, expected "
                      f"{manifest['exit']}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    expected = manifest.get("stdout")
    if expected is not None and stdout != expected:
        return Result(label, "fail", f"stdout {stdout!r}, expected {expected!r}")
    return Result(label, "pass")


def native_host() -> bool:
    return platform.system() == "Linux" and platform.machine() in ("x86_64", "amd64")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("dir", type=Path, help="the KEEL_TEST_EXPORT_DIR the tests wrote")
    parser.add_argument("--cc", default=os.environ.get("KEEL_CLANG") or "clang",
                        help="C compiler for 'cc' build steps (default: $KEEL_CLANG or clang)")
    parser.add_argument("--qemu", default="qemu-system-x86_64",
                        help="QEMU for cpl0 packets (default: qemu-system-x86_64)")
    parser.add_argument("--binutils-prefix", default=None,
                        help="prefix of the GNU as, ld, objcopy and objdump for cpl0 packets "
                             "(default: none on Linux, x86_64-elf- elsewhere)")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
    parser.add_argument("--timeout", type=float, default=120, help="seconds per step")
    parser.add_argument("--filter", default="", help="run packets whose path contains this text")
    parser.add_argument("--keep", type=Path, help="build in this directory and keep the results")
    parser.add_argument("--cpuinfo", type=Path, default=Path("/proc/cpuinfo"),
                        help=argparse.SUPPRESS)
    parser.add_argument("--any-host", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    native = args.any_host or native_host()
    prefix = args.binutils_prefix
    if prefix is None:
        prefix = "" if platform.system() == "Linux" else "x86_64-elf-"
    tools = Tools(args.cc, args.qemu, prefix, native)
    root = args.dir.resolve()
    packets = [p for p in find_packets(root) if args.filter in str(p.relative_to(root))]
    if not packets:
        print(f"no packets under {root}", file=sys.stderr)
        return 1
    flags = host_flags(args.cpuinfo) if native else set()
    keep = args.keep.resolve() if args.keep else None
    with concurrent.futures.ThreadPoolExecutor(max(1, args.jobs)) as pool:
        results = list(pool.map(
            lambda packet: run_packet(packet, root, tools, flags, args.timeout, keep),
            packets))
    counts = collections.Counter(result.status for result in results)
    for result in results:
        if result.status == "fail":
            print(f"FAIL {result.packet}\n{result.detail.rstrip()}\n")
    skips = collections.Counter(r.detail for r in results if r.status == "skip")
    print(f"{counts['pass']} passed, {counts['fail']} failed, {counts['skip']} skipped "
          f"of {len(results)} packets")
    for reason, count in sorted(skips.items()):
        print(f"  skipped {count}: {reason}")
    return 1 if counts["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())

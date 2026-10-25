#!/usr/bin/env python3
"""Run an exported tail packet on an x86-64 Linux host with GCC and Clang."""
import argparse
import json
import pathlib
import platform
import subprocess
import tempfile


def run(command):
    result = subprocess.run(command, capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{command!r}: exit {result.returncode}\n{result.stdout}{result.stderr}")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packet", type=pathlib.Path)
    parser.add_argument("--compiler", action="append", dest="compilers")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("native execution requires x86-64 Linux")
    packet = args.packet.resolve()
    roster = json.loads((packet / "roster.json").read_text())
    names = roster["objects"]
    if len(names) != 36 or len(set(names)) != 36:
        parser.error("expected the complete 36-object packet")
    compilers = args.compilers or ["gcc", "clang"]
    count = 0
    rejected = 0
    with tempfile.TemporaryDirectory(prefix="keel-sysv-tail-native-") as work:
        work = pathlib.Path(work)
        for compiler in compilers:
            print(run([compiler, "--version"]).splitlines()[0], flush=True)
            for level, march in [("V1", "x86-64"), ("V2", "x86-64-v2"), ("V3", "x86-64-v3")]:
                primary = work / "primary.o"
                run([compiler, "-O2", f"-march={march}", "-c", str(packet / "oracle.c"), "-o", str(primary)])
                for name in names:
                    if not name.startswith(level + "-"):
                        continue
                    exe = work / "tail"
                    run([compiler, str(primary), str(packet / "depth.S"), str(packet / (name + ".o")), "-o", str(exe)])
                    output = run([str(exe)])
                    if output != "KEEL SYSV TAIL PASS\n":
                        raise RuntimeError(f"{compiler} {name}: unexpected output {output!r}")
                    count += 1
                    print(f"PASS {compiler} {name}", flush=True)
                if level == "V1":
                    # Replace one argument-permuting tail with a constant return.
                    # The independent value checks must reject it normally.
                    stub = work / "bad-tail.c"
                    stub.write_text("unsigned long long __wrap_tail_gp_0_0(void) { return 0; }\n")
                    exe = work / "bad-tail"
                    first = next(name for name in names if name.startswith("V1-"))
                    run([compiler, str(primary), str(packet / "depth.S"),
                         str(packet / (first + ".o")), str(stub),
                         "-Wl,--wrap=tail_gp_0_0", "-o", str(exe)])
                    result = subprocess.run([str(exe)], capture_output=True, text=True, timeout=60)
                    lines = result.stdout.splitlines()
                    if (result.returncode != 1 or result.stderr or len(lines) != 3
                            or not all(line.startswith("FAIL gp direct reverse: ") for line in lines)):
                        raise RuntimeError(f"{compiler}: negative control did not fail as expected: {result!r}")
                    rejected += 1
                    print(f"REJECT {compiler} constant-return tail control", flush=True)
    print(f"{count} native tail verdicts passed; {rejected} bad-tail controls rejected")


if __name__ == "__main__":
    main()

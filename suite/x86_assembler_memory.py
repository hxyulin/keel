#!/usr/bin/env python3
"""x86 assembler memory: baseline/candidate differential and locked RSS replay.

Example (macOS; both binaries built from private frozen trees):
  python3 suite/x86_assembler_memory.py --baseline B/keel --candidate C/keel \\
      --out target/x86-assembler-memory-20261019 --corpus-list extra.txt
Phases: corpus (generate inputs), diff (CLI exit/stderr/object equality),
memory (locked wait4 peak RSS, alternating binaries, identical-object check).
Each measured child holds an exclusive fcntl.flock on /tmp/keel-perf-20261019.lock;
Darwin ru_maxrss is bytes. A ps watchdog kills children above 1.8 GiB or 120 s.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import signal
import statistics
import subprocess
import sys
import threading
import time

M = 1024 * 1024
LOCK = "/tmp/keel-perf-20261019.lock"
TARGET = "x86_64-unknown-linux-gnu"
WORKLOADS = ["target/x86-speed/quickjs.s", "target/x86-speed/sqlite3.s",
             "target/assembler-investigation-20261019/inputs/x86-functions-keel-debug.s",
             "target/assembler-investigation-20261019/inputs/x86-functions-keel.s",
             "target/assembler-investigation-20261019/inputs/x86-mixed-500000.s"]


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(M), b""):
            h.update(b)
    return h.hexdigest()


def write_repeated(path, header, block, n):
    with open(path, "w") as f:
        f.write(header)
        for _ in range(n // 4096):
            f.write(block * 4096)
        f.write(block * (n % 4096))


def functions(path, size):
    """Compiler-shaped functions: CFI, line rows, frame traffic, local branches,
    calls and RIP-relative data. Deterministic for a given size."""
    r = random.Random(20261019)
    plain = ["mov rax, rsi", "mov qword ptr [rbp - 8], rax", "mov eax, dword ptr [rbp - 12]",
             "add rax, 1", "imul rcx, rdx", "xor edx, edx", "lea rcx, [rax + 8*rbx + 16]",
             "movsxd rax, ecx", "cmp rax, 7", "sete al", "movzx eax, al", "shl rax, 3",
             "sub rsp, 32", "add rsp, 32", "test rdi, rdi", "mov ecx, 1000000"]
    with open(path, "w") as f:
        f.write('.intel_syntax noprefix\n.file 1 "gen.c"\n')
        n = 0
        while f.tell() < size:
            name = f"fn{n}"
            f.write(f".text\n.globl {name}\n.p2align 4\n.type {name},@function\n{name}:\n"
                    ".cfi_startproc\npush rbp\n.cfi_def_cfa_offset 16\n.cfi_offset rbp, -16\n"
                    "mov rbp, rsp\n.cfi_def_cfa_register rbp\n")
            blocks = r.randint(2, 8)
            for b in range(blocks):
                f.write(f".L{name}_b{b}:\n.loc 1 {n % 60000 + 1} {b}\n")
                for _ in range(r.randint(3, 40)):
                    f.write(r.choice(plain) + "\n")
                k = r.random()
                target = f".L{name}_b{r.randrange(blocks)}"
                if k < .45:
                    f.write(f"jne {target}\n")
                elif k < .6:
                    f.write(f"call fn{r.randrange(n + 1)}@PLT\n")
                elif k < .75:
                    f.write(f"lea rsi, [rip + .Ldata{n}]\n")
                else:
                    f.write(f"jmp {target}\n")
            f.write(f"mov rsp, rbp\npop rbp\n.cfi_def_cfa rsp, 8\nret\n.cfi_endproc\n"
                    f".size {name}, .-{name}\n.section .rodata\n.Ldata{n}: .quad {name}\n")
            n += 1


def mutations(source_path, out_dir, count, seed):
    """Inject errors and relaxation-sensitive edits into real assembly. Some
    variants contain several injections to exercise diagnostic precedence."""
    lines = Path(source_path).read_text().splitlines(keepends=True)
    r = random.Random(seed)
    edits = [".bss\n", "mov al, 300\n", "andn eax, ebx, ecx\n", "mov eax, [\n",
             ".zero 200\n", ".p2align 6,0x90,5\n", "jmp .Lnowhere_undefined\n", "nop\nnop\n",
             ".section .tbss,\"awT\",@nobits\n", ".data\n"]
    paths = []
    for i in range(count):
        copy = list(lines)
        for _ in range(r.choice([1, 1, 2, 3])):
            at = r.randrange(len(copy))
            if r.random() < .15:
                del copy[at]
            else:
                copy.insert(at, r.choice(edits))
        p = out_dir / f"mut-{Path(source_path).stem}-{i:03d}.s"
        p.write_text("".join(copy))
        paths.append(p)
    return paths


def relaxation_programs(out_dir, count):
    """Small random programs mixing symbol-free runs, short/near branches,
    max-skip alignment and layout-dependent absolute operands."""
    r = random.Random(7)
    paths = []
    for i in range(count):
        out = [".text\n"]
        labels = [f"L{j}" for j in range(r.randint(2, 12))]
        for j, label in enumerate(labels):
            out.append(f"{label}:\n")
            for _ in range(r.randint(0, 6)):
                k = r.random()
                if k < .35:
                    out.append(f"{r.choice(['jmp', 'je', 'jne', 'jl'])} {r.choice(labels)}\n")
                elif k < .55:
                    out.append(("nop\n" if r.random() < .5 else "mov eax, 1\n") * r.randint(1, 60))
                elif k < .7:
                    out.append(f".p2align {r.randint(1, 7)},0x90,{r.randint(0, 40)}\n")
                elif k < .8:
                    out.append(f".zero {r.randint(1, 130)}\n")
                elif k < .9 and j:
                    out.append(f".set d{len(out)},{label}-{labels[0]}\npush d{len(out)}\n")
                else:
                    out.append("ret\n")
        p = out_dir / f"relax-{i:03d}.s"
        p.write_text("".join(out))
        paths.append(p)
    return paths


def corpus(a):
    c = a.out / "corpus"
    c.mkdir(parents=True, exist_ok=True)
    generated = [c / "x86-nops-8.s", c / "x86-nops-32.s", c / "x86-functions-8.s",
                 c / "x86-functions-32.s", c / "x86-relax-2.s"]
    write_repeated(generated[0], ".text\n", "nop\n", 8 * M // 4)
    write_repeated(generated[1], ".text\n", "nop\n", 32 * M // 4)
    functions(generated[2], 8 * M)
    functions(generated[3], 32 * M)
    # Forward branches each forced to rel32 across 160 bytes of data.
    with open(generated[4], "w") as f:
        f.write(".text\n")
        for i in range(2 * M // 32):
            f.write(f"jmp .Lr{i}\n.zero 160\n.Lr{i}:\n")
    small = relaxation_programs(c, 300)
    for i, w in enumerate(WORKLOADS[:3]):
        small += mutations(w, c, 40, i)
    listing = [str(p) for p in generated + small] + WORKLOADS
    if a.corpus_list:
        listing += [line.strip() for line in a.corpus_list.read_text().splitlines() if line.strip()]
    (a.out / "corpus.txt").write_text("\n".join(str(Path(p).resolve()) for p in listing) + "\n")
    print("corpus", len(listing), "files", flush=True)


def diff(a):
    files = (a.out / "corpus.txt").read_text().split()
    work = a.out / "diff"
    work.mkdir(exist_ok=True)
    results, mismatches = [], []
    with open(LOCK, "a+") as guard:
        fcntl.flock(guard, fcntl.LOCK_EX)
        for n, path in enumerate(files):
            outcome = {}
            for name, binary in [("baseline", a.baseline), ("candidate", a.candidate)]:
                obj = work / f"{n:05d}.{name}.o"
                obj.unlink(missing_ok=True)
                p = subprocess.run([binary, "asm", "--target", TARGET, path, "-o", obj],
                                   capture_output=True, timeout=120)
                outcome[name] = (p.returncode, p.stderr.decode(errors="replace"),
                                 digest(obj) if p.returncode == 0 else None)
                obj.unlink(missing_ok=True)
            same = outcome["baseline"] == outcome["candidate"]
            results.append(dict(path=path, exit=outcome["baseline"][0], same=same,
                                stderr=outcome["baseline"][1][:300], object_sha256=outcome["baseline"][2]))
            if not same:
                mismatches.append(dict(path=path, **outcome))
    summary = dict(files=len(results), identical=sum(r["same"] for r in results),
                   succeeded=sum(r["exit"] == 0 for r in results),
                   rejected=sum(r["exit"] != 0 for r in results), mismatches=mismatches)
    (a.out / "diff.json").write_text(json.dumps(dict(summary=summary, results=results), indent=1) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "mismatches"}), len(mismatches), "mismatches")
    return not mismatches


def run_child(cmd):
    """Run one direct child under the lock; return (exit, seconds, wait4 rss, sampled rss, stopped)."""
    with open(LOCK, "a+") as guard:
        fcntl.flock(guard, fcntl.LOCK_EX)
        load = os.getloadavg()
        start = time.monotonic()
        p = subprocess.Popen([str(x) for x in cmd], stdout=subprocess.DEVNULL,
                             stderr=subprocess.PIPE, start_new_session=True)
        reason, sampled, done = [None], [0], threading.Event()

        def watchdog():
            while not done.wait(.05):
                if time.monotonic() - start > 120:
                    reason[0] = "timeout"
                try:
                    rss = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(p.pid)],
                                                      stderr=subprocess.DEVNULL).strip() or 0) * 1024
                    sampled[0] = max(sampled[0], rss)
                    if rss > 1.8 * 1024**3:
                        reason[0] = "RSS watchdog"
                except (subprocess.CalledProcessError, ValueError):
                    pass
                if reason[0]:
                    try:
                        os.killpg(p.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    return
        w = threading.Thread(target=watchdog, daemon=True)
        w.start()
        _, status, usage = os.wait4(p.pid, 0)
        elapsed = time.monotonic() - start
        done.set()
        w.join()
        stderr = p.stderr.read().decode(errors="replace")
        return dict(exit=os.waitstatus_to_exitcode(status), seconds=elapsed, user=usage.ru_utime,
                    peak_rss_bytes=usage.ru_maxrss, sampled_rss_bytes=sampled[0], stopped=reason[0],
                    stderr=stderr[:500], load_start=load, load_end=os.getloadavg())


def memory(a):
    c = a.out / "corpus"
    inputs = [c / "x86-nops-8.s", c / "x86-nops-32.s", c / "x86-functions-8.s",
              c / "x86-functions-32.s", c / "x86-relax-2.s"] + [Path(w) for w in WORKLOADS]
    meta = dict(platform=platform.platform(), python=sys.version, lock=LOCK, repeats=a.repeats,
                rss="os.wait4 per direct child; Darwin ru_maxrss bytes; ps watchdog KiB",
                binaries={n: dict(path=str(b), sha256=digest(b))
                          for n, b in [("baseline", a.baseline), ("candidate", a.candidate)]})
    for key, cmd in [("rustc", ["rustc", "--version"]), ("model", ["sysctl", "-n", "hw.model"]),
                     ("memory", ["sysctl", "-n", "hw.memsize"])]:
        meta[key] = subprocess.check_output(cmd, text=True).strip()
    records, rows = [], []
    for path in inputs:
        objects = {}
        for repeat in range(a.repeats):
            order = [("baseline", a.baseline), ("candidate", a.candidate)]
            if repeat % 2:
                order.reverse()
            for name, binary in order:
                obj = a.out / "scratch" / f"mem-{path.stem}.{name}.o"
                obj.parent.mkdir(exist_ok=True)
                r = run_child([binary, "asm", "--target", TARGET, path, "-o", obj])
                assert r["exit"] == 0 and not r["stopped"], (path, name, r)
                sha = digest(obj)
                assert objects.setdefault(name, sha) == sha, (path, name, "nondeterministic")
                r.update(name=name, input=str(path), input_bytes=path.stat().st_size, repeat=repeat,
                         object_bytes=obj.stat().st_size, object_sha256=sha, input_sha256=digest(path))
                records.append(r)
                print(path.name, name, repeat, round(r["peak_rss_bytes"] / M, 1), "MiB",
                      round(r["seconds"], 3), "s", flush=True)
        assert objects["baseline"] == objects["candidate"], (path, "objects differ")
        med = {n: dict(rss=statistics.median(r["peak_rss_bytes"] for r in records
                                             if r["input"] == str(path) and r["name"] == n),
                       seconds=statistics.median(r["seconds"] for r in records
                                                 if r["input"] == str(path) and r["name"] == n))
               for n in ("baseline", "candidate")}
        rows.append(dict(input=path.name, input_bytes=path.stat().st_size,
                         object_bytes=records[-1]["object_bytes"], object_sha256=objects["candidate"],
                         **{f"{n}_{k}": v for n in med for k, v in med[n].items()}))
    (a.out / "memory.json").write_text(json.dumps(dict(metadata=meta, summary=rows, records=records),
                                                  indent=1) + "\n")
    for row in rows:
        print(f"{row['input']:32} {row['input_bytes'] / M:7.2f} MiB in  "
              f"{row['baseline_rss'] / M:8.1f} -> {row['candidate_rss'] / M:7.1f} MiB RSS  "
              f"{row['baseline_seconds']:.3f} -> {row['candidate_seconds']:.3f} s")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--baseline", type=Path, required=True)
    p.add_argument("--candidate", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--corpus-list", type=Path, help="extra assembly paths, one per line")
    p.add_argument("--phase", choices=["corpus", "diff", "memory", "all"], default="all")
    p.add_argument("--repeats", type=int, default=3)
    a = p.parse_args()
    assert platform.system() == "Darwin", "ru_maxrss units are macOS-specific here"
    a.baseline, a.candidate, a.out = a.baseline.resolve(), a.candidate.resolve(), a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=True)
    if a.phase in ("corpus", "all"):
        corpus(a)
    if a.phase in ("diff", "all") and not diff(a):
        sys.exit("baseline/candidate differential mismatch")
    if a.phase in ("memory", "all"):
        memory(a)


if __name__ == "__main__":
    main()

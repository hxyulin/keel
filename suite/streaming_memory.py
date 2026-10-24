#!/usr/bin/env python3
"""Bounded assembler/linker memory experiment; no compiler production changes.

Example (macOS; frozen binary built outside target/release):
  python3 suite/streaming_memory.py --keel target/streaming-investigation-20261019/build/release/keel --out target/streaming-investigation-20261019
Each direct executable child has its own wait4 rusage (Darwin ru_maxrss is bytes).
The watchdog kills a child above 1.8 GiB sampled RSS or after 120 seconds.
Generated binaries are inspected, hashed, and repeated at the identical path.
"""
import argparse
import fcntl
import hashlib
import json
import mmap
import os
from pathlib import Path
import platform
import signal
import shutil
import struct
import subprocess
import sys
import time
import threading

M = 1024 * 1024
A64 = "aarch64-apple-darwin"
X86 = "x86_64-unknown-linux-gnu"
START = {A64: ".text\n.globl _main\n_main:\nmov w0, #0\nret\n",
         X86: ".text\n.globl _start\n_start:\nmov eax, 60\nxor edi, edi\nsyscall\n"}


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(M), b""):
            h.update(b)
    return h.hexdigest()


def inspect(path):
    """Bounds-check object/load structures without loading the whole output."""
    with open(path, "rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as b:
        sections, segments = [], []
        nsyms = 0
        if b[:4] == b"\xcf\xfa\xed\xfe":
            kind, ncmd, cmdbytes = struct.unpack_from("<III", b, 12)
            assert 32 + cmdbytes <= len(b)
            at = 32
            entry = None
            for _ in range(ncmd):
                cmd, size = struct.unpack_from("<II", b, at)
                assert size >= 8 and at + size <= 32 + cmdbytes
                if cmd == 0x19:
                    name = b[at+8:at+24].split(b"\0")[0].decode()
                    vm, vmsize, off, filesz = struct.unpack_from("<QQQQ", b, at+24)
                    assert off + filesz <= len(b)
                    segments.append(dict(name=name, vm=vm, memory_bytes=vmsize, file_bytes=filesz))
                    ns = struct.unpack_from("<I", b, at+64)[0]
                    assert 72 + ns * 80 == size
                    for i in range(ns):
                        s = at + 72 + 80*i
                        sn = b[s:s+16].split(b"\0")[0].decode()
                        sg = b[s+16:s+32].split(b"\0")[0].decode()
                        address, length = struct.unpack_from("<QQ", b, s+32)
                        offset, alignment, reloff, relcount, flags = struct.unpack_from("<IIIII", b, s+48)
                        zero = flags & 255 in (1, 0x12)
                        assert zero or offset + length <= len(b)
                        assert reloff + 8*relcount <= len(b)
                        sections.append(dict(name=sn, segment=sg, size=length, zero=zero,
                                             relocations=relcount, offset=offset, address=address, flags=flags))
                elif cmd == 2:
                    so, nsyms, st, sl = struct.unpack_from("<IIII", b, at+8)
                    assert so + 16*nsyms <= len(b) and st + sl <= len(b)
                elif cmd == 0x80000028:
                    entry = struct.unpack_from("<Q", b, at+8)[0]
                at += size
            assert at == 32 + cmdbytes
            if kind == 2:
                assert entry is not None
                text = next(s for s in sections if s["name"] == "__text")
                assert text["offset"] <= entry < text["offset"] + text["size"]
            fmt = "Mach-O64"
        elif b[:4] == b"\x7fELF":
            assert b[4:6] == b"\x02\x01"
            kind = struct.unpack_from("<H", b, 16)[0]
            entry, phoff, shoff = struct.unpack_from("<QQQ", b, 24)
            phsize, phnum, shsize, shnum, shstr = struct.unpack_from("<HHHHH", b, 54)
            assert phoff + phsize*phnum <= len(b)
            assert shoff + shsize*shnum <= len(b)
            names = b""
            if shnum:
                stroff, strlen = struct.unpack_from("<QQ", b, shoff+shsize*shstr+24)
                names = b[stroff:stroff+strlen]
            for i in range(shnum):
                s = shoff + shsize*i
                name, typ, flags, address, offset, length = struct.unpack_from("<IIQQQQ", b, s)
                assert typ == 8 or offset + length <= len(b)
                sn = names[name:].split(b"\0")[0].decode()
                entsize = struct.unpack_from("<Q", b, s+56)[0]
                if typ == 2:
                    nsyms += length // entsize
                sections.append(dict(name=sn, size=length, zero=typ == 8,
                                     offset=offset, address=address, flags=flags,
                                     relocations=length//entsize if typ == 4 else 0))
            for i in range(phnum):
                p = phoff+phsize*i
                typ, flags, offset, vm, _, filesz, memsz = struct.unpack_from("<IIQQQQQ", b, p)
                assert offset + filesz <= len(b) and filesz <= memsz
                if typ == 1:
                    segments.append(dict(vm=vm, memory_bytes=memsz, file_bytes=filesz, flags=flags))
            if kind == 2:
                assert any(s["flags"] & 1 and s["vm"] <= entry < s["vm"]+s["memory_bytes"] for s in segments)
            fmt = "ELF64"
        else:
            raise AssertionError("unrecognized output " + str(path))
        return dict(format=fmt, kind=kind, sections=sections, segments=segments,
                    symbols=nsyms, relocations=sum(s["relocations"] for s in sections))


def section_hash(path, section):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        f.seek(section["offset"])
        remaining = section["size"]
        while remaining:
            chunk = f.read(min(M, remaining))
            assert chunk
            h.update(chunk)
            remaining -= len(chunk)
    return h.hexdigest()


def pattern_hash(prefix, block, count):
    h = hashlib.sha256(prefix)
    for _ in range(count//8192):
        h.update(block*8192)
    h.update(block*(count%8192))
    return h.hexdigest()


def validate(out):
    raw = json.loads((out/"raw.json").read_text())
    checks = []
    unique = {r["command"][-1]: r for r in raw["records"] if r["exit"] == 0}
    for path, r in unique.items():
        path = Path(path)
        structure = inspect(path)
        assert digest(path) == r["output_sha256"]
        name, group = r["name"], r["group"]
        sections = structure["sections"]
        def section(sn):
            return next(s for s in sections if s["name"] == sn)
        if group in ("a64-instructions", "a64-comments", "a64-symbols-relocations"):
            n = r["scale"]//4 if group == "a64-instructions" else 0
            assert section_hash(path, section("__text")) == pattern_hash(bytes.fromhex("00008052c0035fd6"), bytes.fromhex("1f2003d5"), n)
        if group == "a64-symbols-relocations":
            assert structure["relocations"] == r["scale"] and structure["symbols"] == r["scale"]+1
        if group == "macho-symbols-relocations":
            address = section("__text")["address"]
            assert section_hash(path, section("__data")) == pattern_hash(b"", struct.pack("<Q", address), r["scale"])
        if group == "x86-instructions":
            assert section_hash(path, section(".text")) == pattern_hash(bytes.fromhex("b83c00000031ff0f05"), b"\x90", r["scale"]//4)
        if group == "x86-relax":
            assert structure["relocations"] == 0
            assert section_hash(path, section(".text")) == pattern_hash(bytes.fromhex("b83c00000031ff0f05"), bytes.fromhex("e9a0000000")+b"\0"*160+b"\x90", r["scale"])
        executable = structure["kind"] == 2
        if executable and structure["format"] == "Mach-O64":
            assert not any(s.get("segment") == "__DWARF" for s in sections)
            subprocess.run(["codesign", "--verify", "--strict", path], check=True, capture_output=True, timeout=10)
            # Execute only AArch64 binaries; the ELF outputs are cross linked.
            subprocess.run([path], check=True, capture_output=True, timeout=10)
        checks.append(dict(path=str(path), sha256=r["output_sha256"], bounds_checked=True,
                           native_codesign_and_execution=executable and structure["format"] == "Mach-O64"))
    plain, debug = out/"macho-link-nodebug-64.exe", out/"macho-link-debug-64.exe"
    assert digest(plain) == digest(debug), "debug-str bearing output differs from no-debug control"
    prelock = out/"raw-prelock.json"
    comparisons = 0
    if prelock.exists():
        old = {r["command"][-1]: r["output_sha256"] for r in json.loads(prelock.read_text())["records"] if r["exit"] == 0}
        for path, r in unique.items():
            if path in old:
                assert old[path] == r["output_sha256"]
                comparisons += 1
    (out/"validation.json").write_text(json.dumps(dict(outputs=checks, prelock_hash_matches=comparisons,
        debug_and_nodebug_executable_identical=True), indent=2)+"\n")
    print("Validated", len(checks), "unique outputs;", comparisons, "match prelock hashes", flush=True)


class Experiment:
    def __init__(self, args):
        self.a = args
        self.out = args.out.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.records = []
        self.meta = dict(platform=platform.platform(), host=platform.node(), python=sys.version,
                         binary=str(args.keel), binary_sha256=digest(args.keel),
                         max_rss_bytes=int(1.8*1024**3), timeout_seconds=120,
                         rss_method="os.wait4 per direct executable child; Darwin ru_maxrss bytes; ps watchdog KiB",
                         invocation=sys.argv, source_commit=args.source_commit,
                         advisory_lock="/tmp/keel-perf-20261019.lock")
        for key, cmd in [("rustc", ["rustc", "--version"]), ("memory", ["sysctl", "-n", "hw.memsize"]),
                         ("model", ["sysctl", "-n", "hw.model"]), ("clang", ["clang", "--version"])]:
            self.meta[key] = subprocess.check_output(cmd, text=True).strip()
        if args.phase in ("supplement", "workloads"):
            previous = json.loads((self.out / "raw.json").read_text())
            assert previous["metadata"]["binary_sha256"] == self.meta["binary_sha256"]
            self.meta = previous["metadata"]
            self.records = previous["records"]

    def save(self):
        (self.out / "raw.json").write_text(json.dumps(dict(metadata=self.meta, records=self.records), indent=2)+"\n")

    def measure(self, name, cmd, inputs, output, group, scale, expected=None, repeats=1, reject=None):
        hashes = []
        for repeat in range(repeats):
            guard = open("/tmp/keel-perf-20261019.lock", "a+")
            wait_start = time.monotonic()
            fcntl.flock(guard, fcntl.LOCK_EX)
            lock_wait = time.monotonic() - wait_start
            load_start = os.getloadavg()
            log = self.out / (name+f"-{repeat}.stderr")
            outlog = self.out / (name+f"-{repeat}.stdout")
            with log.open("wb") as err, outlog.open("wb") as stdout:
                start = time.monotonic()
                p = subprocess.Popen([str(x) for x in cmd], stdout=stdout, stderr=err, start_new_session=True)
                reason = [None]
                sampled = [0]
                done = threading.Event()
                def watchdog():
                    while not done.wait(.05):
                        if time.monotonic() - start > 120:
                            reason[0] = "timeout"
                        try:
                            rss = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(p.pid)], stderr=subprocess.DEVNULL).strip() or 0)*1024
                            sampled[0] = max(sampled[0], rss)
                            if rss > 1.8*1024**3:
                                reason[0] = "RSS watchdog"
                        except (subprocess.CalledProcessError, ValueError):
                            pass
                        if reason[0]:
                            try:
                                os.killpg(p.pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                            return
                watcher = threading.Thread(target=watchdog, daemon=True)
                watcher.start()
                _, status, usage = os.wait4(p.pid, 0)
                elapsed = time.monotonic() - start
                done.set()
                watcher.join()
                reason, sampled = reason[0], sampled[0]
                p.returncode = os.waitstatus_to_exitcode(status)
            load_end = os.getloadavg()
            fcntl.flock(guard, fcntl.LOCK_UN)
            guard.close()
            r = dict(name=name, group=group, scale=scale, repeat=repeat, command=[str(x) for x in cmd],
                     input_bytes=sum(Path(x).stat().st_size for x in inputs), inputs=[str(x) for x in inputs],
                     input_sha256=[digest(x) for x in dict.fromkeys(inputs)],
                     elapsed_seconds=elapsed, user_seconds=usage.ru_utime, system_seconds=usage.ru_stime,
                     peak_rss_bytes=usage.ru_maxrss, sampled_rss_bytes=sampled,
                     exit=p.returncode, stopped=reason, stderr=str(log),
                     advisory_lock="/tmp/keel-perf-20261019.lock", lock_wait_seconds=lock_wait,
                     load_start=load_start, load_end=load_end)
            if not reason and p.returncode == 0 and not reject:
                structure = inspect(output)
                r.update(output_bytes=output.stat().st_size, output_sha256=digest(output), structure=structure)
                if expected:
                    for section, size in expected.items():
                        assert sum(s["size"] for s in structure["sections"] if s["name"] == section) == size, (name, section, size, structure)
                hashes.append(r["output_sha256"])
                assert len(set(hashes)) == 1, (name, "nondeterminism", hashes)
                r["deterministic_so_far"] = True
            elif reject:
                assert not reason and p.returncode != 0 and reject in log.read_text(), (name, r, log.read_text())
                r["expected_rejection"] = reject
            self.records.append(r)
            self.save()
            print(name, repeat, round(elapsed, 3), round(usage.ru_maxrss/M, 1), p.returncode, reason, flush=True)
            if reason or p.returncode and not reject:
                return False
        return True

    def asm(self, name, target, source, group, scale, expected=None, repeats=2):
        src, obj = self.out/(name+".s"), self.out/(name+".o")
        if source is not None:
            src.write_text(source)
        ok = self.measure(name, [self.a.keel, "asm", "--target", target, src, "-o", obj],
                          [src], obj, group, scale, expected, repeats)
        if not ok:
            raise RuntimeError("bounded assembly failed: " + name)
        return obj

    def link(self, name, target, objects, group, scale, expected=None, repeats=2, reject=None):
        output = self.out / (name+".exe")
        flags = ["--sysroot", self.a.sdk] if target == A64 else []
        return self.measure(name, [self.a.keel, "link", "--target", target, *flags, *objects, "-o", output],
                            objects, output, group, scale, expected, repeats, reject)

    def repeated_source(self, name, header, block, n):
        with (self.out/(name+".s")).open("w") as f:
            f.write(header)
            chunk = block*8192
            for _ in range(n//8192):
                f.write(chunk)
            f.write(block*(n%8192))

    def run(self):
        # Dense source and comments have identical source scale, different output scale.
        for mib in [8, 32, 128]:
            for family, block in [("a64-instructions", "nop\n"), ("a64-comments", "//"+"c"*125+"\n")]:
                name = family+f"-{mib}"
                n = mib*M//len(block)
                self.repeated_source(name, START[A64], block, n)
                self.asm(name, A64, None, family, mib*M, {"__text": 8+(4*n if family.endswith("instructions") else 0)})
        for mib in [1, 4, 16]:
            name = f"a64-symbols-{mib}"
            n = mib*M//24
            with (self.out/(name+".s")).open("w") as f:
                f.write(START[A64]+".data\n")
                for i in range(n):
                    f.write(f"_s{i:07d}:\n.quad _main\n")
            obj = self.asm(name, A64, None, "a64-symbols-relocations", n, {"__data": n*8})
            self.link(name+"-link", A64, [obj], "macho-symbols-relocations", n, {"__data": n*8})
        for mib in [1, 2, 4, 8]:
            name = f"x86-instructions-{mib}"
            n = mib*M//4
            self.repeated_source(name, START[X86], "nop\n", n)
            self.asm(name, X86, None, "x86-instructions", mib*M, {".text": 9+n})
        for kib in [512, 2048, 8192]:
            name = f"x86-relax-{kib}"
            n = kib*1024//42
            with (self.out/(name+".s")).open("w") as f:
                f.write(START[X86])
                for i in range(n):
                    # Forward target requires widening; every target is within its block.
                    f.write(f"jmp .Lf{i}\n.zero 160\n.Lf{i}:\nnop\n")
            self.asm(name, X86, None, "x86-relax", n, {".text": 9+166*n})
        for target, prefix, ds, bs in [(A64, "macho", "__data", "__bss"), (X86, "elf", ".data", ".bss")]:
            start = self.asm(prefix+"-start", target, START[target], "setup", 0)
            for mib in [8, 32, 128]:
                for count, label in [(1, "few"), (32, "many")]:
                    each = mib*M//count
                    payload = self.asm(f"{prefix}-payload-{label}-{mib}", target, f".data\n.zero {each}\n", "asm-file-backed-"+prefix, each, {ds: each})
                    self.link(f"{prefix}-link-{label}-{mib}", target, [start]+[payload]*count,
                              prefix+"-link-"+label, mib*M, {ds: mib*M})
                text = f".zerofill __DATA,__bss,_storage,{mib*M},3\n" if target == A64 else f".bss\n.zero {mib*M}\n"
                bss = self.asm(f"{prefix}-bss-{mib}", target, text, "asm-bss-"+prefix, mib*M, {bs: mib*M})
                self.link(f"{prefix}-link-bss-{mib}", target, [start, bss], prefix+"-link-bss", mib*M, {bs: mib*M})
            tiny = self.asm(prefix+"-tiny", target, ".data\n.zero 4096\n", "setup", 0)
            for count in [32, 128, 512]:
                self.link(f"{prefix}-object-count-{count}", target, [start]+[tiny]*count,
                          prefix+"-object-count", count, {ds: count*4096})
            huge = 512*M+1
            text = f".zerofill __DATA,__bss,_too_big,{huge},0\n" if target == A64 else f".bss\n.zero {huge}\n"
            bss = self.asm(prefix+"-over-limit", target, text, "probe-setup", huge, {bs: huge})
            self.link(prefix+"-reject-section", target, [start, bss], "rejection", huge,
                      reject="512 MiB", repeats=1)
            size = 300*M
            text = f".zerofill __DATA,__bss,_block,{size},0\n" if target == A64 else f".bss\n.zero {size}\n"
            bss = self.asm(prefix+"-merge-limit", target, text, "probe-setup", size, {bs: size})
            # Strip the global name to allow repeated BSS objects without duplicate definitions.
            if target == A64:
                bss = self.asm(prefix+"-merge-local", target, f".zerofill __DATA,__bss,Lblock,{size},0\n", "probe-setup", size, {bs: size})
            self.link(prefix+"-reject-merged", target, [start, bss, bss], "rejection", 2*size,
                      reject="512 MiB", repeats=1)
        for mib in [8, 32, 64]:
            obj = self.asm(f"macho-debug-{mib}", A64,
                           START[A64]+f".data\n.zero {mib*M}\n.section __DWARF,__debug_str,regular,debug\n.zero {mib*M}\n",
                           "asm-debug-bearing", mib*M, {"__data": mib*M, "__debug_str": mib*M})
            self.link(f"macho-link-debug-{mib}", A64, [obj], "macho-link-debug-no-map", mib*M, {"__data": mib*M})
        self.save()

    def supplement(self):
        for mib in [16, 32]:
            name = f"x86-instructions-{mib}"
            n = mib*M//4
            self.repeated_source(name, START[X86], "nop\n", n)
            self.asm(name, X86, None, "x86-instructions", mib*M, {".text": 9+n}, repeats=3)
        self.asm("macho-nodebug-64", A64, START[A64]+f".data\n.zero {64*M}\n",
                 "asm-no-debug-control", 64*M, {"__data": 64*M})
        self.link("macho-link-nodebug-64", A64, [self.out/"macho-nodebug-64.o"],
                  "macho-link-no-debug-control", 64*M, {"__data": 64*M}, repeats=3)
        # Additional repeats at identical output paths. Preserve the initial logs.
        names = ["a64-instructions-128", "a64-comments-128", "a64-symbols-16", "x86-relax-8192",
                 "macho-link-few-128", "macho-link-many-128", "elf-link-few-128", "macho-link-debug-64"]
        for name in names:
            r = next(r for r in self.records if r["name"] == name)
            output = Path(r["command"][-1])
            self.measure(name+"-extra", r["command"], r["inputs"], output, r["group"], r["scale"], repeats=3)
            assert digest(output) == r["output_sha256"]
        # Compare independent wait4 accounting with the macOS time utility.
        r = next(r for r in self.records if r["name"] == "macho-link-few-128")
        with open("/tmp/keel-perf-20261019.lock", "a+") as guard, (self.out/"time-l-calibration.txt").open("w") as log:
            fcntl.flock(guard, fcntl.LOCK_EX)
            subprocess.run(["/usr/bin/time", "-l", *r["command"]], stdout=log, stderr=log, check=True, timeout=120)
        assert digest(Path(r["command"][-1])) == r["output_sha256"]
        self.save()

    def workloads(self):
        for name, target in [("quickjs-quickjs-keel-O", A64), ("sqlite-sqlite3-keel-O", A64),
                             ("sqlite-sqlite3-clang-O2", A64), ("x86-functions-keel-debug", X86)]:
            run_name = "workload-"+name+("-apple-m5" if "clang" in name else "")
            if any(r["name"] == run_name and r["exit"] == 0 for r in self.records):
                continue
            source = self.a.corpus/(name+".s")
            frozen = self.out/("workload-"+name+".s")
            shutil.copyfile(source, frozen)
            obj = frozen.with_suffix(".o")
            flags = ["--cpu", "apple-m5"] if "clang" in name else []
            self.measure(run_name, [self.a.keel, "asm", "--target", target, *flags, frozen, "-o", obj],
                         [frozen], obj, "cached-workload-asm", source.stat().st_size, repeats=3)
        cached = Path("target/app-loop-profile/macos-arm64/sqlite")
        if cached.is_dir() and not any(r["name"] == "workload-sqlite-link" for r in self.records):
            objects = []
            for name in ["shell.o", "sqlite3.o"]:
                obj = self.out/("workload-link-"+name)
                shutil.copyfile(cached/name, obj)
                objects.append(obj)
            output = self.out/"workload-sqlite.exe"
            self.measure("workload-sqlite-link", [self.a.keel, "link", "--target", A64,
                         "--sysroot", self.a.sdk, *objects, "-lm", "-lpthread", "-o", output],
                         objects, output, "cached-workload-link", sum(p.stat().st_size for p in objects), repeats=3)
            result = subprocess.run([output, ":memory:", "select 6*7;"], check=True, capture_output=True, timeout=10)
            assert result.stdout == b"42\n"
            (self.out/"workload-sqlite-query.stdout").write_bytes(result.stdout)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--keel", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--source-commit", default="a24ae66e817137b4872e67279161a54f7e84d352")
    p.add_argument("--sdk", default="/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk")
    p.add_argument("--phase", choices=["primary", "supplement", "workloads", "validate", "all"], default="all")
    p.add_argument("--corpus", type=Path, default=Path("target/assembler-investigation-20261019/inputs"))
    a = p.parse_args()
    a.keel = a.keel.resolve()
    assert platform.system() == "Darwin", "rusage units and tool assumptions are macOS-specific"
    if a.phase == "validate":
        with open("/tmp/keel-perf-20261019.lock", "a+") as guard:
            fcntl.flock(guard, fcntl.LOCK_EX)
            validate(a.out.resolve())
        return
    e = Experiment(a)
    if a.phase == "workloads":
        e.workloads()
        return
    if a.phase != "supplement":
        e.run()
    if a.phase != "primary":
        e.supplement()
    if a.phase == "all":
        with open("/tmp/keel-perf-20261019.lock", "a+") as guard:
            fcntl.flock(guard, fcntl.LOCK_EX)
            validate(a.out.resolve())


if __name__ == "__main__":
    main()

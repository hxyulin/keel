#!/usr/bin/env python3
"""Compare Keel's static ELF linker with GNU ld on identical x86-64 objects."""
import argparse
import json
from pathlib import Path
import platform
import subprocess


def run(args, **kwargs):
    try:
        return subprocess.run([str(a) for a in args], check=True, capture_output=True, timeout=30, **kwargs)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"{error.cmd}: {error.stderr!r}") from error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("run this validator on x86-64 Linux")
    keel = args.keel.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    startup = out / "start.s"
    startup.write_text('''
.text
.globl _start
_start:
    mov (%rsp), %rdi
    lea 8(%rsp), %rsi
    call answer
    mov %eax, %edi
    mov $60, %eax
    syscall
.section .note.GNU-stack,"",@progbits
''')
    run(["clang", "-c", startup, "-o", out / "start.o"])
    source = out / "answer.c"
    source.write_text('''
int zero[256];
int initialized = 17;
int *pointer = &initialized;
#ifndef __KEEL__
extern int absent __attribute__((weak));
#endif
static int local = 25;
int answer(long argc, char **argv) {
    if (argc != 3 || !argv[1] || argv[2][0] != 'b') return 1;
#ifndef __KEEL__
    if (&absent != 0) return 2;
#endif
    if (zero[255] != 0) return 3;
    zero[255] = *pointer + local;
    return zero[255];
}
''')
    records = []

    def compare(name, objects):
        native = out / (name + ".keel")
        reference = out / (name + ".gnu")
        run([keel, "link", *objects, "-o", native])
        run(["ld", "-static", "-o", reference, *objects])
        for exe in [native, reference]:
            process = subprocess.run([str(exe), "a", "b"], capture_output=True, timeout=10)
            if (process.returncode, process.stdout, process.stderr) != (42, b"", b""):
                raise AssertionError((str(exe), process.returncode, process.stdout, process.stderr))
        headers = run(["readelf", "-lW", native], text=True).stdout
        assert "INTERP" not in headers and " RWE " not in headers
        assert "GNU_STACK" in headers
        run(["readelf", "-hSWs", native])
        (out / (name + ".program-headers.txt")).write_text(headers)
        records.append({"name": name, "exit": 42, "stdout_matches_gnu": True,
                        "native_bytes": native.stat().st_size,
                        "reference_bytes": reference.stat().st_size})

    for optimization in ["-O0", "-O2"]:
        for pic in ["-fno-pic", "-fPIC"]:
            obj = out / (optimization[1:] + pic + ".o")
            run(["clang", "-ffreestanding", optimization, pic, "-c", source, "-o", obj])
            compare("clang" + optimization + pic, [out / "start.o", obj])
    for optimization in [[], ["-O"]]:
        obj = out / ("keel" + ("-O" if optimization else "-plain") + ".o")
        run([keel, "cc", "--target", "x86_64-unknown-linux-gnu", *optimization, "--emit-object", source, "-o", obj])
        compare(obj.stem, [out / "start.o", obj])

    # Tentative globals, strong-over-weak precedence, and object-local names.
    definitions = {
        "probe.c": '''extern int shared[]; extern int weak_value(void);
int local_a(void); int local_b(void);
int answer(long n, char **v) {
    if (shared[15] || local_a()!=17 || local_b()!=25) return 1;
    shared[15] = weak_value(); return shared[15];
}''',
        "weak.c": '''int shared[4]; static int local=17;
int local_a(void) {return local;}
__attribute__((weak)) int weak_value(void) {return 3;}''',
        "strong.c": '''int shared[16]; static int local=25;
int local_b(void) {return local;}
int weak_value(void) {return 42;}''',
    }
    for name, text in definitions.items():
        path = out / name
        path.write_text(text)
        run(["clang", "-ffreestanding", "-fcommon", "-c", path, "-o", path.with_suffix(".o")])
    for order in [["weak.o", "strong.o"], ["strong.o", "weak.o"]]:
        compare("resolution-" + order[0], [out / "start.o", out / "probe.o", *[out / p for p in order]])

    # Keel's own assembler supplies startup, PC-relative data and a syscall.
    asm = out / "native.s"
    asm.write_text('''.text
.globl _start
_start:
mov eax, 1
mov edi, 1
lea rsi, [rip+message]
mov edx, 6
syscall
mov eax, 60
mov edi, 42
syscall
.section .rodata
message:
.asciz "hello"
''')
    obj = out / "native.o"
    run([keel, "asm", "--target", "x86_64-unknown-linux-gnu", asm, "-o", obj])
    native = out / "native-asm"
    reference = out / "native-asm.gnu"
    run([keel, "link", "--target", "x86_64-unknown-linux-gnu", obj, "-o", native])
    run(["ld", "-static", "-o", reference, obj])
    for exe in [native, reference]:
        process = subprocess.run([str(exe)], capture_output=True, timeout=10)
        assert (process.returncode, process.stdout, process.stderr) == (42, b"hello\0", b"")
    records.append({"name": "native-assembler-syscalls", "exit": 42, "stdout_matches_gnu": True})

    # CLI diagnostics must preserve an existing destination on failed links.
    sentinel = out / "sentinel"
    sentinel.write_bytes(b"unchanged")
    bad = subprocess.run([str(keel), "link", str(out / "start.o"), "-o", str(sentinel)], capture_output=True, timeout=10)
    assert bad.returncode != 0 and b"undefined symbol" in bad.stderr
    assert sentinel.read_bytes() == b"unchanged"
    report = {"host": platform.node(), "platform": platform.platform(),
              "clang": run(["clang", "--version"], text=True).stdout.splitlines()[0],
              "ld": run(["ld", "--version"], text=True).stdout.splitlines()[0],
              "cases": records, "failure_preserves_output": True}
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

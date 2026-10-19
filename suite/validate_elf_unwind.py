#!/usr/bin/env python3
"""Exercise native ELF unwind discovery across independently retained CFI sections.

Run on x86-64 Linux under the caller's build/workload lock. Requires existing
clang, ld, readelf, libc and libgcc_s; never downloads tools or uses a fallback.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--libc", type=Path, default=Path("/usr/lib/libc.so.6"))
    parser.add_argument("--libgcc", type=Path, default=Path("/usr/lib/libgcc_s.so.1"))
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("run on x86-64 Linux")
    keel, out = args.keel.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    commands = []

    def run(argv, native=False):
        env = dict(os.environ, PATH="/nonexistent") if native else None
        result = subprocess.run([str(a) for a in argv], capture_output=True, timeout=60, env=env)
        record = dict(argv=[str(a) for a in argv], empty_path=native,
                      rc=result.returncode, stdout=result.stdout.decode(errors="replace"),
                      stderr=result.stderr.decode(errors="replace"))
        commands.append(record)
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        return result

    def checked(argv, native=False):
        result = run(argv, native)
        if result.returncode:
            raise RuntimeError((argv, result.returncode, result.stderr.decode(errors="replace")))
        return result

    start = out / "start.s"
    start.write_text('''.att_syntax prefix
.text
.globl _start
_start:
    xor %ebp, %ebp
    and $-16, %rsp
    call main
    mov %eax, %edi
    call exit
    jmp _start
.section .note.GNU-stack,"",@progbits
''')
    checked([keel, "asm", "--target", "x86_64-unknown-linux-gnu", start, "-o", out / "start.o"], True)
    # No frame pointers: walking these frames requires their CFI. Independent
    # objects and differently sized stacks exercise multiple .eh_frame inputs.
    sources = []
    for name, target, stack in [("outer", "middle", 24), ("middle", "leaf", 40), ("leaf", "probe", 56)]:
        source = out / (name + ".s")
        source.write_text(f'''.att_syntax prefix
.text
.globl {name}
.type {name},@function
{name}:
.cfi_startproc
    sub ${stack}, %rsp
.cfi_def_cfa_offset {stack + 8}
    call {target}
.globl {name}_after
{name}_after:
    add ${stack}, %rsp
.cfi_def_cfa_offset 8
    ret
.cfi_endproc
.section .note.GNU-stack,"",@progbits
''')
        sources.append(source)
    source = out / "probe.c"
    source.write_text('''struct context;
extern int _Unwind_Backtrace(int (*callback)(struct context *, void *), void *);
extern unsigned long _Unwind_GetIP(struct context *);
extern char outer_after[], middle_after[], leaf_after[];
int outer(void);
static int callback(struct context *ctx, void *arg) {
    unsigned long pc = _Unwind_GetIP(ctx);
    int *mask = (int *)arg;
    if (pc == (unsigned long)outer_after) *mask |= 1;
    if (pc == (unsigned long)middle_after) *mask |= 2;
    if (pc == (unsigned long)leaf_after) *mask |= 4;
    return 0;
}
int probe(void) {
    int mask = 0;
    _Unwind_Backtrace(callback, &mask);
    return mask;
}
int main(void) { return outer() == 7 ? 42 : 1; }
''')
    records = []
    for assembler in ["keel", "clang"]:
        frames = []
        for source_asm in sources:
            obj = out / (source_asm.stem + "-" + assembler + ".o")
            argv = ([keel, "asm", "--target", "x86_64-unknown-linux-gnu", source_asm, "-o", obj]
                    if assembler == "keel" else ["clang", "-c", source_asm, "-o", obj])
            checked(argv, assembler == "keel")
            frames.append(obj)
        for compiler, flags in [("clang", ["-O0"]), ("clang", ["-O2"]), ("keel", []), ("keel", ["-O"])]:
            level = flags[0].lstrip("-") if flags else "plain"
            name = f"{assembler}-frames-{compiler}-{level}"
            obj = out / (name + ".o")
            if compiler == "clang":
                checked(["clang", *flags, "-fPIC", "-fomit-frame-pointer", "-fasynchronous-unwind-tables", "-c", source, "-o", obj])
            else:
                checked([keel, "cc", "--target", "x86_64-unknown-linux-gnu", *flags, "--emit-object", source, "-o", obj], True)
            inputs = [out / "start.o", *frames, obj, args.libgcc, args.libc]
            native, reference = out / (name + ".keel"), out / (name + ".gnu")
            checked([keel, "link", "--target", "x86_64-unknown-linux-gnu", *inputs, "-o", native], True)
            checked(["ld", "--eh-frame-hdr", "--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-o", reference, *inputs])
            for exe in [native, reference]:
                result = run([exe])
                assert (result.returncode, result.stdout, result.stderr) == (42, b"", b""), (exe, result)
            headers = checked(["readelf", "-lWS", native]).stdout.decode()
            assert "GNU_EH_FRAME" in headers and " RWE " not in headers
            assert headers.count(".eh_frame ") >= 4
            (out / (name + ".headers.txt")).write_text(headers)
            (out / (name + ".frames.txt")).write_bytes(checked(["readelf", "--debug-dump=frames", native]).stdout)
            # Discovery control: retain every section byte, disable only the
            # generated program-header advertisement in a separate executable.
            data = bytearray(native.read_bytes())
            phoff = int.from_bytes(data[32:40], "little")
            phnum = int.from_bytes(data[56:58], "little")
            for i in range(phnum):
                at = phoff + i * 56
                if int.from_bytes(data[at:at+4], "little") == 0x6474e550:
                    data[at:at+4] = bytes(4)  # PT_NULL
                    break
            else:
                raise AssertionError("missing PT_GNU_EH_FRAME")
            control = out / (name + ".no-advertisement")
            control.write_bytes(data)
            control.chmod(0o755)
            result = run([control])
            assert (result.returncode, result.stdout, result.stderr) == (1, b"", b""), (control, result)
            records.append(dict(case=name, mask=7, exit=42, gnu_matches=True, discovery_control_exit=1))
    (out / "report.json").write_text(json.dumps(dict(host=platform.node(), cases=records), indent=2) + "\n")
    print(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()

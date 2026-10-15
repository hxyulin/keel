#!/usr/bin/env python3
"""Differential archive/shared-library probes for the native x86-64 ELF linker."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
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
    keel, out = args.keel.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    records = []

    def source(name, text):
        path = out / name
        path.write_text(text)
        return path

    startup = source("start.s", '''.text
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
    start = out / "start.o"
    run(["clang", "-c", startup, "-o", start])

    def compare(name, objects, native_flags=(), system_flags=(), expected_output=b""):
        native, reference = out / (name + ".keel"), out / (name + ".gnu")
        run([keel, "link", *objects, *native_flags, "-o", native])
        run(["ld", *objects, *system_flags, "-o", reference])
        for exe in [native, reference]:
            process = subprocess.run([str(exe)], capture_output=True, timeout=10)
            assert (process.returncode, process.stdout, process.stderr) == (42, expected_output, b""), (str(exe), process.returncode, process.stdout, process.stderr)
        details = run(["readelf", "-lW", "-d", "-r", "-V", native], text=True).stdout
        assert " RWE " not in details
        (out / (name + ".readelf.txt")).write_text(details)
        records.append({"name": name, "exit": 42, "output_matches_gnu": True,
                        "native_bytes": native.stat().st_size,
                        "input_sha256": {str(p.name): hashlib.sha256(p.read_bytes()).hexdigest() for p in objects}})
        return native

    # GNU long member names, reverse member dependency order, unused duplicate.
    for name, text in [
        ("answer_with_a_long_member_name.c", "int tail(void); int answer(void){return tail();}"),
        ("tail.c", "int tail(void){return 42;}"),
        ("unused.c", "void _start(void){}"),
    ]:
        path = source(name, text)
        run([keel, "cc", "--emit-object", path, "-o", path.with_suffix(".o")])
    archive = out / "libhelpers.a"
    if archive.exists():
        archive.unlink()
    run(["ar", "rcs", archive, out / "tail.o", out / "unused.o", out / "answer_with_a_long_member_name.o"])
    compare("archive-direct", [start, archive], system_flags=["-static"])
    compare("archive-search", [start], ["--static", "-L", out, "-lhelpers"], ["-static", "-L", out, "-lhelpers"])

    # Shared initializer, executable exports, imported functions/data, absolute
    # function pointers, GOT address taking, and pointer identity across images.
    library_source = source("helper.c", '''int lib_value;
extern int executable_value;
__attribute__((constructor)) static void initialize(void){lib_value=25;}
int helper(void){return lib_value+executable_value;}
int (*get_helper(void))(void){return helper;}
''')
    library = out / "libhelper.so.1"
    run(["clang", "-fPIC", "-shared", "-nostdlib", library_source, "-Wl,-soname,libhelper.so.1", "-o", library])
    shutil.copy2(library, out / "libhelper.so")
    program = source("program.c", '''extern int lib_value;
int helper(void); int (*get_helper(void))(void);
int executable_value=17;
int (*pointer)(void)=helper;
extern int absent __attribute__((weak));
int answer(void){
    if (&absent || lib_value!=25) return 1;
    if (pointer!=helper || get_helper()!=helper) return 2;
    return pointer();
}
''')
    for opt in ["-O0", "-O2"]:
        obj = out / ("program" + opt + ".o")
        run(["clang", opt, "-fPIC", "-ffreestanding", "-c", program, "-o", obj])
        native = compare("shared" + opt, [start, obj, library], ["--rpath", "$ORIGIN"], ["--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-rpath", "$ORIGIN"])
        details = run(["readelf", "-d", native], text=True).stdout
        assert "libhelper.so.1" in details and str(out) not in details
        # Relocation and DSO loading at different randomized addresses.
        for _ in range(3):
            assert subprocess.run([str(native)], timeout=10).returncode == 42
    obj = out / "program-O0.o"
    compare("shared-search", [start, obj], ["-L", out, "-lhelper", "--rpath", "$ORIGIN"], ["-L", out, "-lhelper", "--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-rpath", "$ORIGIN"])
    bad = subprocess.run([str(keel), "link", "--static", str(start), str(obj), str(library)], capture_output=True, timeout=10)
    assert bad.returncode != 0 and b"--static" in bad.stderr

    # Combine archive extraction with shared imports, and let a DSO's strong
    # undefined reference pull its executable-side definition from an archive.
    bridge = source("bridge.c", "int helper(void); int executable_value=17; int answer(void){return helper();}")
    run([keel, "cc", "--emit-object", bridge, "-o", out / "bridge.o"])
    bridge_archive = out / "libbridge.a"
    if bridge_archive.exists():
        bridge_archive.unlink()
    run(["ar", "rcs", bridge_archive, out / "bridge.o"])
    compare("mixed-libraries", [start], ["-L", out, "-Bstatic", "-lbridge", "-Bdynamic", "-lhelper", "--rpath", "$ORIGIN"], ["-L", out, "-Bstatic", "-lbridge", "-Bdynamic", "-lhelper", "--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-rpath", "$ORIGIN"])
    exports = source("exports.c", "int executable_value=17;")
    caller = source("caller.c", "int helper(void); int answer(void){return helper();}")
    for path in [exports, caller]:
        run([keel, "cc", "--emit-object", path, "-o", path.with_suffix(".o")])
    export_archive = out / "libexports.a"
    if export_archive.exists():
        export_archive.unlink()
    run(["ar", "rcs", export_archive, out / "exports.o"])
    compare("dso-pulls-archive", [start, out / "caller.o", library, export_archive], ["--rpath", "$ORIGIN"], ["--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-rpath", "$ORIGIN"])

    # The non-default legacy definition must never win an unversioned import.
    versions = source("versions.map", "OLD { }; NEW { } OLD;")
    version_source = source("versions.c", r'''int old(void){return 3;} int current(void){return 42;}
__asm__(".symver old,choose@OLD\n.symver current,choose@@NEW");
''')
    version_library = out / "libversions.so"
    run(["clang", "-fPIC", "-shared", "-nostdlib", version_source, "-Wl,-soname,libversions.so", "-Wl,--version-script=" + str(versions), "-o", version_library])
    chooser = source("choose.c", "int choose(void); int answer(void){return choose();}")
    run([keel, "cc", "--emit-object", chooser, "-o", out / "choose.o"])
    native = compare("default-symbol-version", [start, out / "choose.o", version_library], ["--rpath", "$ORIGIN"], ["--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-rpath", "$ORIGIN"])
    assert "NEW" in run(["readelf", "-V", native], text=True).stdout

    # libc default symbol versions are retained as VERNEED records. IFUNC
    # imports (e.g. memcpy on glibc) are resolved by the runtime loader.
    libc_source = source("libc.c", '''extern int puts(const char*); extern void *memcpy(void*,const void*,unsigned long);
extern void exit(int);
int answer(void){char buffer[16]; memcpy(buffer,"hello libc",11); puts(buffer); exit(42);}
''')
    libc_obj = out / "libc.o"
    run(["clang", "-fPIC", "-ffreestanding", "-fno-builtin", "-c", libc_source, "-o", libc_obj])
    libc = Path(run(["clang", "-print-file-name=libc.so.6"], text=True).stdout.strip()).resolve()
    native = compare("libc-versioned", [start, libc_obj, libc], [], ["--dynamic-linker", "/lib64/ld-linux-x86-64.so.2"], b"hello libc\n")
    versions = run(["readelf", "-V", native], text=True).stdout
    assert "GLIBC_" in versions and "libc.so.6" in versions

    report = {"host": platform.node(), "platform": platform.platform(),
              "clang": run(["clang", "--version"], text=True).stdout.splitlines()[0],
              "ld": run(["ld", "--version"], text=True).stdout.splitlines()[0],
              "cases": records, "static_rejects_shared": True}
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

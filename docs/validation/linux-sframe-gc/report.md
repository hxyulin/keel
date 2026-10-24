# Linux SFrame, GC and CRT runtime validation

This report records native x86-64 Linux runtime validation of the
[ELF linker](../../elf-linker.md)'s SFrame support, opt-in section garbage
collection (`--gc-sections`), COMDAT selection and intact installed-CRT linking.

Environment: x86-64 Linux (CachyOS, kernel 7.2), glibc 2.44, Clang/LLD 23.1.1
and GNU binutils 2.47 (package `2.47-4`). Keel was a release build; the system
GNU tools in `/usr/bin` served as the reference linker.

## Results

All validation steps passed:

- **SFrame lookup.** Discovery through `dl_iterate_phdr` and installed libsframe
  decoding, exact function/row-boundary lookup, RA-undefined rows, segment
  backing and negative discovery/sorted controls, with GC disabled and enabled.
- **SFrame GC reconstruction.** Four input SFrame records became three: indices
  started out of order, FRE blocks had independent physical order, and a local
  dead function and its record disappeared. Keel and GNU outputs both ran the
  installed-libsframe consumer successfully, with exact reconstructed counts
  and 102-byte tables.
- **SFrame structure.** All eight liveness masks matched GNU structural output
  (structure only; these images are not executed).
- **Section GC.** Images including Clang `used`/`retain` inputs ran successfully
  when linked by Keel, GNU ld and LLD. Dead imports and cycles disappeared;
  retained records and live boundaries survived.
- **COMDAT.** Four cases (Clang/GCC fixtures, both input orders) matched GNU ld
  and LLD and executed successfully.
- **Installed CRT.** Twelve intact installed-CRT cases at O0/O2 passed with GC
  disabled and enabled; Keel and GNU executables each ran three times per case.
  Coverage includes argc/argv and stdio, signed 128-bit division through
  libgcc, `_DYNAMIC`, C++ startup/destruction, empty clone-table callbacks and
  template COMDAT selection. No CRT object stripping, custom startup adapter or
  system-linker fallback was used.
- **Unwinding.** Eight DWARF unwind/backtrace cases passed with and without GC,
  including Clang and Keel frame producers and discovery controls.
- **Rust tests.** All `keel-link` tests and the ELF CLI tests passed on Linux.

LLD's default `-z start-stop-gc` policy differs from Keel/GNU boundary-reference
retention, so LLD comparisons use `-z nostart-stop-gc` (see
[LLD's policy documentation](https://lld.llvm.org/ELF/start-stop-gc.html)).

## Reproduction

On x86-64 Linux with Clang, GNU binutils (with SFrame v3 output), libsframe,
readelf and Python 3:

```sh
cargo build --release -p keel
cargo test -p keel-link
cargo test -p keel --test elf_link
python3 suite/validate_elf_sframe.py --keel target/release/keel --out target/elf-sframe
python3 suite/validate_elf_sframe.py --keel target/release/keel --gc-sections --out target/elf-sframe-gc
python3 suite/validate_elf_crt.py --keel target/release/keel --out target/elf-crt
python3 suite/validate_elf_crt.py --keel target/release/keel --gc-sections --out target/elf-crt-gc
python3 suite/validate_elf_unwind.py --keel target/release/keel --out target/elf-unwind
python3 suite/validate_elf_unwind.py --keel target/release/keel --gc-sections --out target/elf-unwind-gc
```

## Limits

This covers native Linux runtime behavior for the tested subsets, including
SFrame reconstruction with actual record removal. It does not test maximum
allocations or turn the 64 MiB SFrame admission estimate into a process-memory
bound. It does not add multi-producer or grouped SFrame, native ELF TLS, PIE,
automatic CRT discovery, shared-object output or general C++ exception support.
The COMDAT inputs are retained real compiler fixtures; this does not claim that
all C++ programs or arbitrary toolchain object forms work.

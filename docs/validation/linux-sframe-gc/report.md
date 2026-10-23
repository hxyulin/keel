# LinuxHost Linux SFrame, GC and CRT runtime validation

Validated on 2026-10-22 on `linuxhost`, CachyOS x86-64 Linux
7.2.10-1-cachyos, glibc 2.44, Clang/LLD 23.1.1 and GNU binutils 2.47
(package `2.47-4`). The release executable was built offline and locked from
source commit `14fefe4a9e6f8883321a46c7c2fdca8cd375821b`. Only the Python
validators were subsequently changed; production linker code was unchanged.

The source was unpacked into an isolated directory, without changing LinuxHost's
existing checkout. Builds and validators ran sequentially under the shared
`/tmp/keel-perf-20261019.lock`, using `suite/fast_opt_locked.py`. Explicit PATH
selection used `/usr/bin` GNU tools rather than the Nix wrapper tools also
installed on the host. The recorded tool versions, input/archive/binary hashes,
validator hashes, workload-lock records and reports identify the exact run.

## Runtime results

All eleven final validation steps passed:

- SFrame discovery through `dl_iterate_phdr` and installed libsframe decoding,
  exact function/row-boundary lookup, RA-undefined rows, segment backing and
  negative discovery/sorted controls, with GC disabled and enabled.
- Actual GC filtering/reconstruction: four input SFrame records became three;
  indices started out of order and FRE blocks had independent physical order.
  A local dead function and its record disappeared. Both Keel and GNU outputs
  executed the installed-libsframe consumer and returned 42, with exact
  reconstructed counts and 102-byte tables.
- All eight SFrame liveness masks matched GNU structural output. This step
  compares structure; its images are not executed.
- ELF GC images, including Clang `used`/`retain` inputs, executed successfully
  through Keel, GNU ld and LLD. Dead imports/cycles disappeared; retained
  records and live boundaries survived.
- Four COMDAT cases (Clang/GCC fixtures, both input orders) matched GNU ld and
  LLD and executed successfully: twelve linked images.
- Twelve intact installed-CRT cases at O0/O2 passed with GC disabled, and the
  same twelve passed with GC enabled. Keel and GNU executables each ran three
  times per case: 144 CRT executions. Coverage includes argc/argv and stdio,
  signed 128-bit division through libgcc, `_DYNAMIC`, C++ startup/destruction,
  empty clone-table callbacks and template COMDAT selection. No CRT object
  stripping, custom startup adapter, or system-linker fallback was used.
- Eight DWARF unwind/backtrace cases passed without GC and eight with GC,
  including Clang and Keel frame producers and discovery controls.
- All 224 Linux `keel-link` tests and seven ELF CLI tests passed.

The first GC-oracle attempt failed in LLD because its default
`-z start-stop-gc` policy differs from Keel/GNU boundary-reference retention.
The validator now explicitly uses `-z nostart-stop-gc` for LLD in both comparison
paths and records that policy. The corrected individual replay and a clean
complete replay both passed. This required no production linker fix. See
[LLD's policy documentation](https://lld.llvm.org/ELF/start-stop-gc.html).

## Evidence and reproduction

[summary.json](summary.json) contains every final validator report and source,
executable and validator hashes. [machine.txt](machine.txt),
[results.txt](results.txt), [linker-tests.log](linker-tests.log),
[cli-tests.log](cli-tests.log) and [workload-lock.jsonl](workload-lock.jsonl)
retain the qualification and test results. [evidence.tar.gz](evidence.tar.gz)
contains the full command logs, reports, input provenance and readelf outputs.

[run-final.sh](run-final.sh) records the exact build and validator commands.
Its paths refer to the isolated LinuxHost run directory
`/tmp/keel-linux-validation-20261022.S1VzCu`, which also retains generated
objects and executable images. For a new run, use a fresh directory and invoke
the recorded commands under the shared workload lock.

## Remaining boundaries

This establishes native Linux runtime coverage for the tested subsets, including
SFrame reconstruction with actual record removal. It does not test maximum
allocations/RSS or turn the 64 MiB SFrame estimate into a process-memory bound.
Subsequent [admission-domain memory testing and profiling](../../../suite/evidence/elf-link-profile-20261022/README.md)
measured near-cap inputs and rejection paths; it does not establish an absolute
whole-linker RSS bound.
It does not add multi-producer/grouped SFrame, native ELF TLS, PIE, automatic
CRT discovery, shared-object output, or general C++ exception support. The
COMDAT inputs use retained real compiler fixtures; these checks do not claim
that all C++ programs or arbitrary toolchain object forms work.

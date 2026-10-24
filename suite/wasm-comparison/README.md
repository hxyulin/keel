# Freestanding Wasm comparison

A small, fixed comparison of Keel's Wasm output (plain and `--wasm-opt basic`)
against Clang `-O0` and `-O2` on the same C source. `corpus.c` has eight cases:
unsigned scalar and i64 edge values, an integer loop, branch/switch, a checksum,
pointer casts and byte mutation, a local array, and direct recursive calls. No case
is silently dropped. This covers an integer/pointer subset only; it is not an
application benchmark or a C ABI test.

## Running

```sh
cargo build --release -p keel
python3 -m unittest discover -s suite/wasm-comparison -p 'test_*.py'
python3 suite/wasm-comparison/run.py target/release/keel /path/to/new-output-dir
```

The output directory must not exist yet and should be outside the repository. Pass
`--static-only` to skip timing. `CLANG`, `NODE` and `WASM_LD` select specific
tools; otherwise `wasm-ld` is looked up on `PATH`, then in the active Rust
sysroot's `gcc-ld/wasm-ld`. The runner does not build Rust or install anything.
A missing compiler or failed assembly/object emission stops the run; Clang link
failures are recorded as emission failures with diagnostics kept where possible.

## What is recorded

`manifest.json` records tool and source hashes, commands, per-stage outcomes,
per-function body sizes, operator and local counts, and a static ranking. Custom
sections are stripped uniformly for comparison; raw modules are kept.

`decode.py` is a bounded statistics decoder. It rejects imports, non-integer
signatures, non-fixed or shared memory, tables, start functions, elements,
FP/SIMD/atomic/bulk-memory instructions and unknown opcodes before execution, and
checks section and body boundaries, LEB widths, block and global types, and
single-result MVP signatures. Node performs full semantic validation and checks
imports before instantiation.

## Correctness

Each module exposes its own scalar object and buffer offsets. `harness.cjs` fills
the whole 1,040-byte object (with an eight-byte canary on each side), then compares
every byte and every return value with an independent BigInt oracle. Four edge
seeds and lengths 0, 1, 31, 1024 and 16384 give 640 invocations across the four
profiles. Absolute addresses and unrelated stack bytes are not compared. The corpus
never relies on undefined behavior to produce traps; trapping IR is covered by the
CLI and optimizer test suites.

All four profiles must pass; failed profiles are never filtered out. Failed
emission, static policy, correctness or runtime stages are recorded in the manifest
and produce a nonzero exit.

## Timing

Timing is a single serialized attempt and is deferred (not retried) if the
machine is busy: before and during every round, load must be at most half the
logical CPU count with no other process at 50% CPU or more, and a failed `ps` probe
also defers timing. Static and correctness results are kept either way.

- CLI samples are end to end, including process startup and Clang linking. Object
  emission is diagnostic only and never mixed into these samples.
- Seven seeded, shuffled, paired rounds record raw values, medians, ranges and
  ratios.
- Node runs in one process with default engine flags, 12 warmup batches per
  profile and case, and eight invocations of 16,384 iterations per sample
  (recursion: 512 iterations at depth eight). Getter, scalar and i64 cases use
  20,000 invocations and are boundary microbenchmarks. All samples include
  JavaScript call overhead, and fixed warmup does not guarantee tier stability.

Partial compile samples are kept if a later gate fails, without summary statistics;
missing samples are never recorded as zero. With `--static-only` the manifest says
timing was not requested. No application-level or geometric-mean conclusion follows.

## Setup differences

Every module has a fixed 64 KiB memory. Keel uses data floor 1024 and a checked
software stack in [32768, 65536); Clang uses a 32 KiB stack-first layout with data
at 32768. Frame layout, stack pointers, allocation checks and stack-exhaustion
behavior differ, so a common stack budget and normalized object checks do not make
the two ABIs or trap policies identical. The source stays within the plain i32/i64
call boundary and uses no FP, SIMD, imports, libc, LTO or fast math; Clang is run
with `-mcpu=mvp`. Optimization levels intentionally differ.

The runner's own tests (`test_run.py`, `test_decode.py`) use fake tools and gates,
with no real compiler invocation or timing.

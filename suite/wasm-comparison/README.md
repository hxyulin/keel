# Finite freestanding Wasm comparison

Eight C cases, unchanged between Keel plain/basic and Clang `-O0`/`-O2`:
unsigned scalar/i64 edges, integer loop, branch/switch, checksum, pointer casts
and byte mutation, local array, and direct recursive calls. No unsupported
fixture is silently dropped. This is an integer/pointer subset comparison,
not an application benchmark or a general C ABI test.

Build the existing release CLI with the project's pinned Rust/dependencies,
then run under the same suite lock. Destination must be new and outside Git:

```sh
python3 suite/fast_opt_locked.py /tmp/comparison-build-UNIQUE.jsonl \
  cargo build --release --offline --locked -j2 -p keel
python3 -m unittest discover -s suite/wasm-comparison -p 'test_*.py'
python3 suite/fast_opt_locked.py /tmp/comparison-run-UNIQUE.jsonl \
  python3 suite/wasm-comparison/run.py target/release/keel /tmp/comparison-UNIQUE
```

Use `--static-only` to omit timing. `CLANG`, `NODE`, and `WASM_LD` can select
existing tools. Linker discovery checks PATH, then the active Rust sysroot's
existing `gcc-ld/wasm-ld`. No installation, remote access, cache change or Rust
build occurs inside the runner. Missing/link-failed Clang output is recorded as
an emission failure; assembly and object diagnostics are retained when possible.
A missing compiler or failed assembly/object emission stops the run.

`manifest.json` records tool hashes, source hash, commands, stage outcomes,
all function bodies/operators/locals and static ranking. Custom sections are
removed uniformly; raw modules are retained. The decoder rejects imports,
noninteger signatures, nonfixed/shared memory, tables/start/elements,
FP/SIMD/atomics/bulk instructions and unrecognized opcodes before execution.
Node independently validates the bytes and checks imports before instantiation.

Each module exposes its own scalar object and buffer offsets. The harness
initializes the entire 1,040-byte object (eight-byte canary on each side), then
compares every byte and each return value with an independent BigInt oracle.
Four edge seeds and lengths 0, 1, 31, 1024, 16384 produce 640 invocations across
four profiles. Neither absolute addresses nor unrelated stack bytes are compared.
Valid trapping IR remains a separate existing CLI/optimizer regression suite;
this C corpus never uses undefined behavior as a trap oracle.

Timing uses one serialized attempt. Preflight and every round require load at
most half the logical CPU count and no other process at 50% CPU or above.
Contention defers timing without retrying; static/correctness results remain.
CLI end-to-end samples include startup and Clang linking. Object emission is a
diagnostic, never mixed into those samples. Seven seeded, shuffled paired rounds
record raw values, medians, ranges and ratios. Node uses a single process and
unchanged engine flags, 12 declared warmup batches per profile/case, and eight
invocations of 16,384 iterations per work sample (recursion: 512 iterations,
depth eight). Results are consumed outside the timed batch. Wrapper getter,
scalar and i64 batches use 20,000 invocations and are explicitly boundary
microcases. All samples still include JS calls and return handling; fixed warmup
does not prove engine-tier stability. No application/geometric-mean claim follows.

Memory is fixed at 64 KiB in every module. Keel uses data floor 1024 and checked
software stack [32768,65536); Clang uses stack-first 32 KiB and data base 32768.
Frame layout, stack pointers, allocation checks, and exhaustion behavior differ.
The common stack budget and normalized object checks do not make their ABIs or
trap policies identical. The source stays within the supported plain i32/i64
call boundary, uses no FP/SIMD/imports/libc/LTO/fast math, and Clang explicitly
selects `-mcpu=mvp`. Optimization levels intentionally differ.

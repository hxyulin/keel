# Extended runtime kernels

This standalone collection complements the fixed 21 kernels in
`crates/keel-cranelift/kernels`. It never edits or combines scores with that
corpus, its 795-line oracle, or the four Lua workloads. It is intentionally
separate from the download/build `suite.toml` programs.

Twelve ordinary C workloads (corpus `runtime-kernels-v2`) use no Keel annotations, `restrict`, alignment promises,
or signed-overflow assumptions:

| Case | Work |
| --- | --- |
| positive32 / positive64 | Positive-only i32/i64 reductions, zero false arm |
| balance32 / balance64 | Signed negative contributions, nonzero false arm |
| sensor_mix | Low unsigned 16-bit readings plus signed i32 offsets widened to i64 |
| clamped | Saturating signed input into a byte-range total |
| unsigned_high | High-bit unsigned values, including UINT64_MAX |
| adjacent | Widened signed differences and absolute distances |
| buckets | Eight stack counters indexed by input, conditional contributions |
| run_balance | Stateful conditional accumulation with saturating debit |
| widen32 / widen_u32 | Direct signed/unsigned i32-to-i64 wrapping sums |

Totals and weighted products use unsigned 64-bit modular arithmetic. Signed
subtraction/negation is widened or range-safe. Inputs mix xorshift64 samples
with INT32_MIN/MAX, INT64_MIN/MAX, UINT64_MAX, high-bit boundaries, and clamp
boundaries. All three input arrays are read-only. `oracle.py` uses Python
integers and independent scalar formulations (including weighted contributions
instead of stack buckets), reducing modulo 2^64 at output. It does not invoke
Clang to establish the expected answer. Hand-computed tests cover empty inputs,
small arrays, overflow/extremes, and generator snapshots.

## Run

Pass absolute compiler paths and a **new** output directory. No Cargo build is
needed. Linux additionally supports `--cpu N`, which pins only this process and
its children; governors, boost, and system settings are observed, never changed.

```sh
python3 suite/runtime-kernels/run.py \
  --keel /absolute/path/to/keel --clang /absolute/path/to/clang \
  --label macos-arm64-current \
  --output target/runtime-kernels/arm-smoke --smoke

python3 suite/runtime-kernels/run.py \
  --keel /absolute/path/to/keel --clang /absolute/path/to/clang \
  --label linux-x86_64-current --cpu 2 \
  --output target/runtime-kernels/x86-full

python3 -m unittest discover -s suite/runtime-kernels -p 'test_*.py'
```

The entry point serializes its whole run (every compile, link, correctness
check and native timing) under a host-wide lock; do not wrap it in another
lock, which would deadlock. `--worker` is internal. Output
directories are never reused. No build cache, Cargo target, or baseline
checkout is touched.

For an optimization experiment, add `--baseline-keel /absolute/path/to/baseline-keel`.
This adds `baseline-full` using the same Full flags as the current compiler.
Both compilers are hashed, freshly build objects, pass all oracle checks, and
participate in the same randomized compile/runtime rounds. This reduces drift
between separate campaigns; internal layout and physical code pages remain
uncontrolled. Inspect unchanged-function controls before attributing small gains.

`--full-passes comma,separated,passes` replaces only the current Full pipeline
for an opt-in experiment. The baseline compiler retains its default pipeline
unless `--baseline-passes comma,separated,passes` is supplied alongside
`--baseline-keel`. Use the same explicit pipeline for both compilers to isolate
an implementation change in an experimental pass. None/Fast/Clang remain
unchanged. Exact flags are saved in the results;
label such runs experimental until correctness and timing gates pass.

Configurations are Keel None (`--opt-tier none`, baseline selection), Fast and
Full (`-O --opt-tier fast|full`, including all selection passes), and Clang `-O2`.
All Keel configurations use the default Full allocator.
`--gcc /absolute/path/to/gcc` adds GCC O2 only when explicitly supplied and
present. The driver is compiled once with Clang O2, and each kernel object is
linked with that same driver, without LTO. Keel executable SHA-256 is the
identity when its CLI lacks `--version`.

Default lengths: 0, 1, 3, 4, 7, 8, 15, 16, 17, 31, 33, 257, 4096, 65536,
1048576. These cover empty/short cases, several vector thresholds and tails,
and roughly 80 KiB through 20 MiB of combined input storage; actual cache
residency depends on the host and workload. Smoke omits the largest length,
uses one measured compile/runtime round, and targets 32768 elements per sample.
The seed defaults to 20261021 and can be overridden with `--seed`.

## Measurement and artifacts

The runner randomizes configuration order for compile rounds and all
configuration/case/length jobs for runtime rounds, deterministically from the
seed. Compile defaults are one warmup plus three measured rounds. Every round
copies the source to a new directory and writes a fresh object; no object is
reused between rounds, runs, or checkouts. Process-to-object wall time includes
startup, source reading, frontend, optimization, and integrated object emission;
it excludes driver compilation, linking, and the separately retained assembly
emission. Sources are header-free and there is no separate preprocessing step.

All configuration/case/length checks must pass the Python oracle before any
runtime sampling. Each timing process rechecks its expected answer and performs
one warmup call outside the timed interval. Input allocation/generation is also
outside it. `clock_gettime(CLOCK_MONOTONIC)` works on Linux and macOS. The timed
loop performs external kernel calls, a volatile sink write, and an unsigned
checksum addition; it never XORs repeated answers away. The final checksum must
match the independent expected result times the repetition count modulo 2^64.
No LTO is used, so the driver cannot hoist a pure kernel call out of the loop.
Every timed variant uses one canonical executable path, installed by atomic
hard-link replacement without changing the retained executable inodes. This
keeps argv[0] constant; it does not control physical code pages or internal layout.
Runtime defaults to five raw rounds and `max(3, target_elements / max(n, 1))`
repetitions, capped at one million, with `target_elements=2000000` by default.

Artifacts are `correctness.json`, `results.json`, `commands.jsonl`, `lock.jsonl`,
`report.html`, and retained sources/objects/assembly/executables. Git revision is optional for source archives; source and compiler hashes remain
authoritative. They include
raw round times, exact argv/flags, stdout/stderr, seed, length, canonical
little-endian input hashes, source/compiler/object/executable hashes,
architecture/host labels, process affinity where supported, load averages, and
available Linux frequency settings. Failures still save partial JSON evidence
and produce a nonzero exit code. The HTML shows per-case/length runtime medians;
compile times remain separate in JSON.

Smoke validates the harness and correctness; it does not establish speedups.
Short/empty cases include substantial call, sink, checksum, and clock overhead.
Repeated inputs measure warm steady-state access, not cold-cache behavior.
None/Fast/Full share one source file, so compile measurements describe the
collection, not individual kernels. CLI startup can dominate this small file.
There is no claim that these twelve integer cases represent whole applications,
all C semantics, floating point, or allocator quality. Run a longer campaign
with host-specific affinity and inspect raw variation before comparing speed.

The v2 cohort adds the two direct widening sums to the original ten-case v1
collection. Compare matching case/length/configuration cells across versions;
do not attribute a changed corpus mean to compiler changes.

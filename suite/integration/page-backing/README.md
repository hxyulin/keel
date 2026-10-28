# Fixed code-backing application comparison

This Linux/x86-64 research harness implements a controlled comparison motivated
by [the executable-backing investigation](../../../docs/benchmarks/page-backing.md).
It does not change compiler behavior or provide a general-purpose ELF loader.
Run on an otherwise available benchmark host. No automatic remote access,
installation, governor changes or global ASLR changes occur.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s suite/integration/page-backing -p 'test_*.py'
sudo python3 suite/integration/page-backing/run.py \
  --config /absolute/config.json --output /absolute/NEW-check --execute --check-only
sudo python3 suite/integration/page-backing/run.py \
  --config /absolute/config.json --output /absolute/NEW-run --execute
sudo python3 suite/integration/page-backing/analyze.py /absolute/NEW-run
```

Privilege is required to inspect PFNs and set process scheduling, lock a bounded
private pool, and then drop application credentials. Every application runs as
the configured non-root UID. The helper is compiled with native Clang from this
directory and its source/binary/compiler hashes are recorded. Processes use a
fixed CPU, `SCHED_OTHER`, nice zero and child-only ASLR-off. Applications must be
little-endian x86-64 ELF PIE with exactly one read/execute, non-writable PT_LOAD,
file-size equal to memory-size, no executable-byte relocations and equal RX
virtual offsets across variants. Unsupported layouts fail clearly.

The private file's read-only parent mapping is mlocked. The runner records its
physical frames and selects disjoint, predetermined sets. Each set is tested in
both physical-sorted and fixed-seed shuffled assignment. Each variant's **own**
RX bytes are written between launches, preserving and checking parent PFNs. A
private preload helper compares loaded code against `/proc/self/exe`, installs
the specified page mappings at the same virtual addresses, and compares every
code byte again. It waits at a pipe barrier. The privileged parent verifies all
live child code PFNs, credentials and scheduling before releasing it. Unequal
code sizes use matching prefixes of each set; their footprint difference is
retained. Non-code backing is not normalized.

The primary interval starts at barrier release and ends at process completion.
It excludes launch, loading, remapping, hashing and PFN observation; remaining
constructors after this helper, application work and captured I/O are included.
It is **not whole-process startup** or randomized deployment performance. Page
prefaulting and the preload library change the environment. A VMA may be split
by page assignment. Code frames are verified before release, not continuously.

Configuration supplies `uid`, `cpu`, `controller_cpu`, `expected_base`, `clang`,
`pool_directory`, `groups`, `blocks` (multiple of six), `repetitions` (1–4),
`seed`, `arms` (three labels, in control-A/control-B/optimized order), and
`workloads`. Each workload has `id`, `binaries` mapping arms to `{path,sha256}`,
`argv` (one shared argv0 and tail), `cwd`, `environment`, `inputs` (hashed file
dependencies), `expected` (hashed stdout file), and optional hashed `stdin`.
All paths are host-local and existing files. Inputs are hash-checked before and
after execution. Put generated configs/results under ignored `suite/reports/`.
The pool is bounded at 64 MiB. `budget_seconds` defaults to 1800.

All route correctness gates finish before two warmup blocks. Measured blocks
balance all six three-arm orders for every workload/set/assignment. Complete
fixed-seed schedules freeze before application execution. No calibration,
outlier exclusion, retry or timing subtraction occurs. Every invocation must
produce exactly its expected stdout, empty stderr and return zero. A failure
stops the run, retains diagnostics and reaps the owned process group. The pool
is unlocked, unmapped and removed at exit; the output/helper artifacts remain.

Analysis compares control B/A and optimized/sqrt(A*B), averaging log-ratios over
both assignments within each physical set. Two-way bootstrap resamples physical
sets and timing blocks (10,000 draws). Identical-byte controls must have their
aggregate interval wholly within ±2%, and every set/assignment control estimate
within ±5%. Otherwise profitability remains unresolved. These are practical
screening margins, not universal precision guarantees. Report all placements;
never choose the fastest placement for each compiler.

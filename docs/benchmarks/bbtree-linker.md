# bbtree linker trial

On 2026-10-25, replacing internal linker ordered collections with bbtree 0.1.5
reduced whole-process link time on generated workloads with 65,536 symbols.
Public `Dylib.exports` and `weak_exports` remain standard `BTreeSet` values.
Mach-O export trie edges also retain standard maps because most nodes have
one edge and bbtree allocates larger nodes. A subsequent vendored alignment
change removes most of the direct Mach-O memory increase described below.

## Results

Baseline: `7b43d15e33c8926e22c2e5abbab6743a65234fc3`, release build.
Candidate: the same revision with the collection changes in this branch.
Host: Apple M5 Max, arm64 macOS. Each pair consumes identical objects and writes
to the same output path. Times include startup, input parsing, output I/O, and
Mach-O signing. Two warmup rounds precede nine measured rounds, with tool order
shuffled using seed 20261025. Values below are medians.

- Mach-O, 64 symbols: 7.99 ms to 7.95 ms, effectively unchanged.
- Mach-O, 65,536 symbols, direct objects: 82.11 ms to 77.54 ms, 5.6% lower.
- Mach-O, 65,536 symbols, archive: 95.42 ms to 89.91 ms, 5.8% lower.
- ELF, 65,536 symbols, direct objects: 53.20 ms to 46.16 ms, 13.2% lower.
- ELF, 65,536 symbols, archive: 305.45 ms to 204.20 ms, 33.1% lower.

Peak resident memory, measured in a separate single run with `/usr/bin/time -l`,
rose from 90.5 to 100.9 MiB for large direct Mach-O links. It fell from 103.7
to 101.9 MiB for Mach-O archives, from 69.2 to 65.4 MiB for direct ELF links,
and from 69.0 to 65.2 MiB for ELF archives. The small case rose from 7.6 to
8.2 MiB. These are individual observations, not repeated memory measurements.

The generated symbols have long shared prefixes. Each of 32 object shards
defines 2,048 functions and contains data references to its functions and the
next shard. Archives store shards in reverse demand order. This exercises
symbol lookup and archive rescanning; it does not establish speedups for
production C++ or Rust programs, COMDAT, or unwind-heavy inputs. ELF files
were cross-linked on macOS and were not executed on Linux.

[Raw samples](bbtree-linker.json) include binary and input hashes, exact
commands, failure diagnostics, memory observations, and execution order.

## Collection scope

The first trial also replaced the export trie's tiny edge maps. Large Mach-O
direct links then took 120.97 ms instead of 82.06 ms, and peak resident memory
rose from 90.6 to 357.8 MiB. Restoring standard maps for those edges removed
that regression. [Initial samples](bbtree-linker-all-maps.json) cover four
completed cases; that run stopped before ELF archive timing because Apple's
`ar` silently omitted the ELF members. The final harness writes regular
archives directly and completes all five cases.

Two temporary collections need explicit drops because bbtree's drop checking
keeps their borrowed data live until destruction. Dense symbol caches and
vector fast paths retain their existing representations.

## Vendored allocation alignment

The linker now uses the published 0.1.5 sources in `third-party/bbtree`, with
an opt-in `cache-line-alignment` feature. It lowers allocation alignment from
the full 512- or 1,024-byte page size to 64 bytes. Stricter alignment required
by keys, values, pointers, or a leaf annex still applies. Node sizes,
capacities, and slot offsets stay unchanged. This targets allocator padding
without shrinking nodes or changing tree depth.

In a new paired comparison with the same std baseline binary, large direct
Mach-O links used 91.7 MiB versus std's 90.6 MiB, a 1.1 MiB increase. Mach-O
archives used 92.9 versus 106.0 MiB. ELF direct and archive links used 65.2
and 65.0 MiB versus 69.2 and 68.9 MiB. These memory values are separate
single-run observations, as in the initial trial.

Median times versus std in that comparison were:

- Mach-O, 64 symbols: 8.14 ms to 8.48 ms, 4.2% higher.
- Mach-O, large direct: 85.33 ms to 80.91 ms, 5.2% lower.
- Mach-O, large archive: 99.61 ms to 94.64 ms, 5.0% lower.
- ELF, large direct: 54.71 ms to 47.14 ms, 13.8% lower.
- ELF, large archive: 339.48 ms to 224.97 ms, 33.7% lower.

A second paired run used the previous upstream-bbtree binary as the baseline;
its hash matches the initial trial's candidate. Cache-line alignment reduced
large Mach-O direct memory from 100.9 to 91.7 MiB and archive memory from
102.0 to 93.0 MiB. Median times differed by less than 1.1% for the large
direct links and Mach-O archive. The ELF archive median was 3.8% lower, with
overlapping sample ranges. The small Mach-O median was 1.3% higher.
The patch's demonstrated benefit is memory usage; these comparisons do not
establish a further speed improvement over upstream bbtree.

[Samples versus std](bbtree-vendor-vs-std.json) and
[samples versus upstream bbtree](bbtree-vendor-vs-upstream-complete.json)
retain hashes, commands, diagnostics, and every timing sample. Both runs
required identical output bytes and failure diagnostics and verified and
executed the Mach-O outputs. The benchmark fields now use `baseline` and
`candidate` so either baseline implementation is named accurately.

The remaining 1.1 MiB direct-link increase is small enough to retain this
experiment given the measured large-link improvements. Production C++ and
Rust workloads are still needed before making a general adoption decision.
Vendor provenance and the patch's alignment invariants are documented in
[`VENDORED.md`](../../third-party/bbtree/VENDORED.md).

A separate memory-only comparison ran each of std, upstream bbtree, and the
vendored version three times per case in shuffled order, without concurrent
builds. Medians for large direct Mach-O links were 90.6, 100.7, and 91.7 MiB,
respectively. The approximately 9 MiB reduction versus upstream and 1 MiB
increase versus std persisted across these samples.
[Repeated memory samples](bbtree-vendor-memory.json) retain every observation
and require matching output hashes. The harness now collects three memory
runs by default; `--memory-rounds` changes that count.

The vendored suite passed 387 unit and documentation tests with the feature
off and 411 with all features on. Keel's linker and CLI packages passed
393 tests with two ignored. Strict Clippy still reports the same ten existing
linker diagnostics as before vendoring.

## Validation

`cargo test -p keel-link -p keel` passed 393 tests with two ignored. Formatting
and `git diff --check` passed. The generated benchmark requires byte-identical
outputs and identical exit status, stdout, and stderr for duplicate and
undefined symbol errors in every case. Each generated Mach-O executable ran
successfully and passed strict signature verification.

The existing runtime-kernel correctness gate also passed: 288 oracle checks
across six executables, plus instruction, relocation, and strict signature
checks. Strict Clippy remains blocked by existing warnings. A library-only
comparison reported 11 warnings on the baseline and 10 on the candidate;
the candidate introduced no additional diagnostic categories.

## Reproduce

Build the baseline in a separate checkout at the revision above and retain
its binary before building the candidate. Cargo commands here use ordinary
`cargo`; use the local build scheduler where configured.

```sh
cargo build --release -p keel
python3 suite/native-linker/collections_bench.py \
  --baseline /absolute/path/to/keel-std \
  --candidate target/release/keel --out /tmp/keel-collections-results
cargo test -p keel-link -p keel
python3 suite/readme-tools-bench/run.py --keel target/release/keel \
  --out /tmp/keel-collections-kernels --correctness-only
```

Output directories must not already exist. The benchmark requires arm64 macOS
and Apple's cross-target assembler. Defaults are 65,536 symbols and nine
rounds; `--symbols` and `--rounds` allow smaller trials.

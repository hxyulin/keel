# bbtree linker trial

On 2026-10-25, replacing internal linker ordered collections with bbtree 0.1.5
reduced whole-process link time on generated workloads with 65,536 symbols.
Public `Dylib.exports` and `weak_exports` remain standard `BTreeSet` values.
Mach-O export trie edges also retain standard maps because most nodes have
one edge and bbtree allocates larger nodes.

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

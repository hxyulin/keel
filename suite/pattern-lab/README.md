# Keel pattern lab

This download-free laboratory compares native assembly patterns against independent C oracles. Python 3.10+ standard library and native Clang are required for checks; listing needs only Python. It does not change Keel compiler behavior. First review the fixture semantics in `PATTERNS.md` and the strict version-one `manifest.schema.json`.

```sh
python3 suite/pattern-lab/run.py list
python3 suite/pattern-lab/run.py check --output suite/reports/pattern-lab-check-001
python3 suite/pattern-lab/run.py verify suite/reports/pattern-lab-check-001
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s suite/pattern-lab/tests -p 'test_*.py'
```

Pass one or more manifest paths to select cases. Paths inside manifests are relative to `suite/pattern-lab`. Unknown fields, duplicate JSON keys/IDs, unsupported schema versions, invalid architectures, booleans/floats in integer fields, missing or escaped source paths, and integers outside `[0,18446744073709551615]` are rejected. Preserve JSON integers exactly: do not round-trip seeds through JavaScript numbers. Mask must be below allocated words. A fixture must additionally prove every effective address fits its allocation; the ARM address case uses 512 words, mask 255, a 16-byte stride and an 8-byte load (4088 bytes required).

`list` does not compile or run code. `check` builds only native patterns, retaining separate driver, oracle and variant objects with `-fno-lto`, then checks every manifest seed at iterations 0, 1, 2, 17, 257 and any additional requested counts. Wrong-architecture patterns have explicit `unsupported` records, never PASS. An entirely unsupported selection exits 3; failures exit 1; successful native checks exit 0. Driver/oracle setup and verification run outside the kernel timer. A driver invocation calls `pattern_kernel(iterations, seed, data, mask)` exactly once, consumes its result, and exits nonzero on an oracle mismatch or a violation of iterations-zero returning seed.

The data array is generated independently of the input seed: initialize unsigned 64-bit `x=0x243f6a8885a308d3`; for each word, set `x=x*6364136223846793005+1442695040888963407` modulo 2^64 and store x. Oracles receive the same array. All integer arithmetic in fixture semantics wraps modulo 2^64. Data setup, oracle computation, process startup and JSON output are excluded from the kernel interval.

Outputs must name a **new** directory. All commands use argument arrays, no shell, with bounded subprocess deadlines (default 20 seconds each and 180 seconds total; configurable maxima 60/600). Inputs, manifests, compiler, driver, objects, binaries and generated artifacts are hashed. Compiler version, host, source commit/tree/dirty state, flags, exact commands, raw stdout/stderr and failures are retained. `otool` on macOS or available `llvm-objdump`/`objdump` on Linux records object and linked disassembly; missing tools or failures remain explicit. `verify` detects subsequent input, manifest, executable or report mutation using `integrity.json`; this is a local integrity record, not a signed attestation. Preserve output directories to keep failures and fixes auditable. Optional adapters are independent subprocess CLIs; see `TOOLS.md` (for example `python3 suite/pattern-lab/tools.py discover --output NEW_FILE`).

Benchmarking is opt-in and should follow independent review:

```sh
python3 suite/pattern-lab/run.py bench suite/pattern-lab/patterns/SELECTED.json \
  --execute --output suite/reports/pattern-lab-bench-001
```

Every selected native build and correctness check finishes before any calibration. The protocol and source/object/binary hashes freeze before calibration. Each variant gets one 100,000-iteration pilot; the fastest pilot determines a common iteration count targeting 100 ms (configurable 100–250 ms). Counts above the explicit 1,000,000,000 cap fail as unattainable. A target is an estimate, not a duration guarantee. All variants then use that fixed count for two warmups each, followed by two complete cycles of all variant permutations: AB/BA for two variants, six permutations for three. The complete crossover blocks are retained. No hidden retries, outlier removal, empty-baseline subtraction or automatic performance PASS occurs. Process timeouts, checksum mismatches, migration or deadline failures end the campaign and preserve logs.

Samples report raw nanoseconds (or TSC ticks), work count, work unit and elapsed per work. Ratios are candidate/first-variant elapsed; values below one indicate less time in this session. The deterministic paired bootstrap resamples complete crossover blocks within each order stratum; its 95% interval is conditional on this host, session, timer and protocol. Two cycles give a deliberately small initial sample; the interval does not capture between-session variability. Use independent repeat sessions for performance claims. Identical control fixtures and the retained correctness runs at 0/1/2 iterations expose call/loop timer floors diagnostically; these are never subtracted, and correctness timings are not a calibrated overhead estimate.

The default timer is `CLOCK_MONOTONIC` nanoseconds with compiler barriers around the single call. It includes call/return and timer boundary overhead. A separate wall duration accompanies TSC diagnostics. On x86_64, `--timer tsc` requires CPUID RDTSCP and invariant-TSC support, uses `lfence; rdtscp; lfence` plus compiler barriers at both endpoints and rejects changed TSC_AUX. Ticks are **not actual core cycles**. Equal endpoint TSC_AUX cannot exclude a migration away and back. ARM supports monotonic only; TSC requests are unsupported. There is no hardcoded counter frequency, PMU access, Apple M5 model or hardware cycle claim.

Dependent chains measure latency under that dependency graph. Independent accumulators approximate throughput subject to loop/control overhead. Memory patterns include real loads, address generation, cache state and dependencies. These are microexperiments: whole-application correctness, code-size and performance validation remain separate work.

# Targeted and full tests

Run the Rust suite through `tools/test.py` from any directory. It uses the
`test-fast` Cargo profile, with optimization level 2, limited debug information,
debug assertions and overflow checks. Plain `cargo test` retains its original
unoptimized profile and includes every nonignored matrix.

```sh
python3 tools/test.py                         # All workspace Rust tests and doctests
python3 tools/test.py frontend --quick       # Frontend and downstream consumer tests
python3 tools/test.py assembler              # Assembler and downstream consumer tests
python3 tools/test.py decoder --direct       # Decoder package only
python3 tools/test.py --changed main --quick # Changed packages and affected consumers
python3 tools/test.py --dry-run              # Print packages and commands without building
```

Other component names are `optimizer` and `codegen`. Component selections include
transitive workspace consumers, including dev dependencies. This can select much
of the workspace for shared IR or assembler changes. `--direct` deliberately
limits a named selection to its component packages. It is useful during local
iteration, but does not validate consumers.

`--changed BASE` includes differences between BASE and the current working tree,
including staged, unstaged and untracked files. A source change selects its
package and consumers. A top-level integration test source selects its package. Shared test helpers
and fixtures select the entire workspace because several crates include each
other's test inputs without Cargo dependency edges. Root manifests, generator changes, unknown paths and deleted packages
select the entire workspace. This is a conservative file/dependency selection,
not execution coverage or proof that unselected code cannot be affected.

## Quick and exhaustive coverage

`--quick` skips test names containing `exhaustive_` and skips doctests. The five largest
codegen matrices have separate boundary smoke tests:

- String port I/O: all 288 recipes, both raw and optimized modules, baseline with
  normal registers and full selection/allocation with empty pools, at V1.
- Legacy and AVX strings: both narrow and wide fixtures, baseline/fast with
  registers and full/full with empty pools.
- D8 ABI: every fixture and feature policy, baseline/fast with normal registers
  and full/full with empty pools, each checked against Clang.
- NEON modified immediates: every record and shape, immediate values
  0, 1, 127, 128 and 255, baseline/full selectors and normal/spill pools, including
  Clang byte comparisons and native execution on macOS ARM64.

Full runs retain all original cases, selector/allocator/pool combinations,
independent assembler comparisons and host-conditional native execution.
Exhaustive native string tests are also excluded by `--quick`. Other tests
remain selected; quick is a coverage selection, not a fixed time budget.
Existing ignored tests stay ignored. Privileged guest qualification and external
corpus/native campaigns remain separate from this Rust runner.

Test preparation reuses verified parsed modules and lazily optimized modules
across allocator/pool combinations. String port I/O checks literal bytes once
per distinct instruction and checks whole-object parity for every configuration.
Smoke tests do not write the exhaustive export files.

## Parallel execution

Install `cargo-nextest` to run test binaries in parallel:

```sh
cargo install cargo-nextest --locked
python3 tools/test.py --runner nextest
```

The runner selects nextest when available and otherwise uses Cargo. Override
with `--runner cargo` to compare behavior. Nextest runs at most four tests at
once; the codegen group is limited to two. These bounds limit concurrent compiler
processes and large modules. Tests use process-specific temporary paths. The
runner invokes Cargo doctests separately after a full nextest run, even if a
regular test failed. Nextest writes JUnit results under `target/nextest/default/`.

Both runners use the same Cargo profile. To compare with unoptimized tests:

```sh
python3 tools/test.py codegen --direct --runner cargo --profile test
```

For a single test target, Cargo remains sufficient:

```sh
cargo test --profile test-fast -p keel-codegen --test x86_string_port_io
cargo test --profile test-fast -p keel-codegen --test x86_string_port_io source_matrix_boundary_smoke
```

Tests comparing external GNU assembly require `KEEL_GNU_AS` to point to the
appropriate assembler. Tool availability and host architecture still determine
which native checks can execute. Shared export directories should belong to one
run; do not run separate Cargo or nextest invocations against the same exports.

## Timing and runner checks

```sh
python3 tools/test.py --report target/test-reports/full.json
python3 -m unittest discover -s tools/tests -p test_test_runner.py
```

The JSON report records selected packages, quick/full coverage, commands, elapsed
wall time and exit codes. It does not infer test counts. Build time is included;
compare warm execution separately before attributing a change to test code.
A full runner exit succeeds only when all commands, including doctests, succeed.

## Measurements

Measured on the same macOS ARM64 host. Execution times exclude Cargo compilation.
The five-binary comparison uses the preceding full-run log as its baseline;
string port I/O was also rerun immediately before this change.

| Check | Unoptimized/original | Optimized/refactored |
| --- | ---: | ---: |
| Exhaustive string port I/O, 144 configurations | 189.84 s | 16.60 s |
| Five slowest binaries, ordinary Cargo | 650.00 s | 114.45 s |
| Python tooling tests | 154.61 s, 16 tests | 0.18 s, 17 tests |

The five Rust binaries include five additional smoke tests after the change.
Python tooling retains all samples and adds a check against catalogue reloads.
Its instruction-metrics helper now derives a sample from the already loaded
instruction instead of reloading all 1,670 ISA records per sample. Catalogue
rows, provenance and hashes match the previous implementation exactly.

These are elapsed observations, not isolated microbenchmarks. Optimization,
fixture reuse and repeated-work removal contribute together. External Clang,
linker and native-oracle work remains. Cold-build costs were not compared;
the optimized profile uses separate artifacts and may increase first-build time.

The full nextest run after this change passed 2,796 tests across 500 binaries in
258.04 seconds, with 13 existing ignored tests. The separate doctest also passed.
The preceding full run spent 1,472.69 seconds executing tests. The combined
changes reduced observed test execution time by about 5.7 times. Including the
79-second build and doctests, the new invocation took 339.19 seconds.

Validation also passed the workspace all-targets/all-features check, both
Cargo and nextest quick decoder invocations, generator freshness, 99 x86
Python tests, four AArch64 Python tests and 17 tooling Python tests (one optional
CLI integration skipped). Scoped Clippy completed with existing warnings in
untouched compiler code and two unchanged expressions in the AVX string test;
the new preparation helper produced no diagnostics.

Generator checks remain separate from the Rust runner:

```sh
python3 -m unittest discover -s tools/x86-isa
python3 -m unittest discover -s crates/keel-asm/tools -p test_generate.py
python3 tools/x86-isa/generate.py --check
python3 crates/keel-asm/tools/generate.py --check
```

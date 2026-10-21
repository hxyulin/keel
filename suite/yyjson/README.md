# yyjson application workload

Pinned upstream: [yyjson 0.13.0 official release](https://github.com/ibireme/yyjson/releases/tag/0.13.0).
The source archive is downloaded from the upstream repository and checked against
the SHA-256 in `suite.toml`. Its header declares version 0.13.0. No upstream
source edits, feature-disabling defines, CPU tuning, or other nondefault upstream
flags are used. The build uses C11 and links libm.

`corpus.py` deterministically generates three inputs and independent expected
semantic digests using Python's JSON parser:

* `api`: 1,024 user profiles and variable-length event lists, UTF-8, escaped
  control characters, embedded NUL, booleans, nulls and large integer IDs.
* `config`: 160 service configurations, pretty whitespace, escaped Unicode,
  24 nested levels, empty keys/containers and signed/unsigned 64-bit boundaries.
* `telemetry`: 4,096 numeric records, exact fractional measurements, missing
  samples and floating-point boundaries including subnormal values and -0.0.

The C driver loads one input once, then repeatedly parses through yyjson's strict
DOM API, traverses every value/key, writes alternating compact/pretty JSON,
reparses it, and checks semantic equality. Its endian-independent FNV-1a digest
includes type markers, lengths, integer sign/magnitude and double bit patterns;
node and string-byte counts are also checked. Allocations and document frees are
inside each iteration. Input generation is outside the workload. File loading
and process startup remain inside any future CLI measurements.

`check.py` verifies all three digests against Python, eight valid edge documents,
and eleven strict rejections (including invalid UTF-8, surrogate escapes,
trailing commas, comments and trailing values). Checks do not use C assertions
that can disappear in release builds. It prints deterministic JSON lines so
the suite's Clang comparison can also check identical output.

The standard suite follows the repository's shared Clang preprocessing route:

```sh
cargo suite -j 2 test yyjson -c clang-O0,clang-O2,keel,keel-fast,keel-full --release
```

Run builds and checks under `suite/fast_opt_locked.py` when sharing a host with
the benchmark campaign. Correctness is not an upstream exhaustive test suite;
mutable DOM, JSON5, incremental parsing, custom allocators and file APIs are not
exercised. The workload makes no performance claim.

## Native C validation

`validate.py` additionally compiles the original `.c` files directly through
Keel's own preprocessor/frontend and native object emitter. It uses a fresh
output directory, two concurrent translation units, and retains exact argv,
stdout, stderr, source/compiler/binary hashes and failures in `report.json`.
Clang supplies the independent O0/O2 builds and the common system linker.
Keel None uses baseline selection; Fast/Full use Full selection; all use Full
allocation. The helper does not run performance measurements. Python 3.9 or
newer, Clang, Cargo and the host SDK/libc headers are prerequisites.

For example, from the repository root (create `target/yyjson-validation` first):

```sh
python3 suite/fast_opt_locked.py target/yyjson-validation/lock.jsonl \
  cargo build --offline --locked --release -j 2 -p keel -p keel-suite \
  --target-dir target/yyjson-validation/cargo
python3 suite/fast_opt_locked.py target/yyjson-validation/lock.jsonl \
  python3 suite/yyjson/validate.py --out target/yyjson-validation/native \
  --keel target/yyjson-validation/cargo/release/keel
python3 suite/fast_opt_locked.py target/yyjson-validation/lock.jsonl \
  cargo test --offline --locked --release -j 2 -p keel-suite \
  --target-dir target/yyjson-validation/cargo manifest::tests::registry_loads
```

`--archive PATH` reuses a downloaded archive after checking its hash. The
validation directory must not already exist. Generated reports are local
artifacts, not tracked source files.

On Apple AArch64/macOS, compiler source base `0f6fdd7f` passed Clang O0/O2 and
native Keel None/Fast/Full: all three corpus checks, eight valid edge documents,
eleven rejection checks, and one untimed invocation of each manifest workload
at its configured iteration count. The suite registry/schema test also passed.
No compatibility fallback was needed. This is local correctness coverage;
Linux and performance campaigns remain unmeasured.

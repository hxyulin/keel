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

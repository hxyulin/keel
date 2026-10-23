# C system-header compatibility: LinuxHost reductions

This bounded investigation starts at main `1a2417f7` and retains the rejection
of unsupported extended floating types. The change improves `_Float32`
diagnostics; it does **not** unblock the practical application corpus.

## Frozen failures and provenance

Original evidence is `/tmp/keel-compile-profile-bfa86ed7/`, including
`linuxhost-evidence.tar.gz`, `provenance.json`, and `gcc-pre/preprocess.json` in the
tar. New inputs, reductions, exact commands, output, hashes, objects and native
executables are outside Git at `/tmp/keel-c-header-20261023/` on LinuxHost. The
investigation scripts and copied JSON are also retained locally under
`/tmp/keel-c-system-headers-evidence/`.

LinuxHost is native x86-64 Linux with glibc 2.44. The frozen system preprocessors
are `/usr/bin/clang` 23.1.1 and `/usr/bin/gcc` 16.2.1 20260810. SSH's current
default `gcc` instead resolves to a Nix GCC 16.2.0 and different header paths;
that initial comparison is recorded separately in `probe.json`. The final
`final-probe.json` uses `/usr/bin/gcc` and reproduces the retained GCC outputs
byte for byte. No package, header, application or compiler configuration changed.

Representative inputs are QuickJS 2024-01-13 `libregexp.c`, Lua 5.4.7 `lvm.c`
and SQLite 3.46.1 `sqlite3.c`. The frozen GCC preprocessing commands run in
their original application source directories:

```sh
/usr/bin/gcc -E -std=gnu11 -D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"' \
  -DCONFIG_BIGNUM -I. libregexp.c -o /tmp/libregexp.i
/usr/bin/gcc -E -std=gnu99 -DLUA_USE_LINUX lvm.c -o /tmp/lvm.i
/usr/bin/gcc -E -std=c11 -DSQLITE_THREADSAFE=1 -D_GNU_SOURCE \
  -DLONGDOUBLE_TYPE=double sqlite3.c -o /tmp/sqlite3.i
```

The regenerated output SHA-256 values, respectively, are:

- `c47f60de637b7785053ac2f582c66163b78f35fe6d454673a5daa790a00912e6`
- `542ebc728cf60e8464e7732d5b399148d3d6899519dc2486c82c659fa1815c27`
- `0c4d913e97a762ab0b3e67e55303169e767b7c7a7b5e7a5844c9da68abe3fb85`

The equivalent Clang preprocessing uses the same standard, definitions and
include flags, and the original absolute source paths used by `keel-suite`.
Regenerated `libregexp.i` matches the retained Clang file at SHA-256
`629a619529f16d533df7e599c71321e80953c78402d9e03926e8e96b556d89f0`.
The retained Clang files for all three TUs reproduce the same first failure.

Compilation throughout uses explicit
`cc -O --opt-tier none --target x86_64-unknown-linux-gnu --cpu-level v1
INPUT --emit-object -o OUTPUT`. This preserves release legality enforcement
and makes no target-feature override. Both compiler binaries in the smoke
comparison use the ordinary debug Rust build profile, not performance builds.

## `__TC__`: a real complex binary128 contract

The entire reduced declaration is:

```c
typedef _Complex float C __attribute__((__mode__(__TC__)));
```

Both independent preprocessors preserve `__TC__` literally. It is GNU's
complex binary128 machine mode, not an unexpanded macro. The original source
is `/usr/include/bits/floatn.h:83`; glibc selects this typedef for compilers
without native `_Complex _Float128` syntax, including Clang. Keel reports
`unsupported integer mode '__TC__'` (with backticks in the actual diagnostic).
The general macro expansion/hiding/pasting machinery is not implicated.

Native Clang and GCC both compile, link and execute a reduced defined program
checking 32-byte size, 16-byte alignment, identity distinct from complex float
and complex double, and `(1+2i)*(3+4i) == -5+10i` in this type. Keel's existing
complex float/double support and opaque `__float128` declaration placeholder
do not implement those semantics. Ignoring the mode would change layout and
values, even if the typedef were unused in a particular TU.

## `_Float32`: GCC builtin versus Clang header typedef

The original QuickJS/SQLite GCC failure at `/usr/include/stdlib.h:145` reduces
to:

```c
extern _Float32 strtof32(const char *restrict, char **restrict);
```

Before this change, implicit-int parsing gave the misleading diagnostic
`expected ';', found 'strtof32'`. Now an unbound `_Float32` in a declaration
or type-name context reports `unsupported extended floating type '_Float32'`.
Existing ordinary identifiers with this spelling and explicit header typedefs
retain their lookup behavior; no builtin type or implicit alias is introduced.

Native GCC confirms `_Float32` has size/alignment 4/4 but is incompatible with
both float and double. A defined program using `f32` literals, an `_Float32`
function boundary and addition links and executes successfully with GCC.
Clang rejects the reduced builtin declaration as an unknown type. Conversely,
the Clang glibc path supplies `typedef float _Float32`, which Keel continues to
accept with exactly that typedef's float identity, arithmetic and ABI. GCC
rejects that typedef and its float-identity assertion; it cannot serve as a
source-level substitute for the GCC builtin.

The Lua GCC TU first encounters a different member of the missing family:
`extern int __fpclassifyf128 (_Float128 __value);`. Its retained first
diagnostic remains `expected ')', found '__value'`. The `_Float32` diagnostic
change does not claim to diagnose or support every extended floating spelling.

## Original `.c` behavior and bounded validation

Keel's own `-E` succeeds on each unmodified original `.c`, using the original
definitions/includes. Full compilation of all three still rejects the glibc
`__TC__` declaration, at line 83, column 54. Keel's pinned Clang-compatible
identity macros make glibc select this path too. Foreign `.i` assumptions
therefore explain the difference between the GCC and Clang failures, but do
not account for the entire own-source compatibility gap. No identity or feature
macro was faked to suppress the declaration. Keel has a fixed C11 predefined
profile; the original foreign preprocessor standards remain recorded above.

`system_header_compat` checks unsupported declaration/type-name diagnostics,
preprocessor preservation of `__TC__`, header typedef identity/layout,
arithmetic/conversion IR verification, compatible and incompatible
redeclarations, nested typedef scopes and ordinary identifier shadowing on
all three supported targets. Target-loop checks are frontend/IR checks, not
execution on those targets.

A separate native Keel control compiles two functions using the header-style
float typedef and links each Clang/GCC harness through ordinary float arguments,
return and float-to-double conversion. Both executables pass. The resulting
x86 ELF object has 109 bytes of text; disassembly is in `final-probe.json`.
Three warm-process compile-time smoke samples under the host lock measured
2.06–2.60 ms before and 2.03–2.63 ms after (debug compiler, tiny control).
These are smoke observations without a threshold or performance claim, and
are separate from generated-code inspection and native correctness checks.
Practical TUs never reach object generation, so no application link,
correctness or runtime-performance result is claimed.

Validation on 2026-10-23:

- Full local `cargo test --offline --locked -j2 -p keel-c`: **259 passed**.
- Native LinuxHost `system_header_compat`, `gnu_types`, `preprocess`: **33 passed**.
- Local `cargo clippy --offline --locked -j2 -p keel-c --tests`: completed with
  existing repository warnings; none names the new regression file.
- Native full-suite attempt stops in
  `atomic_representations::scalar_builtin_families_and_writable_buffers_match_clang`:
  Clang 23 accepts float `__atomic_load_n`, contrary to that test's expected
  rejection. The separate `complex::raw_platform_complex_header` oracle also
  exits unsuccessfully. Both failures reproduce in a clean `1a2417f7` source
  snapshot on LinuxHost; neither is repaired or counted as a passing full native
  suite here. Seven other complex tests pass in the focused attempt.

Every build/test/measurement command uses `suite/fast_opt_locked.py` with the
shared host lock, `CARGO_CACHE_LINKS=0` for Rust compilation, default shared cargo
cache and separate managed checkout targets. For example:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-header-tests.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

## Smallest staged follow-up

1. Specify and implement a distinct `_Float32` scalar identity through type
   compatibility, generic selection, arithmetic rank, casts, literals and
   diagnostics. Deliberately specify usual/default promotions and x86 SysV
   argument/result/varargs behavior; sharing F32 storage/IR requires preserving
   frontend identity. Validate finite values and boundary transport against GCC.
   This alone will not accept the other `_FloatN` declarations in these headers.
2. Separately specify real binary128 values and ABI, including conversions,
   constants and the necessary compiler-runtime helpers; the existing opaque
   declaration placeholder is insufficient. Add complex binary128 layout,
   operations and ABI only after that real-type contract exists. Native
   Clang/GCC cross-call and arithmetic oracles must cover `mode(TC)` itself.
3. Re-run the frozen and original-source inputs after each stage to expose the
   next genuine boundary, without removing prototypes or changing headers.

Neither stage implies arbitrary `_FloatN`, complex/tgmath, vendor-header,
automatic-vectorization or extended floating-environment completeness.

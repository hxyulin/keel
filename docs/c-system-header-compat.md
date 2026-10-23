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

## Earlier staged follow-up

The declaration-only investigation below refines the next implementation step;
these value-support stages remain separate future work.

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

## Declaration-only boundary investigation at 8bb21e9a

This read-only production-code investigation starts from clean main
`8bb21e9a17620184bb3571ae19221ddc673f1281`. No parser, lowering, backend,
application, system header or predefined macro was changed. The new files are
native oracle/reproducer fixtures, not newly passing Keel compatibility tests.
The proposed implementation below is **not implemented**.

### Reproduction and exact unchanged-input blockers

LinuxHost evidence is `/tmp/keel-header-boundary-8bb21e9a/`: `source.tar` is a
`git archive` of the base; `src/` is its independent build checkout;
`investigate.py`, `commands.json`, `census.json`, `metadata.json`, `fixtures.json`, per-TU
`*-provenance.json`, `build.jsonl`, `probe-lock.jsonl` and `test.jsonl` retain
commands, source hashes, output hashes, diagnostics and lock observations.
The compiler SHA-256 is
`ca65de66fe32c6a8037bb5d9373c8ca0f566dacdb774dd04721d15d742b2821b`.
Local copies of the investigation script and JSON are at the same `/tmp` path.
The earlier evidence directories above remain untouched.

The three original application sources match the retained source SHA-256
values. Regenerated GCC `.i` files match all three frozen hashes above exactly;
Clang `.i` files match the prior retained files exactly. Use `/usr/bin/gcc`
16.2.1 and `/usr/bin/clang` 23.1.1 explicitly. GCC's original relative input
spelling matters for line-marker hashes; Clang uses the retained absolute
source paths. Flags and working directories come from the retained provenance.
The SQLite `LONGDOUBLE_TYPE=double` flag is part of that original configuration,
not a new workaround. Keel raw-source runs retain its existing C11 identity.

There are **12 failed full object-compilation attempts**: three unchanged raw
sources, plus each TU preprocessed by GCC, Clang and Keel. Every invocation
uses `cc -O --opt-tier none --target x86_64-unknown-linux-gnu --cpu-level v1`;
release legality enforcement remains active. All nine preprocessing operations
succeed. No application object, application link or application execution
succeeds or is inferred from preprocessing.

- All three raw sources, all three Keel `.i` files and all three Clang `.i`
  files stop at the one `__TC__` typedef per TU in `bits/floatn.h:83`.
- GCC `libregexp.i` and `sqlite3.i` stop at `_Float32` in `stdlib.h:145`.
- GCC `lvm.i` stops at `_Float128` in `mathcalls-helper-functions.h:20`.

These are exact first-blocker counts, **not a count of all latent errors**.
Because this investigation does not implement or bypass the first error, an
exact count of post-fix failures cannot be measured. No declarations were
deleted to manufacture an application pass.

The token census of the unmodified `.i` files excludes line markers and
literals. In `_Float32`, `_Float64`, `_Float32x`, `_Float64x`, `_Float128`
order, GCC counts are respectively:

- QuickJS `libregexp`: **3, 3, 3, 3, 3** (15 occurrences).
- Lua `lvm`: **0, 0, 0, 0, 8** (8 occurrences).
- SQLite `sqlite3`: **455, 467, 460, 472, 487** (2341 occurrences).

Each corresponding Clang and Keel input has exactly one additional occurrence
of each spelling (the five header typedefs), one `__float128` and one `__TC__`.
All occurrences belong to system-header line-marker regions, not application
bodies. QuickJS's 15 header sites are three groups of five declarations in
`stdlib.h`; Lua's eight tokens occur on seven helper-declaration lines.
SQLite has 640 lines containing family tokens in `stdlib.h`, `math.h`,
`bits/mathcalls.h`, `bits/mathcalls-narrow.h` and
`bits/mathcalls-helper-functions.h`. Token/line counts are deliberately not
reported as distinct declarations, unsupported calls or independent errors.

Thus GCC inputs need five distinct builtin identities for QuickJS/SQLite and
one for Lua even to cover this declaration inventory. The Clang/Keel header
path already supplies its own five typedefs. Correct `TC` declaration support
is the smaller candidate for that path; adding only GCC `_Float32` still
cannot cover its other four missing spellings. There are no `TC` value uses
in these three retained TUs. This makes a declaration-only payoff plausible,
not demonstrated.

### Native identity, layout and ABI evidence

[GCC's additional floating types documentation](https://gcc.gnu.org/onlinedocs/gcc/Floating-Types.html)
specifies `TC` for complex binary128 and the x86 `__float128`/`_Float128`
alias. [GCC's C type description](https://gcc.gnu.org/gcc-7/changes.html)
states that the `_FloatN`/`_FloatNx` identities remain distinct from ordinary C
floating types even for equal encodings. On LinuxHost, native GCC confirms:
`_Float32` 4/4, `_Float64` and `_Float32x` 8/8, `_Float64x` 16/16 with x87
encoding, and `_Float128` 16/16 with binary128 encoding. `_Float64` and
`_Float32x` are distinct; `_Float64x` is distinct from `long double`;
`_Float128` is compatible with `__float128`; `TC` is compatible with
`_Complex _Float128`. Clang's header typedef aliases have the identities the
headers actually declare. They do not establish GCC builtin compatibility.

The maintained `extended-float-layout-oracle.c` fixture compiles, links and
executes with **both native system compilers** (exit zero). It checks `TC`
size/alignment **32/16**, array stride 32, real/imaginary binary128 order,
16-byte member alignment, offsets 16 and 48 in a char/TC/char wrapper, total
size 64 and sentinel bytes around a 32-byte copy. This is finite native
layout evidence, not Keel arithmetic or ABI execution. The GCC-only identity
assertions use actual builtin types; no substitute typedef is introduced.

The [AMD64 psABI classification](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
classifies real binary128 using SSE/SSEUP and complex binary128 as a pair of
those real fields. The 32-byte pair becomes MEMORY after aggregate cleanup;
it must not inherit complex-float/double register transport or
`COMPLEX_X87` return rules. No call ABI is granted by knowing its layout.

An independent existing defect makes this distinction concrete:
`extended-float-aggregate-abi.c` defines
`struct box { __float128 x; };` and returns the second of two by-value inputs.
Current Keel emits a real ELF object without a diagnostic, using integer
argument/result registers. Both native GCC and native Clang callers linked
against that object exit **1**. Each compiler's own object with the same
harness exits **0**. The distinct inputs avoid the false positive of an
identity function leaving incoming XMM0 unchanged. Keel disassembly and both
oracle assembly files are retained. This is an observed ABI failure, not an
inference from a parse-only probe. It remains unfixed in this research change.

### Frontend architecture and safety gaps

- `types.rs`: `Ty::Opaque` stores only name and size, compares structural
  identity and reports `unusable()` only for the immediate type. `Types::align`
  defaults to size: naively adding a 32-byte opaque `TC` gives wrong alignment.
  Existing `_Alignof(_Complex long double)==16` also fails Keel's static
  assertion while both native oracles accept it. Size is insufficient for
  layout and never establishes a numeric format or ABI class.
- `parse.rs`: attribute storage/parser currently reduce integer modes to a
  byte width and `apply_type_attrs` requires an integer subject. `TC` needs a
  typed mode selector and a distinct complex-binary128 identity; ignoring it
  or retaining the original complex-float type changes size and meaning.
  Definition-object rejection only unwraps arrays, not record fields.
- `types.rs` compatibility and `parse.rs` redeclaration checks need a numeric
  identity predicate independent of value support. Current
  `extern __float128 object; extern double object;` incorrectly compiles;
  both native compilers reject the conflict. Adding a non-arithmetic opaque
  identity alone would reproduce this hole.
- `irgen.rs`: `abi_param`, `convert` and scalar places have immediate-type
  guards. `aggregate_layout` can fall back to integer chunks for opaque
  fields; `contains_complex` recognizes extended complex only by the existing
  `_Complex long double` name. `complex_abi_layout` calls `elem.ir()`, which
  cannot represent binary128. Generalizing `Ty::Complex` to opaque elements
  without guards risks a panic as well as wrong ABI/footprints.
- Existing unused `__float128` typedef/prototype and real-size/alignment
  reductions compile fully to objects. Direct/indirect scalar calls, value
  loads, casts and direct object definitions diagnose. A discarded static
  inline body containing an unsupported call can escape IR-generation checks;
  a discarded empty definition with an unsupported result does too. The latter
  is only a diagnostic-timing probe, never executed (it has no defined result).
  Fail-closed validation for the new boundary must run before body pruning.
- `debug.rs` and constant/initializer paths (`eval.rs`, `init.rs`) need audit:
  preserve identity/layout without inventing binary128 constants or debug
  encodings. Never round a binary128 initializer through host `f64`.

### One proposed bounded implementation

Implement **x86-64 Linux binary128/TC declaration and layout support with
explicit value/ABI rejection**, using the existing `__float128` declaration
model as the starting point. Do not add GCC `_Float32`/other builtin value
support in this step. Do not change identity macros, include paths, ISA flags,
application flags or headers.

Give extended real/complex types canonical semantic identity, explicit size
and alignment, and a separate value-support capability. For this target, `TC`
is complex binary128 (32/16), and its real component is the existing GNU
binary128 identity (16/16). Header typedefs retain their declared identity.
Accept typedefs, unused prototypes/function-pointer types, extern object
declarations, pointers and non-evaluating layout/type queries, including
record/array layout. Initially reject materialized extended values and objects
(including containing records/arrays), initialization, arithmetic, comparisons,
casts, real/imaginary extraction, scalar/aggregate calls/results, definitions
and varargs involving those values. Ordinary data-pointer transport remains legal;
materializing an unsupported function address must also diagnose if doing so
requires an ABI signature the frontend cannot represent safely.
Do not publish extended-FP feature support based on this declaration subset.

The same small step must add recursive unsupported-value containment and
numeric redeclaration checks. Recursion traverses arrays, records, unions and
atomic wrappers, stops at pointer boundaries, and tolerates recursive records.
Use it before by-value ABI layout and before pruning unsupported bodies.
Otherwise completing this type exposes the independently reproduced hazards.
No new F128 IR type, software arithmetic helper or backend instruction is
needed for the accepted subset. Reject GCC extended builtin spellings rather
than aliasing them; `_Float32` default promotions and arithmetic rank remain a
separate semantic project (it does not get float's default double promotion;
see [GCC's implementation rationale](https://gcc.gnu.org/pipermail/gcc-patches/2016-August/455079.html)).

Expected files: `types.rs` (identity/layout/recursive capability), `parse.rs`
(mode subject and declaration/definition/redeclaration gates), `expr.rs`
(unevaluated queries versus forbidden operations), `irgen.rs` (recursive ABI
guard), and `debug.rs` as needed. Audit `eval.rs`/`init.rs`; add regressions in
`system_header_compat.rs`, native fixtures and this document. Own no instcombine
or AVX256 files. Keep other targets' unsupported behavior explicit; a Linux
x86 oracle is not AArch64 or Darwin ABI validation.

Acceptance gates for that follow-up:

1. GCC/Clang oracle agreement on identity, 16/16 and 32/16 layout, containing
   records/unions/arrays, offsets and exact byte footprints. Keel IR verification
   and object emission for accepted queries/prototypes/pointer controls, with
   and without debug info; no unsupported scalar IR or runtime helpers.
2. Negative tests for every value category above, direct and indirect calls,
   function pointers, variadics, returned/nested aggregates, all storage
   durations, conflicting redeclarations and discarded inline/static bodies.
   The maintained aggregate reproducer must diagnose instead of emitting the
   currently failing object. Do not count the native oracle executable as a
   Keel value/ABI pass.
3. Re-run the exact unchanged three raw sources and all nine frozen `.i`
   compilations at explicit v1, with legality checks active. Record the next
   diagnostic or actual object success separately per input. GCC `.i` support
   is not an acceptance promise of this bounded step. Only after successful
   full compilation attempt normal native link/execution for each project;
   a TU is not a whole application. Preserve provenance and report remaining
   counts instead of adjusting inputs.
4. Run focused and full `keel-c` tests with the required locked cargo command;
   distinguish the known native-suite oracle limitations documented above.
   Keep release legality regression checks and exact access-width tests active.

Application payoff: this targets the shared first barrier on all three raw
sources and all three Clang-preprocessed TUs. The inventory shows no application
use of these extended types, so declaration support may enable useful progress
without arithmetic/backend work. **Zero additional application objects or
executables are demonstrated here**. Later frontend, lowering, encoding, linker
or runtime failures remain unknown until the follow-up is implemented.

### Validation and retained commands

The unmodified base compiler built on LinuxHost using the default debug profile:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-header-boundary-8bb21e9a/build.jsonl \
  cargo build --offline --locked -j2 -p keel
python3 suite/fast_opt_locked.py /tmp/keel-header-boundary-8bb21e9a/test.jsonl \
  cargo test --offline --locked -j2 -p keel-c --test system_header_compat
```

The focused suite passed **4 tests**. Native probes also ran under
`suite/fast_opt_locked.py` and the shared host lock. There are no timing or
performance conclusions. No Rust toolchain, compiler wrapper, shared target
directory, native-link cache setting or build output was replaced/deleted.

To reproduce the native layout control from the repository root on LinuxHost,
place all outputs in a private `/tmp` directory and run each command through
the shared lock (replace `OUT` with that directory):

```sh
/usr/bin/gcc -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o OUT/layout-gcc
/usr/bin/clang -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o OUT/layout-clang
OUT/layout-gcc
OUT/layout-clang
```

For the ABI reproducer, compile `extended-float-aggregate-abi.c` with the Keel
flags above, then link `extended-float-aggregate-harness.c` against its object
using each native compiler. Both executables currently return 1. Compile the
same function fixture with each native compiler for the two passing controls.
These fixtures intentionally expose unsupported behavior; they are not
automatically registered as passing Keel tests.

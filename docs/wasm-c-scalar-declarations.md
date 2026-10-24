# Private C scalar declaration checkpoint

Based on reviewed main `4f9f49c0`, following the
[C layout foundation](wasm-c-layout-foundation.md) and
[integer evaluator](wasm-c-integer-evaluation.md) checkpoints. This completes a
bounded parser/type follow-up from the [layout plan](wasm32-layout-plan.md).
Production remains LP64; `CDataLayout::WASM32` remains `cfg(test)`. No public
Target, Options, macro, header, CLI, IR, backend or lockfile changes are included.
Native ABI, FP, memory, vector and architectural intrinsic contracts are unchanged.

## Implemented boundary

GNU integer mode parsing now resolves `word` and `pointer` to the stored model's
pointer width, independently of fixed-width `DI`. Applying the resulting type
attribute selects long for eight-byte modes only when long is eight bytes;
otherwise it selects long long. Thus WASM32 DI is nominal long long,
word/pointer are int, and native DI/word/pointer remain long. Original signedness
is retained, including unsigned long long / unsigned int in the test model.
Underscore spellings, attribute positions and numeric conflict checks retain the
existing rules. Duplicate DI/word attributes coincide on LP64 and conflict on
WASM32. QI/byte, HI, SI and TI retain their existing nominal identities.

The private `Parser::declarations_with_layout(src, target, text, layout)` test
entry constructs a parser, selects the model, and invokes translation-unit
parsing. This builds the actual struct table with the model before expression,
size/alignment, array-bound, type-identity, offset and initializer queries run.
The `target` argument is a borrowed native parser profile, not a Wasm target or
ABI. The resulting AST must never enter the unchanged native generator. No
sibling IR/backend interface changed: this uses the existing private CDataLayout
and Types queries, with no dependency on the sibling IR DataLayout API.

String initializer element widths now use `size_with_layout`. Four-byte storage
alone cannot establish wide-character identity: WASM32 accepts int arrays for
wide strings and rejects long/unsigned-int arrays, as the local Clang oracle
requires. The test model also rejects _Bool arrays initialized by narrow
strings rather than treating one-byte storage as character identity. Wide literal
AST elements are int under the test model even with a borrowed Linux AArch64
profile; LP64 keeps its existing target-based element
selection and permissive unsigned-wide initializer acceptance. Narrow strings,
inferred array lengths and existing truncation/terminator policies remain intact.
The maintained oracle's exactly sized wide array omits only the terminator;
this does not establish acceptance of excess non-terminator characters in ISO C.

## Explicit native fences and remaining work

Vector type construction and scalar broadcast entry reject the non-native
model. Both native constant proofs now receive the model and return None for
non-native input before consulting native width/range/evaluator helpers. The
x86 builtin families reject the test model before argument handling. Native
proof validity rules and architecture behavior are unchanged. Native ABI
classifiers retain their existing LP64 guards; inline assembly, asm linker
labels, va_list, long double declarations and TC mode now also reject the test
model instead of borrowing native layout/ABI behavior. Packed records and
bit-fields retain their existing non-native rejection.

Remaining no-argument integer-size / Ty-IR uses in the owned files are classified:

- `expr.rs` broadcast and x86 immediate proof widths and native evaluator calls
  sit behind explicit model guards. Vector shape/range queries stay native.
- `expr.rs`'s two 16-byte integer cast guards identify i128 and are width-invariant
  across these models. The LP64 `usual` wrapper remains for native generation.
- `parse.rs`'s old-style parameter integer acceptance check (`<= 8`) is
  width-invariant across these models. Its vector lane `Ty::ir()` query is behind
  the native vector-type gate. Complex storage queries use the stored Types.
- `init.rs` has no remaining native integer-width queries. Scalar initializer
  conversion, aggregate member offsets and array strides use the parser model.

Initializer *parsing* and initializer *serialization* are separate checkpoints.
The tests inspect converted scalar expressions, their evaluated values, inferred
array types and flattened AST offsets. String `InitItem::Bytes` is the existing
literal byte buffer, tested here for lengths and placement only. Numeric object
bytes, pointer relocation widths/addends, zero filling, data alignment and cursor
advancement still belong to the unchanged native `irgen.rs::data` path. Passing
these ASTs to it would mix models. Scalar SSA/ABI widths, pointer lowering,
standalone generator folds/proofs and debug widths also remain unported. Array
object placement alignment, runtime VLA support, macros/headers, Wasm ABI and
memory/runtime support require later gated checkpoints. No full translation-unit
Wasm compilation, FP support, serialization or execution is claimed.

## Validation

Five new private unit tests cover:

- A maintained declaration fixture parsed in LP64/WASM32/LP64 order within one
  arena: nominal typedef/literal/conversion identities, real struct/union layouts,
  nested array strides, nested offsetof, literal-dependent bounds, designated
  initializer offsets and model-normalized long values, string array completion.
- GNU mode signedness, underscore spellings, declaration-specifier/final
  attributes, object/function/parameter identities, duplicate attributes and
  incompatible redeclaration diagnostics.
- Native-contract rejection through the declaration parser, wide-string nominal
  identity and borrowed-target wide-element isolation.
- Independent Clang wasm32 and native syntax/layout/type-identity assertions using
  the same maintained fixture, without headers or preprocessing.
- Explicit non-native rejection at both proof helpers and the broadcast entry,
  with positive LP64 constant controls. The full existing GNU/vector/intrinsic
  suites remain the native behavior gate.

The maintained fixture is
[`wasm-scalar-declarations.c`](../crates/keel-c/tests/fixtures/wasm-scalar-declarations.c).
Its Clang wasm32 command is recorded at the top; the unit test also invokes it
and a native control. Clang is an existing required test tool, not installed by
this checkpoint. Generated logs and build artifacts remain outside Git.

All Rust compilation uses the pinned project setup, existing shared lock,
default cargo cache and `CARGO_CACHE_LINKS=0`:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

Focused release controls add:

```sh
--release --lib --test gnu_types --test gnu_vector_abi \
--test gnu_vector_broadcast --test generic_overflow --test x86_intrinsics \
--test x86_shifts --test x86_aes --test x86_saturation --test x86_conversions \
--test pclmul --test initializers --test run --test binary128_layout \
--test atomic_representations --test complex --test system_header_compat
```

Parent owns independent review and integration. No remote hosts, main integration,
subagents, cleanup or performance campaigns are included.

Implementation `92be6b9f` passed locally on 2026-10-24, Apple ARM64 with
Apple Clang 21.0.0: 297 full C debug tests and 113 focused release tests,
zero failures/ignored and no compiler warnings. Both configurations include
all five new private tests and both Clang declaration oracle profiles.
Applicable native runtime/ABI controls passed; Linux/x86-only runtime branches
do not execute on this host, so cross-object checks are not x86 runtime evidence.
Focused rustfmt checks on the three changed Rust files and `git diff --check`
passed. Final logs and serialized command records are outside Git:

- `/tmp/keel-c-parser-followup-final-debug-20261024.{log,jsonl}`
- `/tmp/keel-c-parser-followup-release-20261024.{log,jsonl}`

The first full debug run also passed 297 tests; it preceded the final private
_Bool string-identity guard, so the final-source run above is the acceptance run.
The release run serialized behind that run using the existing shared lock.
No Wasm C execution or performance claim follows from these results.

## Independent review

Candidate `256e8d86` (implementation `92be6b9f`) was reapplied onto clean
`4f9f49c0` in the assigned review checkout as `0234794b`/`2c274b57`.
The audit found no blocking defect in the agreed private parser-width checkpoint;
no production source fix was needed. Review test commit `2a3578e2` expands the
maintained Clang/parser fixture with an explicitly aligned recursive record,
anonymous union arrays, inferred aggregate array lengths and nested braced wide
and narrow string initialization. Actual AST offsets, record/array size and
alignment, recursive pointer identity and literal byte lengths are checked in
WASM32/LP64/WASM32 order. The fixture independently asserts the corresponding
C layout decisions in both Clang profiles.

Additional regressions check coincident word/pointer underscore aliases,
all three atomic declaration qualifier forms, zero-width and union bit-fields,
member packing and pragma packing. Rejection tests require the intended
boundary diagnostic. Native constructor controls cover every existing Target
and confirm LP64 with a fresh record table after private-model parsing.
The existing tests retain DI signedness/nominal identity, conflicting modes,
wide-string element identity, native proof and ABI/vector/asm fences.

Production `compile` still invokes only `Parser::new`, which always selects
LP64; the alternate constructor and WASM32 constant are both `cfg(test)` in
private modules. The constructor's initial token fill does not construct types
or records before the model is set. Native mode mapping, string width/type
selection, proof logic and fences take their previous LP64 branches. There is
no public non-native selection, sibling interface change or generator handoff.

The private seam calls the ordinary translation-unit parser, which can also
build function-body ASTs. Its declaration-only purpose is a caller precondition,
not a restricted C grammar or complete Wasm validation pass. Generic FP/complex,
opaque declarations, variadics, runtime expressions and other unported value
paths still need the future production target's complete validation gates.
Selective rejection here does not establish those gates. The AST/type/record
model pairing likewise remains a trusted private precondition. These limits are
safe only while the non-native model stays test-only and never enters native
IR generation. Numeric/pointer initializer serialization, generator lowering,
macro/header policy, array object placement and Wasm ABI/runtime remain deferred;
no serialization, executable Wasm C or general FP support follows from this review.

Independent Apple Clang 21.0.0 wasm32 syntax checks also passed seven string
controls: int wide arrays, signed/unsigned char narrow arrays and an exactly
sized wide array are accepted; long/unsigned-int wide arrays and _Bool narrow
arrays are rejected with `-Werror`. Exact inputs, arguments and diagnostics are
outside Git in `/tmp/keel-declarations-review-6f2d-clang-strings.json`.

Fresh review validation of `2a3578e2` on Apple ARM64 passed the full C debug
suite (298 tests) and the focused release command listed above (114 tests),
zero failures/ignored and no compiler warnings. Both include the expanded
wasm32/native Clang fixture and private rejection tests. Applicable native
runtime/ABI controls executed; local Linux/x86 frontend/object checks remain
cross-compilation evidence rather than x86 execution. Production
`cargo check --offline --locked -j2 -p keel-c` also passed. All three commands used
`python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl
cargo ...`, the existing shared lock, toolchain, features and default
cargo cache/managed targets. Logs and exact serialized invocations are
`/tmp/keel-declarations-review-6f2d-{debug,release,check}.{log,jsonl}`.
Focused rustfmt checks on the three candidate source files and `git diff --check`
passed. No remote hosts, subagents, toolchain/cache changes, manual output
deletion, performance campaign, main integration or worktree cleanup were
performed. Parent retains integration and cleanup ownership.

# Private scalar C-to-IR width checkpoint

Based on `aa326b52`, following the reviewed C layout, integer evaluation and
scalar declaration checkpoints. This completes the permitted integer-function
stage of the [layout plan](wasm32-layout-plan.md). Production remains LP64.
There is no public Target, CLI, Options, macro/header, IR, backend, dependency
or lockfile change. Native ABI, FP, memory, vector and intrinsic contracts remain
unchanged. Parent owns independent review, backend joining and integration.

The test-only API is
`irgen::wasm_scalar::lower(src: &str, debug: bool) -> Result<Module, Error>`.
Both its module and `CDataLayout::WASM32` remain `cfg(test)` in private modules.
Input is header-free/preprocessed C, parsed with the model selected before the
translation unit. The borrowed native parser target establishes neither Wasm
predefines nor a calling convention. No optimizer runs. The returned module has
no target tag and must never be passed to a native backend.

The implementation explicitly bypasses native ABI classification for this test
profile. Signatures use only normal i32/i64 parameters and at most one normal
integer result (or void), no extensions, prototypes required, no variadics. IR's
ordinary C calling-convention spelling records this bounded value interface;
it does not establish native C interoperability or a complete Wasm C ABI.
Direct calls use the same signatures. Undefined function declarations can be
represented for IR checking, but do not authorize backend imports or execution.

Accepted values are integer types through 64 bits, including narrow locals,
casts and promotions, assignment/compound assignment, arithmetic/comparisons,
conditions, ordinary loops/switch/goto, direct calls and scalar local
initializers. SSA and memory integer types, loads/stores, defaults and numeric
casts use the parser model: long is i32, long long is i64. Existing Types queries
supply scalar slot size/alignment; a volatile long uses 4/4. Local alignment
above 16 is rejected to match the sibling runtime's initial static-frame limit.
No C address-taking or pointer local is admitted. Internal stack addresses used
for scalar storage do not establish C pointer-expression support.

Generator folds, identity elimination, inert-expression checks and truth-wrapper
proofs recursively receive the same model. In particular, a long-long-to-long
cast cannot be stripped from a condition under ILP32. Folded long constants
truncate/sign-normalize and unsigned-long masks normalize at 32 bits. Both
usual-conversion generator queries and scalar Ty/IntKind shape queries now use
the stored model. The now-unused private native `usual` and `normalize` wrappers
were removed; native evaluator/proof compatibility entries remain.

Before Gen's worklist and dead pruning, a test-profile validator walks **all**
function declarations, definitions, locals and supported statement/expression
children. It rejects pointer values/casts, indirect calls, static/external data,
symbolic/aggregate/byte initializers, aggregate/array/atomic locals, FP/complex,
vectors, i128, all builtins (including architectural intrinsics, atomics, memory
and stack builtins), VLA/VM, label addresses/computed goto, assembly, cleanup,
nonordinary function/linkage metadata, debug output and narrow/pointer/aggregate
signatures. Existing parser native-layout gates may reject earlier. Scalar
compound literals and statement expressions are deliberately excluded. Ordinary
record declarations/type queries can still parse; this is a value-lowering gate,
not a new restricted declaration grammar. Existing parser constant/type queries
may erase unevaluated source expressions; the generator gate validates the
remaining typed tree, including unused bodies and unselected runtime branches.

## Explicit remaining blockers

This stage intentionally emits no Module.data, DataItem::Ptr, symbolic address
or C pointer value. It therefore uses the existing `verify_module` on actual
unoptimized output: every stored object is an integer, whose width does not
differ between IR storage layouts. The actual C long widths are already i32.
When data/pointers are admitted, use the parent's forthcoming
`verify_module_with_layout(&module, DataLayout::WASM32)` on the real module;
never substitute a widened initializer for verification.

The next private checkpoint must jointly migrate and test:

- Pointer locals/spills to four-byte storage/alignment, integer-offset casts,
  unsigned pointer normalization and comparisons, i64 offset extension, and
  pointer difference division in i64 **before** signed-long truncation. Current
  `scaled` and PtrDiff runtime paths are not admitted; changing their scalar
  shape queries alone does not migrate their operations.
- `data()` evaluator and scalar numeric serialization, numeric pointer bytes,
  symbolic pointer-only relocations, bounds/overlap checks, four-byte width,
  alignment and cursor advance together. Negative/unrepresentable addends,
  integer symbolic addresses, function pointers and external data need rejection.
  The existing native serializer and computed-goto tables retain native
  evaluator and explicit eight-byte assumptions behind this profile's fences.
- Arrays/records/strings and compound literals, including array object placement
  alignment and local byte initializers. Scalar volatile slots here do not
  establish aggregate memory/value support.
- Narrow/pointer call signatures and ABI extension, backend unsupported scalar
  operations (for example narrow IncDec arithmetic), imports and external
  function policy, actual runtime frames/restoration/exhaustion and execution.
  This frontend checkpoint validates IR semantics, not backend acceptance of
  every emitted operation. There is no cross-crate backend dev dependency.
- Overflow/vector/atomic builtin native size/evaluator queries, native debug
  widths and debug-only 8/8 pointer slot, ABI classifiers, FP contracts,
  preprocessing/header isolation, production target routing and optimizer audit.

No partial migration above authorizes relaxing the private gate. All remaining
no-argument IntKind width queries in irgen are the width-invariant i128 guard
or builtin paths excluded by preflight. Native evaluator calls remain in excluded
data/dispatch/atomic/intrinsic paths. Types::size and FnGen::size already consult
the stored model. There is no complete width-migration or executable C claim.

## Validation

Three private tests check actual unoptimized modules with the IR verifier,
plain signature types/metadata, calls, 4/4 long slots, truncation/sign extension,
unsigned shifts/division, model-specific mixed arithmetic, folded constants,
mask identity and the long truncation truth proof. A maintained header-free
[fixture](../crates/keel-c/tests/fixtures/wasm-scalar-lowering.c) also passes
Clang wasm32 and native declaration/type assertions. Existing full C tests
supply native runtime/ABI/parity controls. Rejection tests exercise unused
bodies and declarations, constant-false branches and alignment above 16.

All compilation uses the existing toolchain, shared lock, default cargo-managed
cache/targets and `CARGO_CACHE_LINKS=0`, with generated evidence outside Git:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

Focused release validation adds `--release --lib --test gnu_types
--test gnu_vector_abi --test gnu_vector_broadcast --test generic_overflow
--test x86_intrinsics --test x86_shifts --test x86_aes --test x86_saturation
--test x86_conversions --test pclmul --test initializers --test run
--test binary128_layout --test atomic_representations --test complex
--test system_header_compat`. No remote hosts, subagents, toolchain/cache changes,
output deletion, performance campaign, main integration or cleanup are included.

On 2026-10-24, Apple ARM64 with Apple Clang 21, the complete C debug suite
passed 301 tests and the focused release suite passed 117 tests, zero
failures/ignored and no compiler warnings. Both include all three new private
tests and Clang wasm32/native oracle checks. Applicable native runtime/ABI
controls executed; Linux/x86-only runtime branches remain cross-object evidence,
not local x86 execution. Logs and serialized invocations are
`/tmp/keel-c-scalar-{debug,release}-20261024.{log,jsonl}`. The focused initial
unit run exposed a text-assertion spelling error (binary operations print
without type suffixes); the final suites include the corrected assertions.
No C-to-Wasm execution or performance claim follows from these checks.

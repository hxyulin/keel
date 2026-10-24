# Private C layout model checkpoint

Based on main `d0a912e7`, after the reviewed pointer-free backend. This implements
only the private Types/IntKind/Ty checkpoint permitted by the
[wasm32 layout plan](wasm32-layout-plan.md). It is not a wasm32 C compiler or a
complete stage 2 width migration. Native ABI, FP, memory, headers and intrinsic
contracts in [ISA status](userspace-isa-status.md),
[C x86 intrinsics](c-x86-intrinsics.md) and [GNU modes](gnu-vector-mode.md) remain
in force. No public target, Options field, macros or runtime path was added.

## Implemented seam

`types::CDataLayout` has private fields and validated constants `NATIVE_LP64`
and `WASM32`. The latter is compiled only for private unit tests. Queries
`pointer_bytes()` and `pointer_align()` use local constants, so this checkpoint
has no dependency on the sibling IR DataLayout API. The parent can join those
queries to `DataLayout::NATIVE64/WASM32` when that API is integrated.

`IntKind::size_with_layout` distinguishes four-byte wasm32 long from eight-byte
long long, preserving nominal identity, rank and signedness. `Ty::ir_with_layout`
uses that width for integer IR types; pointer IR identity stays `Type::Ptr`.
Existing `IntKind::size` and `Ty::ir` retain LP64 behavior. No host inference is
used. This model does not select char signedness, long-double format or call ABI.

`Types::new(structs, layout)` replaces the tuple constructor. Integer and pointer
size/alignment, recursive arrays/complex/atomic storage, and natural record
layout use its stored model. StructDef still stores completed explicit layout;
its table must have been computed with the same model. Types does not relayout
an existing table or infer its provenance. Parser construction always selects
`NATIVE_LP64`; generator queries already delegate to that parser's `types()`.
There is no parser constructor for the test model.

Native ABI classifiers are deliberately fenced: `abi_layout` and `aapcs_layout`
assert LP64; `complex_abi_layout` and `atomic_size` return None for the test
model. Storage size for an atomic type does not establish atomic support.
Non-native record layout rejects bit-fields, packed members and pragma-pack
before using the existing AAPCS64 algorithm. Ordinary explicit alignments still
use the existing layout algorithm. Array object placement alignment is separate
from natural type/member alignment and remains unfinished.

## Unfinished width migration

All unchanged no-argument IntKind size and Ty IR queries still mean LP64.
The following is the migration inventory, not a claim those uses are portable:

- `expr.rs`: usual arithmetic conversions, literal candidate lists and fallback,
  vector broadcast range/constant proofs, x86 immediate constant bounds, casts
  and pointer difference. Promotion by rank is width-invariant for these two
  models; mixed signed/unsigned usual conversions are not. Add long long
  candidates after long/unsigned long while preserving LP64 choices and the
  existing unsigned-long-long extension fallback. Required wasm32 cases include
  unsigned int + long -> unsigned long and unsigned long + long long -> long long.
- `eval.rs`: thread one model through eval/eval_with, eval_int, eval_constant,
  recursive address evaluation, fits_i64, casts and normalization. Integer
  normalization must truncate/sign-extend 32-bit long; numeric pointers must
  truncate to unsigned 32-bit bits. Preserve symbolic base/addend and relocation
  rejection rules. No evaluator helper has been migrated in this checkpoint.
- `parse.rs`: GNU DI/word/pointer remain native aliases. Split mode parsing so
  DI selects long long on wasm32 and word/pointer select int, retaining unsigned
  counterparts and underscore spellings. Vector lane shape selection still
  uses native Ty IR queries; atomics, va_list and long double need explicit gates.
- `irgen.rs`: scalar ABI parameters, SSA locals, loads/stores, casts, numeric
  conversions, overflow builtins and bit-field containers still use native
  queries. Standalone fold_int/constant proof helpers also need the model.
  Pointer spill slots, symbolic initializer width/alignment/cursor advancement,
  computed-goto tables and dispatch recognition contain explicit eight-byte
  assumptions. Ptrdiff must divide the i64 address difference before truncating
  to signed long; pointer casts must follow the plan's normalization rules.
  Complex component IR queries are width-invariant for supported float/double,
  but native complex ABI classification remains fenced.
- `init.rs::init_string`: the four-byte integer element test is width-sensitive
  (long joins int on wasm32); migrate it along with string initializer typing.
  The one-byte test is width-invariant. `parse/asm.rs` uses native scalar IR and
  register constraints; reject Wasm inline assembly before reaching them.
- `debug.rs`: integer and pointer debug widths remain native. Reject Wasm debug
  output before enabling a public target. Bit offsets and byte serialization
  shifts by eight are bits-per-byte operations, not pointer-width constants.
- `types.rs::complex_abi_layout` retains native scalar IR/Layout queries behind
  its LP64 guard. Other remaining Ty IR queries here are compatibility wrappers
  or tests. `size/align` recurse through the stored model. Natural record size
  arithmetic retains the existing unchecked overflow behavior; this checkpoint
  does not add a large-object acceptance guarantee.

Predefined macros, bundled header policy, host include isolation, target routing,
optimizer gating, declaration/value rejection, pointer initializers, software
stack/data placement and backend memory support remain separate plan stages.
Enabling WASM32 in production before the evaluator and lowering migration would
mix four-byte storage with native arithmetic and is explicitly unsupported.

## Validation

Private tests cover every integer kind under both models, native compatibility
wrappers, rank and nominal identity, long/long-long IR widths, pointer-containing
records, unions, nested arrays, and non-native ABI/atomic/packing rejection.
The existing full C regressions are the native behavior gate. These tests do not
execute wasm32 C or establish its call ABI or pointer/memory semantics.

Reproduce with the existing toolchain, shared lock and default cargo cache:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-layout-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

On 2026-10-24 the full command passed locally on Apple ARM64: 285 tests,
zero failures/ignored, including five private model tests. The final run's log is
`/tmp/keel-c-layout-20261024-c4359e04.log`; serialized command records are
`/tmp/keel-c-layout-20261024-c4359e04.jsonl`. An initial compile rejected a test
helper's unnecessarily static lifetime; the helper was made lifetime-generic
before the successful complete rerun. Focused rustfmt and git diff whitespace
checks pass. Workspace-wide `cargo fmt --all -- --check` reports pre-existing
formatting differences across unrelated files; those files were not changed.

Generated evidence stays outside Git. No performance claim is made. Independent
review is recorded below; parent integration remains outstanding.

## Independent checkpoint review

Candidate `40b8a6cb` (implementation `8f43be9a`) was reapplied onto `d0a912e7`
in a separate review checkout. Source review found no blocking native behavior
or public API defect within the approved private-model checkpoint. `types` is
not publicly exported, `Options` is unchanged, every parser constructor selects
LP64, and generator storage queries use that parser's `Types`. WASM32's only
constant is behind `cfg(test)`; no production target or selection path was added.

Review commit `6503b3cc` adds two regressions: alternating model queries over
shared interned type nodes, including array/atomic/pointer recursion; and an
explicitly aligned record with a long array and self-referential pointer,
checking offsets, trailing padding, aggregate array stride and traversal stopping
at the pointer. It also documents the compatibility wrappers and constructor
provenance precondition directly at the methods.

The dual APIs deliberately do not prevent every future model mix. In particular,
`IntKind::size` and `Ty::ir` remain native even beside a WASM32 `Types`, and
`Types::new` trusts completed `StructDef` sizes/alignments/offsets from its caller.
Reusing an LP64 record table with WASM32 would retain those LP64 record values.
These are migration constraints, not production wasm32 support: callers must
thread one model consistently and build the entire record table under it before
any production non-native selection is enabled. No provenance framework or
cross-model relayout was introduced. Existing unchecked large-layout arithmetic
and deferred evaluator/conversion/literal/mode/lowering work remain as listed
above. No IR/backend/CLI/Target files were changed by this review; the sibling
IR-layout review and parent integration are separate.

Review validation on Apple ARM64 with Apple Clang 21 used the prescribed
`python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl
cargo test --offline --locked -j2 -p keel-c` invocation, preserving
existing pins/features and the shared lock/cache. The focused release command
adds `--release --lib --test gnu_types --test gnu_vector_abi
--test gnu_vector_broadcast --test binary128_layout --test atomic_representations
--test complex --test initializers --test system_header_compat`: 66 tests passed,
zero failures/ignored. This includes the seven model tests and applicable native
Clang controls; Linux/x86-only runtime branches do not run on this host.
The unmodified candidate first passed all 285 debug tests; the final full debug
rerun including review regressions passed all 287, zero failures/ignored.
Logs and serialized invocation records use
`/tmp/keel-review-c-layout-fce03f1a-{debug,release,final-debug}.{log,jsonl}`.
Focused `rustfmt --edition 2024 --check crates/keel-c/src/types.rs` and
`git diff --check` pass. No Wasm C execution or performance claim follows.

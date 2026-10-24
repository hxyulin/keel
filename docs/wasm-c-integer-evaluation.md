# Private C integer expression/evaluator checkpoint

Based on `bc76d65c`, after the reviewed IR and C layout foundations. This is a
bounded follow-up to [C layout foundation](wasm-c-layout-foundation.md) and the
[wasm32 layout plan](wasm32-layout-plan.md). Production remains LP64. WASM32 is
still a private `cfg(test)` constant; there is no Target, CLI, public Options,
backend, IR, ABI, memory, header or lockfile change.

## Completed explicit path

`eval_int_with_layout`, `eval_with_layout`, `eval_constant_with_layout` and
`eval_with_binding_and_layout` take the existing `types::CDataLayout` by value.
Their recursive operands, address evaluation, hint arguments, casts, arithmetic,
normalization and `fits_i64_with_layout` all use that same argument. Recursion
never enters a native compatibility wrapper. The old `eval`, `eval_with`,
`eval_int`, `eval_constant` and `normalize` signatures retain native boundaries
for lowering/proof callers. The private `fits_i64` helper had no remaining
native callers and was replaced by its explicit-model counterpart.

`usual_with_layout` preserves integer promotion, nominal identity and rank:
long still ranks above int and below long long, even with 32-bit long.
Unsigned int plus long selects unsigned long on WASM32; unsigned long plus
long long selects signed long long. The `usual` compatibility wrapper still
selects LP64 for the unchanged generator.

Both integer literal parser paths and integer cast folding use `Parser::c_layout`.
Decimal, hexadecimal and U/L/LL suffix candidate lists now include long long
and unsigned long long after long and unsigned long where required. LP64
selects the same first fitting type as before; the existing unsigned-long-long
extension fallback for decimal values above signed 64-bit range remains.
`normalize_with_layout` truncates/sign-extends long32 and zero-extends unsigned
long32 into `Const::Int(i64)`. Long long remains 64 bits. Cast representability
checks distinguish unsigned long32 from unsigned long long64 when widening to
i128; this does not establish general i128 constant arithmetic support.

Parser arithmetic, comparisons, conditional expressions, cast folding and
constant-query sites in `expr.rs`/`parse.rs` use the stored model. This includes
array bounds, alignment expressions, enumerators/case constants, duplicate-case
normalization and builtin constant arguments. Every production parser still
constructs `NATIVE_LP64`. The unit-test `expression` helper sets the private
model only for one scalar expression, checks end-of-input, and never invokes
translation-unit compilation or IR generation.

Numeric pointer constants keep low 32 bits as an unsigned i64 value on WASM32,
including casts and pointer addition. Pointer difference divides the address
difference before normalizing the long result. Address recursion retains the
same model. Symbolic `Const::Addr(base, addend)` stays symbolic, and final-binding
truth checks remain unchanged. The existing LP64 eight-byte integer-address cast
behavior is preserved; non-native symbolic address-to-integer casts return None.
No integer relocation support, pointer memory access, object placement, function
pointer ABI or pointer lowering follows from these evaluator-only tests.

## Preserved boundaries and unfinished work

The general evaluator keeps its existing wrapping arithmetic, masked shifts,
and saturating floating-to-integer cast behavior. `eval_int` is not a new strict
ISO ICE validity checker. Division by zero returns None; short-circuit and
comma policies are unchanged. The x86 immediate-control proof still separately
rejects invalid shifts, signed overflow, division overflow and invalid FP casts.
Its recursive shape/bounds checks and its native evaluator entry stay LP64.
The vector broadcast proof likewise retains its native width/range queries,
64-bit limit and exclusion of pointer, i128, extended FP, comma and effectful
intermediates. No Wasm vector or intrinsic proof support is claimed.

Remaining work before a production non-native parser is safe:

- `expr.rs`: native vector broadcast range checks and immediate-control proofs;
  width-invariant 16-byte i128 guards remain no-argument queries deliberately.
- `parse.rs`: GNU mode aliases and vector lane IR shape queries remain native;
  the mode integer acceptance check (`<= 8`) is width-invariant for both models.
  No Wasm declaration/value rejection gates or header policy were added.
- `init.rs`: integer string-element width selection remains native. The lone
  integer initializer expression shortcut now uses the model, but that does not
  migrate initializer parsing/packing as a whole.
- `irgen.rs`: both `usual` calls, standalone `fold_int` and identity proofs,
  evaluator calls for static initialization/atomics/immediates, scalar widths,
  overflow builtin lowering, pointer differences and relocation serialization
  remain native. In particular `data()` still uses native `eval_with` and native
  relocation widths. No model-aware AST may be handed to that generator yet.
- Debug widths, ABI classifiers, GNU attributes, runtime VLA/pointer operations,
  extended FP/atomic support and all later target/backend gates retain the
  boundaries recorded in the foundation and layout plan.

## Validation

Four private unit tests cover edge literal candidate identity, long/long-long
rank and signedness, nested arithmetic/casts/conditionals, truncation and sign
normalization, unsigned division/right shifts, numeric pointer casts/addition/
difference, symbolic address rejection/final binding, i128 representability,
constant/comma/short-circuit safeguards and unchanged native proof rejection.
They alternate native and WASM32 models within one arena to detect hidden model
state. Existing GNU/vector/intrinsic and overflow regressions are the native gate.

All compilation uses the existing toolchain, shared lock and default cargo cache:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

On 2026-10-24 the full C debug suite passed 291 tests, zero failures/ignored,
and the focused release suite passed 80 tests, zero failures/ignored, on Apple
ARM64 with Apple Clang 21. The release command uses the same prefix and flags
above, adding:

```sh
--release --lib --test gnu_types --test gnu_vector_abi \
--test gnu_vector_broadcast --test generic_overflow --test x86_intrinsics \
--test x86_shifts --test x86_aes --test x86_saturation --test x86_conversions \
--test pclmul --test initializers --test run
```

This includes the native end-to-end arithmetic/overflow program harness and
applicable native GNU vector/ABI controls. Linux/x86-only runtime branches do
not execute on this host; their frontend, IR and cross-object checks pass.
The four new private tests also passed in a separate initial debug library run.
Locked `cargo check` passed; focused rustfmt and `git diff --check` pass. An initial
check reported an unused native `fits_i64` wrapper; removing that unneeded
private wrapper left the test builds without new warnings.

Tested implementation is `fbc9017c` (only source comments changed after the full
debug compilation began). Logs and serialized command records are
`/tmp/keel-c-integer-20261024-{debug,release}.{log,jsonl}`; the initial library
run and compile check use `/tmp/keel-c-eval-20261024-{unit,check}.{log,jsonl}`.
Generated logs and artifacts remain outside Git. No performance or Wasm C
execution claim is made. Parent owns independent review and integration.

# CPU-profile generation investigation

The profile implementation lives in
[`features.rs`](../../crates/keel-asm-ir/src/x86/features.rs).
Feature fields, CLI names, aliases, containment and names iteration already
come from `feature-fields.json`. Instruction-to-feature requirements already
come from the ISA catalogue and `feature-policy.json`. Profile membership,
modifier processing and dependency validation remain handwritten.

## Current contracts

| Constructor | Capability fields | `explicit` |
|---|---|---|
| `NONE` / `none()` | None | false |
| `legacy()` / default | `popcnt`, `sse41` | false |
| `for_level(V1)` | None | true |
| `for_level(V2)` | `cx16`, `lahf_sahf`, `popcnt`, `sse3`, `ssse3`, `sse41`, `sse42` | true |
| `for_level(V3)` | V2 plus `avx`, `avx2`, `bmi1`, `bmi2`, `f16c`, `fma`, `lzcnt`, `movbe`, `xsave`, `osxsave` | true |
| `all()` | V3 plus `aes`, `adx`, `sha`, `vaes`, `vpclmulqdq`, `clflush`, `pclmulqdq` | true |

These are Keel's current tracked fields. This table is not an external ISA
standard audit or an assertion that every baseline capability has its own bit.
`all()` enables all 24 current capability fields. It does not mean that Keel
supports all x86 instruction families or that a CPU supports these features.
There is no runtime CPU or operating-system detection in these constructors.

The following compatibility behavior matters when moving the code:

- `CpuLevel::parse` accepts exactly lowercase `v1`, `v2` and `v3`. The CLI
  handles `legacy` separately, selecting the default feature set.
- `explicit` distinguishes explicit v1 from `NONE`. Equality includes this
  field; `contains` and feature-name iteration intentionally ignore it.
- `is_empty()` means equality with `legacy()`, rather than zero enabled bits.
- `parse("none")` retains the legacy set. `with_features("none")` returns the
  input unchanged and bypasses validation, even if that input is invalid.
- Feature tokens apply in order. Enabling the exact name `avx` also enables
  `xsave` and `osxsave`. Disabling AVX does not disable those state bits.
- AVX2, F16C, FMA, VAES and VPCLMULQDQ require AVX. This check runs before
  the state-management check and determines the first reported error.
- AVX requires XSAVE and OSXSAVE only when `explicit` is true. Older implicit
  struct literals can omit those bits. Removing this exception is a policy
  change, not a generation refactor.
- CLI `--target-features` starts from the selected profile or legacy default;
  it does not modify the previous `--target-features` result. CLI option order
  remains an orchestration rule in `crates/keel/src/main.rs`.

## Proposed next increment

Generate only the profile constructors and level-name lookup first. An
explicit `cpu-profiles.json` can list ordered levels, additive parent profiles,
capability fields and the `explicit` value. Keep `legacy` and `all` named
policies. Resolve inheritance at generation time so emitted constructors stay
`const fn`, with no runtime lookup or allocation.

Validate unknown feature fields, duplicate names, inheritance cycles, level
order and monotonic V1/V2/V3 membership. Pin the current memberships and exact
parse errors independently before replacing Rust. Require an explicit update
to the `all` policy when adding a feature; do not silently enable every new
field merely because it exists in `feature-fields.json`.

Keep modifier order, the `none` shortcut, AVX side effects, dependency checks
and compatibility exceptions handwritten in that first increment. They amount
to two validation rules and one ordered side effect. Moving them into a generic
expression language would add more machinery than it removes. They can later
be represented by narrowly typed policy records if more rules create repeated
code. Such records must retain evaluation order and exact error strings.

Do not infer profile membership from the imported ISA labels. Those labels
classify instruction forms; choosing default capabilities and operating-system
state requirements remains explicit Keel policy. Existing optional AES, ADX,
SHA, VAES and PCLMUL policies should not acquire profile membership merely
because their encodings are imported.

This AT&T generation increment leaves `features.rs`, all profile constructors,
feature dependencies and the CLI policy unchanged.

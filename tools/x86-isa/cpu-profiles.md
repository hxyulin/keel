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

## Replacement proposal without legacy compatibility

The user removed legacy compatibility as a requirement after the AT&T
checkpoint. The current-contract notes above describe migration inputs, not
constraints on the replacement. Generate both constructors and dependency
handling in one coherent feature model.

### Feature sets and target selection

Make `Features::default()`, `Features::NONE`, `Features::none()` and the v1
profile describe the same baseline capability set. Remove `legacy()`, the
`legacy` CLI profile and the `explicit` field. Define `is_empty()` as no optional
capability fields enabled. Keep named v1/v2/v3 profiles; do not expose v4 until
the EVEX/AVX-512 assembler, decoder and compiler support can honor it.

Keep whether the user supplied x86 options in CLI configuration, separate from
the resulting feature set. For example, an optional parsed x86 configuration
can distinguish no x86 options from an explicitly requested v1 profile even
though both resolve to zero optional bits. Reject x86-specific options on ARM
from that configuration rather than from feature equality or emptiness.
The library `FeatureSelector` should validate its actual capability set for
its target; an empty set need not pretend to be a selected x86 CPU profile.

Keep the current public Boolean capability fields unless a separate need for
a packed representation appears. Removing compatibility does not require
rewriting every backend capability query.

### Generated schemas

Use two data sources with separate ownership:

- Extend `feature-fields.json` with stable fingerprint bit IDs and explicit
  prerequisite lists. It already owns field names and CLI aliases. Generate
  names, containment, literal emptiness, fingerprints, prerequisite closure
  and strict validation from these records.
- Add `cpu-profiles.json` with the default profile, level names, additive
  inheritance and member features. Generate `CpuLevel`, lookup, constructor
  methods and profile documentation. Resolve inheritance at generation time.

Start with the dependency graph already enforced by Keel: AVX2, F16C, FMA,
VAES and VPCLMULQDQ require AVX; AVX requires XSAVE and OSXSAVE. Apply the
state prerequisites to every feature set. Remove the implicit-target exception.
Do not infer additional dependencies from similarly named extensions or from
which CPU happened to introduce them together. Instruction-specific VAES and
PCLMUL requirements remain in the existing form/feature policy.

Generate a finite set of graph operations, not a general expression language.
Reject unknown fields, duplicate bit IDs, cycles, duplicate profile names,
unknown parents, profiles missing prerequisites and non-cumulative levels.
Keep an explicitly named `all_known()` constructor for tests and tooling.
It should include every declared capability and validate successfully, but
should not be offered as a promise of host support or a default CLI target.

### Feature modifiers and CLI resolution

Parse profile selection and all feature modifiers first, then resolve once.
Default the x86 profile to v1. Apply repeated `--target-features` arguments in
source order to the chosen profile, regardless of where `--cpu-level` appears.
Reject repeated `--cpu-level` rather than silently resetting previous edits.
This shares one resolution implementation between compiler and assembler CLI.

An empty modifier source is a validated no-op. The standalone `none` modifier
clears all optional capability bits; it does not retain defaults or bypass
validation. Reject `none` inside a comma list to keep the reset unambiguous.
`Features::parse` starts from the empty baseline, not POPCNT/SSE4.1 defaults.

Enabling a feature adds its prerequisite closure. Disabling a feature clears
that bit, without silently removing dependents. Validate the completed set
and report missing prerequisites. Modifiers remain ordered: a later positive
edit may re-enable a prerequisite that an earlier edit disabled. For example,
`+avx2` enables AVX state prerequisites, while `+avx2,-avx` is an error unless
a later modifier restores AVX. Publicly constructed sets must also pass the
same strict validation before assembly or code generation.

### Standards and execution state

The [x86-64 psABI source](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
was checked on 2026-10-25. Its v2/v3 extension lists agree with Keel's current
tracked profile additions. Keel additionally tracks XSAVE as an AVX state
prerequisite. The specification makes the levels cumulative and requires
v3/v4 execution state to be enabled, including XCR0 checks. Keep profile
membership explicit, rather than deriving it from the instruction catalogue.

A compile-time feature set is a contract about the target environment.
Generated validation does not perform CPUID/XGETBV, configure CR4/XCR0, grant
CPL0 or establish a kernel entry ABI. A runtime detector would be a separate
component. No runtime detector is needed for this generation refactor.

### Concrete callers to migrate

- `crates/keel-asm-ir/src/x86/features.rs`: remove legacy/default exceptions;
  replace constructors, modifier application and dependency checks with
  generated methods. Update its four existing feature tests to the new rules.
- `crates/keel-codegen/src/x86_features.rs`: replace the handwritten feature
  fingerprint with the generated fingerprint; remove feature equality as a
  proxy for target-option provenance.
- `crates/keel/src/main.rs` and `crates/keel/src/asm.rs`: use one profile/modifier
  resolver and explicit option tracking. Update target-free optimizer and
  wrong-target checks, which currently depend on the special `is_empty()`.
- Default assembler entry points and the default instruction selector now
  target v1. Tests or callers needing POPCNT/SSE4.1 must request those features
  or a suitable named profile. Optional-feature struct literals must include
  their prerequisites or use the generated enabling API.
- `crates/keel-codegen/tests/x86_features.rs`: replace old fingerprint bit
  assertions and legacy-vs-explicit ARM behavior with the new contracts.

The fingerprint change fixes an observed omission: ADX, SHA, VAES and
VPCLMULQDQ are absent from the current handwritten fingerprint. A local probe
on this source confirmed that toggling each changes the capability set while
leaving its fingerprint unchanged. Generate the fingerprint from all fields,
excluding target-option provenance. Do not infer that this probe establishes
an existing optimizer miscompile; it demonstrates incomplete metadata.

### Validation plan

Replace compatibility pins for the intentionally removed behavior with tests
for the new contract. Check every declared feature's fingerprint bit, graph
closure, disabling prerequisites, cycles, profile inheritance and unknown
names. Check repeated CLI modifiers, option-order independence, explicit v1
on a non-x86 CLI target and target-free optimization. Exercise assembler and
codegen entry points with invalid manually constructed feature sets.

Run generator freshness, all feature and CLI tests, assembler/disassembler
packages, focused codegen feature tests and the full workspace once the
implementation is complete. This audit reran the existing four feature tests
and three codegen feature tests successfully. The probe and logs are local:
`/tmp/keel-cpu-profile-probe.rs`, `/tmp/keel-cpu-profile-probe`,
`/tmp/keel-cpu-profile-feature-tests.log` and
`/tmp/keel-cpu-profile-codegen-tests.log`.

This checkpoint changes the investigation and proposal only. Runtime profile,
CLI and feature behavior have not yet been changed.

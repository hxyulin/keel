# Generated CPU profiles

Keel's x86 feature sets describe the target contract. They do not detect host
CPU support, configure execution state or grant privilege.

## Sources and generated code

`feature-fields.json` owns capability fields, CLI aliases, stable fingerprint
bit IDs and direct prerequisites. `feature_fields.py` emits the shared Boolean
feature struct, names, containment, literal emptiness, complete fingerprints,
strict validation, all-known constructor and enabling prerequisite closure.
`cpu-profiles.json` owns profile names, the default and additive inheritance.
`cpu_profiles.py` resolves inheritance and emits `CpuLevel`, lookup and constant
profile constructors. Both run through `generate.py` and its freshness check.

The generator rejects unknown fields, duplicate bits, cycles, unknown profile
parents, missing prerequisites and non-cumulative level order. The instruction
feature policy is closed over the same prerequisites, so a requirement set can
also be passed as a valid assembler target. Optional instruction-specific AES,
VAES and PCLMUL requirements remain explicit in `feature-policy.json`.

## Profiles

The [profile membership table](cpu-profile-table.md) is generated from the same
schema as the constructors.

Baseline operations such as SSE2 have no optional feature bit. Empty means no
optional capability bits; it does not mean no x86 instructions. The former
legacy POPCNT/SSE4.1 default and `explicit` marker are removed. Callers needing
those operations must request the features or a suitable profile.
`Features::all_known()` enables every declared capability for tests and tooling.
It is not a host query or a CLI profile.

The [x86-64 psABI source](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
was checked on 2026-10-25. Its tracked v2/v3 additions agree with the profile
memberships above. Keel also tracks XSAVE as an AVX state prerequisite. The
standard requires cumulative levels and enabled vector state, including XCR0
checks for v3/v4. Keel exposes AVX-512F/VL assembler and decoder support, but
no v4 profile. Other required v4 families and compiler support remain outside
this implementation.
Compile-time validation does not perform CPUID/XGETBV or establish a CPL0 ABI.

## Modifier rules

`Features::parse` starts from v1. `with_features` modifies an existing set.
`with_feature_lists` applies ordered lists and validates the final result once.

- An empty source is a validated no-op. Standalone `none` clears every optional
  bit. `none` inside a comma list is rejected.
- Positive edits enable their prerequisite closure. For example, `+avx2` also
  enables AVX, XSAVE and OSXSAVE. AVX2, F16C, FMA, VAES and VPCLMULQDQ require
  AVX. Every AVX-enabled set requires both state-management bits.
- Negative edits clear only the named bit. They never silently disable
  dependents. `+avx2,-avx` fails validation. A later `+avx` restores validity.
- Manually constructed public structs must satisfy the same validation before
  assembly or code generation. Custom selectors cannot bypass this check.

Both CLIs use `crates/keel/src/x86_options.rs`. It collects one optional profile
and all feature lists, then resolves once after parsing. Profile flag position
does not reset modifiers. Repeated `--target-features` arguments accumulate in
order; repeated `--cpu-level` is rejected. No profile flag means v1.

Whether x86 options were supplied is stored separately from capabilities.
Thus `--cpu-level v1` and `--target-features none` are still x86-specific options
and are rejected for ARM targets, even though their resulting feature sets
are empty. Target-free optimization remains available when no x86 options
were supplied. The empty library feature set itself is not target provenance.

## Fingerprints and migration

Every capability contributes its declared bit to the generated fingerprint.
The codegen projection consumes that fingerprint rather than its former
handwritten list, which omitted ADX, SHA, VAES and VPCLMULQDQ. Target-option
provenance is not part of a capability fingerprint.

This intentionally changes defaults and modifier semantics. Library users
should replace `Features::all()` with `all_known()` for test inventories, remove
`legacy()` and `explicit`, and request optional capabilities explicitly.
Manually constructed AVX sets must include state prerequisites, or use the
positive-feature API. Old fingerprint bit positions are replaced by the schema
IDs; subsequent schema changes must preserve those assigned IDs.

Tests cover dependency closure, every fingerprint bit, invalid manually
constructed sets, profile membership, cycles, duplicate IDs, repeated CLI edits,
profile flag order, empty requests on foreign targets and target-free optimizer
behavior. Existing assembler and codegen suites retain instruction oracles and
feature-specific lowering checks. No legacy compatibility mode remains.

## Implementation validation

The workspace compile check, all 55 generator tests and both generator
freshness checks pass. The full workspace test run reports 2,705 passed,
one failed and 13 ignored across 495 reported groups. Its sole failure was
the low-load oracle expecting SSE4.1 from the former default. That fixture now
requests SSE4.1 explicitly; its complete target passes all three tests on the
subsequent rerun. This is combined workspace-and-rerun evidence, not a claim
that the recorded full-run command exited successfully.

Logs are retained locally at `/tmp/keel-generated-profiles-final-workspace.log`,
`/tmp/keel-generated-profiles-final-rerun.log` and
`/tmp/keel-generated-profiles-check.log`. No new CPL0 or native x86 execution
qualification is claimed by this profile-generation change.

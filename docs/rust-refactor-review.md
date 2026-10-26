# Rust review: generated instruction tables and their consumers

This review starts with the shared assembler-IR library and the native assembler
and disassembler. It also samples the C parser's state management. It is not a
whole-workspace safety audit. The workspace uses Rust 2024. Guidance comes from
[BeautifulRust](https://github.com/morganlinton/BeautifulRust), including its
Rust best-practices and review skills. The comparison baseline is `946e6c0a`.

## Implemented

`IsaForm::osz` now uses `OperandSizeRule` instead of an unrestricted string.
The assembler and disassembler share an exhaustive interpretation of all eight
imported rules. Unknown source rules fail generation instead of silently
receiving default prefix behavior. This changes the public Rust field's type;
downstream code constructing `IsaForm` must use the enum.

Both x86 catalogue generators now emit each distinct operand list once in
first-use order. The legacy/VEX table uses 371 nonempty lists; the EVEX table
uses 152. Instruction order, operand contents, import scope, feature policy and
decode indexes are unchanged. Normalizing the new representations back to the
old representation reproduced every one of the 5,394 form records and both
decode indexes exactly.

Measurements on the development ARM64 host:

| Measurement | Before | After |
| --- | ---: | ---: |
| Legacy/VEX generated source bytes | 1,928,015 | 1,448,019 |
| EVEX generated source bytes | 1,572,633 | 1,221,761 |
| Operand struct literals across both files | 12,958 | 1,277 |
| `size_of::<IsaForm>()` | 104 | 96 |
| `size_of::<EvexForm>()` | 184 | 176 |
| Distinct operand-slice payload bytes | 71,512 | 71,512 |

Generated source is 830,868 bytes smaller, a 23.7% reduction. The sum of form
record payload sizes is 43,152 bytes smaller. These are source and data-layout
measurements, not executable-size or throughput benchmarks. Rust already
shared identical operand slices before the refactor. Explicit sharing reduces
source repetition without claiming another runtime operand-memory saving.

Reproduce the layout measurement with:

```sh
cargo run -p keel-asm-ir --example isa_table_layout
```

## Must fix

No correctness or soundness defect was established by this limited review.
The generator's silent fallback for unknown operand-size rules is closed by
the implemented change.

## Should fix

### Simplify AArch64 predicates in the generator

Location: `crates/keel-asm/tools/generate.py:337`.

Rule: `clippy::eq_op` and `clippy::nonminimal_bool`.

The decoder generator emits feature-only expressions such as `(true) &&
(true)`. A broader Clippy run reaches the unchanged AArch64 output and fails
on 167 equal-expression diagnostics. Fold Boolean constants and duplicate
terms while generating predicates:

```python
left = condition(node['left'])
right = condition(node['right'])
if left == right:
    return left
```

Add the corresponding AND/OR identity and negation rules. Visit both operands
before simplifying, so an unknown AST or feature in a discarded branch still
fails validation. Preserve predicate masks and the exhaustive overlap checks.
Rust already optimizes these expressions; the benefit is cleaner generated
source, less frontend work and a usable lint baseline, not a proven decoder
throughput gain.

### Type operand roles and EVEX rules

Locations: `crates/keel-asm-ir/src/x86/isa.rs:20` and
`crates/keel-asm-ir/src/x86/evex.rs:60`.

Rule: BeautifulRust, "Model the domain with types" and "Make invalid states
unrepresentable."

Operand kinds and encodings still occupy string slices. EVEX functionality,
tuple type and mask policy do too. Consumers repeat string matches and may
accept an unrecognized value through a wildcard. Replace each closed domain
with an enum and reject unknown source values before writing Rust:

```rust
pub enum OperandEncoding {
    None,
    ModRmReg,
    ModRmRm,
    // Remaining imported encodings need explicit variants.
}
```

Do this next, across the generator and all consumers together. Keep mnemonic
names and diagnostic strings as strings. Measure layout again before choosing
any further packing or shared descriptor IDs.

### Replace prefix sentinels with explicit constraints

Location: `crates/keel-asm-ir/src/x86/isa.rs:43`.

Rule: BeautifulRust, "Option for absence, never sentinels" and "Data-carrying
enums instead of flag ints."

`prefix` combines ignored, forbidden-F3, no-prefix and required-prefix states
in an integer. `w`, `b` and `group1` use another sentinel convention. Replace
these with small constraint enums whose methods own the comparison logic:

```rust
pub enum BitConstraint {
    Any,
    Clear,
    Set,
}
```

The existing masks are actual sets of accepted values and can remain masks.
Independent instruction flags do not require an enum per flag.

## Consider

### Index AArch64 decoder candidates before extracting fields

Location: `crates/keel-disasm/src/lib.rs:80`.

Rule: BeautifulRust, "Performance, in order": measure before optimizing.

Every word scans all 1,670 imported records. Generate buckets using selected
fixed instruction bits, then apply the original masks and predicates to every
candidate. Records whose key bits are unconstrained must appear in every
compatible bucket. Preserve all matches and ambiguity reporting. Benchmark
candidate counts, throughput and index size before accepting this change.

```rust
let matches = candidates(word)
    .filter(|encoding| encoding.matches(word));
```

The current exhaustive overlapping-predicate test is a useful correctness
guard and should remain independent of bucket generation.

### Make function-only parser state explicit

Location: `crates/keel-c/src/parse.rs:869`.

Rule: BeautifulRust, "Make invalid states unrepresentable."

Function-only helpers repeatedly unwrap `self.fs`. These calls express parser
invariants; this review has not shown a user-reachable panic. Consider moving
function-only operations onto a borrowed function-parser context rather than
replacing every unwrap mechanically:

```rust
impl FuncState<'_> {
    fn push_block(&mut self) -> usize {
        let id = self.scopes.len();
        self.scopes.push(self.scope);
        self.scope = Some(id);
        id
    }
}
```

Start with a small family of operations. Preserve the existing arena ownership
and interned-name identity; changing those requires separate measurements.

## What is already good

- x86 encoding and decoding share one imported form catalogue with checked
  provenance and deterministic generation.
- x86 mnemonic lookup and opcode decoding already use generated indexes.
- Compiler semantics remain separate from instruction recognition and encoding.
- Literal byte witnesses and independent assembler oracles test observable
  results rather than only comparing two consumers of the same tables.

## Lint policy

The workspace has no shared lint table. The strict baseline run stops in
`keel-asm-ir` with 52 diagnostics, mostly mechanical style findings. Keep that
baseline distinct from new diagnostics when reviewing this refactor.
The post-change strict run reproduces those same messages. Running with
pedantic warnings without promoting warnings to errors reaches the unchanged
AArch64 output and stops on the 167 predicate errors described above.

Adopt a reviewed subset of
[BeautifulRust's lint configuration](https://github.com/morganlinton/BeautifulRust/blob/main/lints/lints.toml)
through `[workspace.lints]` and member inheritance after reducing the existing
baseline. `missing_debug_implementations`, `redundant_clone` and
`undocumented_unsafe_blocks` are useful starting checks. Blanket pedantic or
cast warnings need a compiler-specific review because instruction encoders
deliberately manipulate widths and bit patterns. String-domain modeling and
generated-table indexing still require design review; lint configuration
cannot substitute for either.

## Validation

- The full workspace run passed all 2,788 unit and integration tests, with
  13 ignored tests. It then failed in eight doctest targets with dependency
  artifact mismatches (`E0460`/`E0463`). An isolated
  `cargo test --workspace --doc --no-fail-fast` rebuilt dependencies and passed
  every doctest target, including the one executable doctest. This is combined
  evidence for 2,789 passing tests, not a single fully green workspace run.
- The prefix-rule test passed again after merging two identical match arms.
- `cargo check --workspace --all-targets --all-features` passed.
- All 99 x86 generator Python tests and both ISA generator freshness checks passed.
- Scoped Rust formatting and diff checks passed.
- Rustdoc built the three affected libraries, reporting three existing broken
  links in unchanged generated feature documentation.
- Strict Clippy remains blocked by the same 52 baseline diagnostics. The broader
  pedantic run also exposes the unchanged AArch64 predicate errors described above.

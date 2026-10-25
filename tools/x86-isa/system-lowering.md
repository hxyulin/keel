# Generating regular x86 system lowering

Investigation at `25ba19d3`. This is a proposal, not an implemented lowering
change. The scope is the 45 existing system intrinsics at IDs 774 through 818.

## Finding

Per-instruction bindings for 43 of the 45 intrinsics fit four small recipe
families. The backend already shares most of the emission logic. Generate the
bindings and dispatch, and keep a small shared emitter for each family. A new
instruction using an existing recipe should require a declaration and tests,
without another instruction-specific backend match arm.

| Recipe family | Current bindings | Existing mechanism |
|---|---:|---|
| Fixed instruction | 7 | CLTS, INVD, WBINVD, CLI, STI, HLT and SWAPGS in `cpu_control.rs`. |
| System-register transfer | 26 | Thirteen reads and thirteen writes, using `native_system_register` and `x86_system_register`. |
| Single operand | 9 | LGDT/LIDT; register and memory LLDT/LTR/LMSW; address-only INVLPG, using `native_system_write` and `x86_system_write`. |
| Adjacent fixed pair | 1 | STI;HLT, emitted without intervening allocation, spills or labels. |
| Explicit handwritten hook | 2 | RDMSR and WRMSR, retaining the existing fixed-register sequences initially. |

These sets are disjoint and cover all 45 declarations. The counts were checked
against `system-intrinsics.json` and the current helper mappings. No compiler,
assembler or runtime tests were run for this documentation-only investigation.

## Proposed declaration

Add a required tagged `lowering` object to each existing system declaration.
For example, the LGDT declaration would include:

```json
"lowering": {
  "kind": "unary",
  "mnemonic": "lgdt",
  "operand": "memory_read",
  "bits": 80,
  "argument": 0
}
```

The supported kinds should be a closed set, with no arbitrary Rust snippets:

- `fixed`: one existing zero-operand opcode, no parameters or results.
- `system_register`: an approved control/debug register and read/write direction.
  Reads have no parameters and one i64 result; writes take i64 and return nothing.
- `unary`: an opcode, operand width and explicit operand role. Roles distinguish
  scalar register input, memory read and address-only operands. INVLPG's m8
  syntax must not become a claim that it reads a byte. The IR compiler memory
  effect remains the separately declared conservative barrier.
- `adjacent_fixed`: the explicitly reviewed STI;HLT pair. Do not introduce a
  general sequence language for this one case.
- `handwritten`: a whitelisted existing handler for RDMSR or WRMSR.

Validate signatures, argument indices, register domains, operand widths and
public opcode bindings before generating output. Confirm that referenced
assembler adapters/forms exist; do not derive compiler semantics from them.
A declaration with no valid recipe or known hook must fail generation.

## Generated code and shared code

Generate a typed recipe lookup and dispatch in `keel-codegen`, which already
depends on both `keel-ir` and `keel-asm-ir`. Bind opcodes through the existing
public identities instead of repeating runtime `Opcode::parse(...).unwrap()`
choices. Resolve those identities from the existing adapter policy rather than
maintaining another list of instruction bytes.

Keep `keel-ir` independent of assembly transport types. Its existing public
`x86_system_register()` and `x86_system_write()` mappings can be generated from
the same declarations while preserving their current return shapes. The
backend can consume the typed recipes directly. Generate the matching dispatch
classifications so a new binding does not need another handwritten allowlist.

Keep shared execution of the recipes in ordinary Rust. Reuse `use_reg`,
`def_reg`, `def_done` and `emit`; these already handle allocated registers,
spills, rematerialization and text/typed/chunked transport. Do not generate a
separate copy of a full emitter for each register or mnemonic.

The adjacent-pair emitter must make only the two consecutive instruction
emissions between its boundaries. No operand materialization or result
publication may occur between STI and HLT. This preserves the current emission
contract; it does not establish the caller's interrupt environment.

## Boundaries

RDMSR returns EDX:EAX packed into i64. WRMSR captures its index in ECX, reconstructs
its i64 input in RAX, then splits its high half into EDX. Input homes must remain
unchanged. The existing forced-rematerialization test checks that rebuilding
RAX preserves the captured ECX index. Keep these sequences as explicit hooks
for the first rollout. A later fixed-register recipe needs reviewed ordering,
clobber and result-packing rules; register names alone are insufficient.

CPUID is outside the 45-row schema and also needs an RBX save slot and prologue
save/CFI planning. Its instruction emission cannot be migrated independently
of those frame requirements. Existing RDTSC/RDPMC/XGETBV sequences may later
share a reviewed EDX:EAX read recipe.

IRETQ and other owned entry/return adapters are outside ordinary returning
intrinsic recipes. Stack installation, terminal control flow, fault delivery
and asynchronous state ownership remain distinct contracts. Port I/O and x87
regions also need their own reviewed recipes rather than an arbitrary sequence
escape hatch.

The current metadata generator assumes an immutable 774-entry prefix and a
contiguous system tail; its total inventory length uses that assumption. This
is sufficient for these existing 45 rows. Before allowing unrelated intrinsic
families to interleave with new system IDs, separate global ID allocation from
family membership and inventory length. Its current scalar type vocabulary
also needs extension before vector intrinsic migration.

## Rollout and validation

1. Add and validate recipes for the existing 45 rows, including two explicit
   hooks. Preserve IDs, signatures, effects and existing helper APIs.
2. Generate recipe lookup, helper mappings and dispatch. Reuse shared emitters,
   remove the migrated per-instruction bindings and retain the MSR hooks.
3. Freeze complete text/typed/chunked outputs before migration. Compare fresh
   outputs afterward across selectors, allocators, normal/empty pools and the
   existing forced-rematerialization cases. No unknown recipe may silently
   reach an unimplemented backend fallback.
4. Retain independent literal/Clang/GNU witnesses, STI;HLT adjacency assertions,
   unused-read retention, foreign-target rejection and effect checks. Generator
   tests should reject mismatched signatures, invalid registers/opcodes,
   unsupported widths and unknown hooks.
5. Run freshness checks, generator tests, workspace compilation and workspace
   tests. If carrying prior CPL0 evidence forward by object identity, freshly
   export and compare every relevant object, including relocation-bearing
   contents. Changed payloads require renewed qualification for their contracts.
   Matching artifacts retain only the scope of their previous qualification.

This generates regular bindings from reviewed semantic policy. It does not add
instructions, infer privilege/state contracts, or complete baseline kernel
entry/return support by itself.

## Source locations

- `crates/keel-codegen/src/x86_64/cpu_control.rs`: fixed instructions, register
  transfers, MSR sequences and adjacent STI;HLT.
- `crates/keel-codegen/src/x86_64/descriptor_queries.rs`: single-operand emitter.
- `crates/keel-codegen/src/x86_64.rs`: dispatch, scratch registers, frame planning,
  allocation access and result publication.
- `crates/keel-ir/src/intrinsic.rs`: current register and single-operand mappings.
- `crates/keel-codegen/tests/x86_msr.rs`, `x86_interrupt_control.rs`,
  `x86_system_registers.rs` and `x86_system_write.rs`: existing regression checks.

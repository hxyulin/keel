# Generated regular x86 system lowering

The 45 system declarations at IDs 774 through 818 each contain a required
`lowering` recipe. `system_lowering.py` validates those bindings and emits typed
recipes for `keel-codegen`, plus the existing public helper mappings in
`keel-ir`. The backend dispatch uses the generated system-intrinsic pattern.
There is no separate per-instruction backend allowlist for these bindings.

| Recipe family | Bindings | Shared execution |
|---|---:|---|
| Fixed instruction | 7 | CLTS, INVD, WBINVD, CLI, STI, HLT and SWAPGS. |
| System-register transfer | 26 | Thirteen reads and thirteen writes using approved CR/DR registers. |
| Single operand | 9 | LGDT/LIDT; register and memory LLDT/LTR/LMSW; address-only INVLPG. |
| Adjacent fixed pair | 1 | STI;HLT, with no intervening allocation, spills or labels. |
| Explicit handwritten hook | 2 | Existing RDMSR and WRMSR fixed-register sequences. |

These sets are disjoint and cover all 45 declarations. The first four recipe
families supply 43 regular bindings. Shared emission remains ordinary Rust in
`crates/keel-codegen/src/x86_64/system_lowering.rs`; it is not copied once per
mnemonic or register.

## Declarations and validation

For example, the LGDT declaration includes:

```json
"lowering": {
  "kind": "unary",
  "mnemonic": "lgdt",
  "operand": "memory_read",
  "bits": 80,
  "argument": 0
}
```

Recipes form a closed set with no arbitrary Rust snippets:

- `fixed`: one existing zero-operand system/flags opcode, no parameters or
  results. The imported shape must have no GPR or stack operands; the existing
  hidden GS operand is supported. CPUID, MSR accesses and terminal returns do
  not fit this template.
- `system_register`: an approved control/debug register from `registers.json`
  and read/write direction. Reads have no parameters and one i64 result;
  writes take i64 and return nothing.
- `unary`: an existing static Core opcode, encoded width, sole argument index
  and explicit operand role. Roles distinguish scalar register input, memory
  read and address-only operands. A matching imported operand form must exist.
  INVLPG's m8 syntax is not a claim that it reads a byte. Its compiler memory
  effect remains the separately declared conservative barrier.
- `adjacent_fixed`: only the reviewed STI;HLT pair, with no parameters or
  results. This is not a general sequence language.
- `handwritten`: a whitelisted RDMSR or WRMSR hook with its exact signature.
  The generated recipe names its canonical handler intrinsic, so another
  declaration cannot silently fall through to an unknown MSR dispatch case.

The generator rejects unknown fields, kinds, hooks and registers; incorrect
signatures or argument indices; unsupported widths; missing opcode bindings;
and operand forms that the imported catalogue does not describe. Those checks
validate structural compatibility. The declarations are reviewed compiler
policy, not inferred architectural semantics or permission proofs.

## Generated and shared code

The generated recipe lookup uses typed public `Opcode`, `CoreOpcode`,
`FixedOpcode` and `Register` values. Regular emission no longer repeats
`Opcode::parse(...).unwrap()` or `Register::parse(...)` choices. Public opcode
bindings come from the existing identity policy, and register values come from
the register schema. Instruction bytes remain owned by the shared ISA tables.

`keel-codegen` already depends on both `keel-ir` and `keel-asm-ir`; this adds no
crate dependency. `keel-ir` stays independent of assembly transport types. Its
public `x86_system_register()` and `x86_system_write()` methods are generated
from the same recipes with their original return shapes and values.

The shared emitter reuses `use_reg`, `def_reg`, `def_done` and `emit` for
allocated registers, spills, rematerialization and text/typed/chunked transport.
The adjacent-pair arm makes exactly two consecutive instruction emissions.
There is no operand materialization, result publication or label emission
between STI and HLT. This preserves emission adjacency without establishing
the caller's interrupt environment.

## Boundaries

RDMSR returns EDX:EAX packed into i64. WRMSR captures its index in ECX,
reconstructs its i64 input in RAX, then splits its high half into EDX. Input
homes remain unchanged. The existing forced-rematerialization test checks
that rebuilding RAX preserves the captured ECX index. Both existing sequences
remain handwritten in `cpu_control.rs`. A later fixed-register recipe needs
reviewed ordering, clobber and result-packing rules; register names alone are
insufficient.

The older descriptor/status, selector and scalar port-I/O families now have
[generated metadata and public helper tables](../../docs/x86-query-bindings.md)
for 42 existing IDs. Their shared emission code has not migrated into
this recipe engine. The four wide LAR/LSL register
operand gaps found during that migration are now fixed.

CPUID is outside this schema and needs an RBX save slot and prologue save/CFI
planning. Its instruction emission cannot be migrated independently of those
frame requirements. Existing RDTSC/RDPMC/XGETBV sequences may later share a
reviewed EDX:EAX read recipe.

IRETQ and other owned entry/return adapters are outside ordinary returning
intrinsic recipes. Stack installation, terminal control flow, fault delivery
and asynchronous state ownership remain distinct contracts. Port I/O and x87
regions also need their own reviewed recipes.

The [global intrinsic registry](README.md#intrinsic-registry-and-family-declarations)
owns IDs and inventory positions independently of family membership. System
metadata no longer requires a contiguous tail or a fixed handwritten prefix;
an unrelated family can allocate an ID between system declarations. The scalar
type vocabulary still needs extension before vector intrinsic metadata migration.

## Compatibility evidence

Before this migration, seven compiler integration targets exported 122 complete
ELF objects and 122 assembly listings from `7ae03b80`. Fresh exports after the
migration match all 244 files byte for byte. Complete-object comparison includes
relocations and symbol tables, rather than only instruction text. The targets
cover the migrated families, both MSR hooks and neighbouring descriptor cases.
They exercise selectors, allocators, ordinary/empty pools and existing forced
rematerialization cases; exports are the explicit-v1 variants. Their tests also
check text/typed/chunked parity across the configured v1/v2/v3 matrices.

Local evidence is `/tmp/keel-system-recipes-before`,
`/tmp/keel-system-recipes-after` and
`/tmp/keel-system-recipes-comparison.json`. No new CPL0 campaign was run for
this refactor. Matching artifacts retain only their previous qualification
scope; they add no runtime semantics evidence.

Frozen fixtures separately check all 819 numeric IDs and inventory entries,
the 45 names/signatures/effects/target classifications, and all 35 public
register/single-operand helper mappings. Generator tests reject malformed
recipes and verify that an appended declaration can reuse an existing recipe
without a new backend allowlist. Existing literal/Clang/GNU witnesses, adjacency,
unused-read retention, foreign-target rejection and allocation tests remain.

This migration adds no instruction, privilege/state contract or kernel
entry/return capability. New bindings can reuse the regular recipes; new
semantics still require an emitter or an explicitly reviewed hook and tests.

The final workspace run passed 2,709 tests with zero failures and 13 ignored
across 496 reported groups. Workspace compilation, all 70 generator tests,
both generator freshness checks and diff checks passed. The workspace log is
`/tmp/keel-system-recipes-workspace.log`; the check and Python logs are
`/tmp/keel-system-recipes-final-check.log` and
`/tmp/keel-system-recipes-python.log`.

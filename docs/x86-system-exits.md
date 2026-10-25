# SYSEXIT and RSM assembler support

The public fixed-opcode adapters expose these operand-free instructions in
Intel and AT&T input. They use the existing shared ISA forms and generic
encoder/decoder.

| Canonical spelling | Aliases | Bytes | Public identity |
|---|---|---|---|
| `sysexit` | `sysexitd`, `sysexitl` | `0f 35` | `FixedOpcode::Sysexit` |
| `sysexitq` | — | `48 0f 35` | `FixedOpcode::Sysexitq` |
| `rsm` | — | `0f aa` | `FixedOpcode::Rsm` |

Keel's bare `sysexit` deliberately selects the 32-bit return form. The decoder
prints `sysexitq` for the REX.W form, preserving the distinction when canonical
instruction bytes are assembled again. Both spellings are accepted in either
dialect. Explicit operands and LOCK are rejected.

GNU Intel syntax requires `sysexitd` for the 32-bit form; LLVM Intel syntax
uses `sysexit`. Both external assemblers accept AT&T `sysexitl` and `sysexitq`.
The oracle tests account for those spelling differences rather than changing
the expected bytes.

## Generated mappings

`tools/x86-isa/keel-adapters.json` supplies the appended identities, aliases,
source mnemonics and operand-size selections. Generation emits fixed byte
tables, assembler normalization tuples and reverse width-to-mnemonic mappings.
The encoder and decoder consume those mappings. The same change removes the
handwritten fixed-opcode normalization match and fixes width-specific SYSRET
and 16-bit IRET formatting through their existing schema entries.

The RSM `I486` label now has an explicit empty optional-feature requirement.
This replaces its missing-policy fallback, without granting execution
permission. Catalogue contents and the source's unknown RSM CPL field remain
unchanged. The structural ledger now joins both SYSEXIT adapters and RSM to
their imported forms. Per-form validation in that ledger remains untracked.

## Execution boundary

These are assembler interfaces, not callable native IR intrinsics. No intrinsic
IDs, terminal compiler APIs or guest execution campaigns are added. The regular
fixed-instruction lowering recipe continues to reject these SYSRET-category
forms; an ordinary void intrinsic cannot own their terminal control flow.

SYSEXIT requires a configured selector/MSR and target register/stack protocol,
with CPU/vendor and mode preconditions. RSM requires an SMM saved-state
environment. Neither a v1 CPU profile nor successful assembly establishes
those conditions. See the architectural SYSEXIT and RSM entries in
[Intel's instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf),
and the project's [remaining transition contracts](x86-system-transitions.md).

Literal-byte, text/typed/chunked transport, GNU/Clang Intel and AT&T comparisons,
invalid-operand rejection and canonical decode/reassembly tests cover encoding
and formatting. They do not qualify execution or an SMM return contract.

## Validation

The full workspace run passed 2,713 tests, with zero failures and 13 ignored
across 497 reported groups. All 80 ISA generator tests and both generator
freshness checks passed. The refreshed audit inventory and documentation links
also checked successfully. Logs are `/tmp/keel-system-exits-workspace.log`,
`/tmp/keel-system-exits-focused.log` and `/tmp/keel-system-exits-generator.log`.

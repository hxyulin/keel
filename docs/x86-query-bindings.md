# Generated descriptor-query and scalar port-I/O bindings

`tools/x86-isa/query-bindings.json` owns reviewed metadata for 42 existing
intrinsics at IDs 687 through 728. `query_bindings.py` generates names,
signatures, purity, memory effects, target classification patterns and the
three public helper tables used by shared native emitters.

| Family | Bindings | Generated helper |
|---|---:|---|
| SGDT/SIDT, SLDT/STR/SMSW memory/register observations | 14 | `x86_descriptor_query()` |
| LAR/LSL and VERR/VERW register/memory queries | 16 | `x86_selector_query()` |
| IN/OUT, widths 8/16/32, DX or immediate port | 12 | `x86_port_io()` |

The schema retains exact signatures and helper return shapes. Descriptor
memory write footprints are separate from encoded operand widths. Register
observations retain their old i64 containers. LAR/LSL retain the old destination
and their separate I32 ZF result. Immediate port APIs retain an i16 parameter;
existing compiler verification still requires a constant port in 0..255.

Every binding remains impure, x86-only and a pinned compiler memory barrier.
The generator rejects attempts to weaken that policy, invalid opcode/width
combinations, incorrect signatures, wrong memory footprints and unknown fields.
These declarations are reviewed compiler policy, not semantics inferred from
instruction bytes or CPL metadata.

## Shared code and registry boundary

`crates/keel-ir/src/generated/x86_query_intrinsics.rs` supplies metadata and
helper implementations. Generic memory classification and AArch64/native
exhaustive classification use its generated patterns. The same metadata emitter
serves the existing 45 system declarations; their generated file is unchanged.

The descriptor, selector and scalar port-I/O emitters remain shared handwritten
Rust. Their staging order, result publication, ZF capture, zero extension,
allocator interaction and rematerialization behavior are unchanged. No new
instruction, intrinsic ID, encoder, operand rule or execution contract is added.

Enum declarations and `ALL` entries now come from the
[global intrinsic registry](../tools/x86-isa/README.md#intrinsic-registry-and-family-declarations).
Generated const assertions and the registry's family join check every query
ID. The frozen inventory checks numeric IDs and iteration order independently.
The registry cleanup removes the former 774-entry handwritten prefix and
contiguous system-tail assumption. Query metadata and lowering stay separate.

The structural coverage ledger still tracks its original 45 system lowering
declarations. It does not yet join these 42 query/I/O bindings. Empty compiler
links for their forms remain unknown coverage, rather than unsupported claims.

## Compatibility evidence and existing gaps

The independent metadata fixture was captured from `50b85e31` before generation.
It checks all 42 IDs, variants, names, signatures, effects, target classification
and helper values. Nonmembers must remain outside all three helper tables.

A new compile-only matrix keeps original inputs live and publishes query
results. It covers three CPU levels, three selectors, two allocators and both
ordinary and empty pools, with text/typed/chunked parity. All 36 complete ELF
objects and 36 assembly listings match their pre-migration counterparts byte
for byte. IR also matches. SHA-256 digests are recorded in
`/tmp/keel-query-bindings-comparison.json`; exports are under
`/tmp/keel-query-bindings-before` and `/tmp/keel-query-bindings-after`.

That successful-object matrix covers **38 bindings**. Before this migration,
the other four register APIs, `x86_lar32`, `x86_lar64`, `x86_lsl32` and
`x86_lsl64`, fail assembly: lowering selects a 32/64-bit destination with a
16-bit source register, which the current generated operand matcher rejects.
Their metadata is included and checked, and all 42 bindings reach typed
lowering. Fixing the operand spelling/matching boundary and adding independent
byte witnesses is follow-up work. This migration does not count the four APIs
as successfully assembled.

The baseline probe is `/tmp/keel-query-bindings-probe.log`. This is an existing
assembler integration gap, separate from the open CPL0 execution qualification
of descriptor queries and port I/O. No privileged guest campaign or native
privileged execution is added here.

## Validation

The workspace run passed 2,716 tests, with zero failures and 13 ignored across
499 reported groups. Workspace compilation, all 87 ISA generator tests and
both freshness checks passed. Final focused tests and freshly rewritten exports
also passed after removing unrelated formatting changes. Logs are
`/tmp/keel-query-bindings-workspace.log`, `/tmp/keel-query-bindings-check.log`,
`/tmp/keel-query-bindings-final-focused.log` and
`/tmp/keel-query-bindings-final-generator.log`.

## LAR/LSL register-view follow-up

The four assembly gaps above are fixed. The selector emitter uses the
same-width register spelling as the LAR/LSL destination. Only the low 16 bits
carry the selector. Memory sources remain m16, and VERR/VERW register sources
remain r16. Staging, old-destination handling and immediate ZF capture are
unchanged. No generated ISA or operand-matcher change is needed.

The successful-object matrix now includes all 42 bindings across all 36
configurations, with text/typed/chunked parity and original input liveness.
It no longer excludes the four wide register APIs. These expanded objects
are not compared against the earlier 38-binding object snapshots.

Six literal byte witnesses cover both instructions at all three register
widths, including extended registers and REX.W. Keel accepts both Intel and
AT&T spellings. Clang matches the literal encodings directly. GNU as 2.46
omits REX.W for these wide register forms in both dialects; its independent
witness therefore requests `rex.W` explicitly. This check preserves the
64-bit encoding instead of accepting GNU's unprefixed 32-bit encoding.

This closes compilation gaps, not the separate descriptor-state execution
qualification. No privileged instructions execute on the host.

Follow-up validation: both focused compiler tests passed, including all 42
bindings across 36 configurations and explicit register-view checks. The
literal/Clang/GNU oracle passed in both dialects. Both generator freshness
checks and formatting/diff checks passed.

The full workspace run completed 500 groups: 2,716 passed, 13 ignored and one
failure in an earlier compiled version of the new oracle test, which used
unsupported suffixed AT&T spellings. The final test uses register-inferred
widths and passed when rerun with workspace settings. All other targets passed.
Logs: `/tmp/keel-selector-fix-workspace.log`,
`/tmp/keel-selector-fix-workspace-oracle.log`,
`/tmp/keel-selector-fix-focused.log` and `/tmp/keel-selector-fix-codegen.log`.

# Baseline x86-64 system instruction work

The active goal is to complete baseline long-mode instruction support,
including privileged instructions. The psABI v1 profile describes userspace
CPU capabilities, not a kernel execution environment. The existing
[userspace contract](userspace-isa-status.md) still has its documented limits;
full baseline system support remains unfinished.

The assembler and decoder share generated ISA forms. Public mnemonic/operand
adapters and typed lowering determine which compiler paths are exposed. The
remaining work below tracks those layers and their validation.

## Current coverage

SGDT, SIDT, SLDT, STR and SMSW have typed observation APIs and assembler
encodings. Disposable CPL0 campaigns cover SGDT/SIDT and the twelve
SLDT/STR/SMSW forms with known selectors, two CR0 values and two old register
containers. See [descriptor/status observations](x86-descriptor-queries.md).
LAR, LSL, VERR and VERW have typed descriptor-query APIs. Scalar and string
port I/O have typed APIs, with their own permission and FLAGS contracts.

LGDT and LIDT now have assembler encodings and pinned typed APIs:
`x86_lgdt(ptr)` and `x86_lidt(ptr)`. Each reads one packed ten-byte image:
a little-endian u16 limit followed by a u64 linear base. They return nothing
and act as compiler memory barriers. They require 64-bit execution at CPL0
and readable input memory. The hardware validates the operation and delivers
faults; Keel adds no permission checks or recovery. Other backends reject them.
There is no optional feature bit, operand-size override, padding read or
synthesized table copy.

The caller owns table lifetime, contents and restoration. Loading GDTR does
not reload segment caches. Loading IDTR changes subsequent event delivery.
The tables and execution environment must permit all subsequent generated
code, stack accesses and returns. These APIs do not provide a boot protocol,
interrupt handler ABI or arbitrary execution-mode transition.

RDMSR and WRMSR have pinned typed APIs with explicit CPL0 and model-specific
preconditions, register-liveness checks and a two-index disposable guest
campaign. See [raw MSR access](x86-msr.md).

CLTS, INVD and WBINVD have pinned typed APIs and bounded CPL0 execution
checks. CLTS has direct CR0-state checks; physical cache writeback and
invalidation remain unqualified. See [privileged CPU/cache control](x86-privileged-control.md).

CLI, STI, HLT, SWAPGS, IRET and SYSRET have assembler encodings, but no typed
system-state/transition contract. An
assembler encoding alone is not evidence of working kernel code generation.

LLDT, LTR and LMSW have register/memory APIs, and INVLPG has an address-only
API over generated forms. CPL0 fixtures check selector changes, TSS busy bits,
LMSW low-bit/PE behavior and a TLB remap with a stale-mapping control. See
[system mutation/invalidation](x86-system-write.md) for contracts and limits.

Control/debug MOV operands and 26 typed read/write operations cover
CR0/2/3/4/8 and DR0 through DR7, including conditional DR4/5 aliases.
The caller must preserve continued compiled-code execution. The disposable
fixture checks independent state, full-width values, liveness and canaries
across 19 expected guest verdicts. See [raw system registers](x86-system-registers.md).

## Remaining baseline work

- Extend control/debug state qualification beyond the finite guest fixture.
  Arbitrary mode, address-space, breakpoint and exception transitions still
  need their own execution and entry/return contracts.
- Extend address-translation/cache qualification beyond the bounded emulated
  INVLPG remap and CPU/cache-control fixtures. Keep native hardware, multicore
  scope and additional address-space contexts explicit.
- Add typed CPU-control operations for the remaining baseline fixed opcodes,
  with state and control-flow contracts. The existing typed MSR operations
  retain explicit model/permission preconditions.
- Define entry/return, segment state and physical stack/frame ownership for
  SWAPGS, IRET, SYSCALL/SYSRET, SYSENTER/SYSEXIT, far transfers and segment
  loads. Preserve CPU/mode preconditions rather than treating v1 as a generic
  permission or transition ABI. RSM also needs its SMM saved-state contract;
  an ordinary function return does not implement these transitions.
- Extend guest validation to LAR/LSL/VERR/VERW and string I/O. Keep known raw
  FLAGS/x87 and saved-context limits visible. SLDT/STR/SMSW now have bounded
  CPL0 checks, including all operand sizes, memory canaries and old-value
  liveness; other machine states and fault paths remain unqualified.

Optional extensions such as VMX/SVM, SMX, XSAVE and INVPCID are separate from
this baseline goal. Instructions invalid in 64-bit mode do not become
long-mode requirements. CPU-specific MSRs are not all implied by v1.

## LGDT/LIDT validation

The load fixture exports `descriptor_load(ptr gdtr, ptr idtr)` at explicit V1
across three selection tiers, two allocators and normal/empty register pools.
The optimized tier runs IR optimization. Text, typed and chunked objects must
match. Literal encoding tests check instruction widths, and assembler tests
compare indexed, extended-register, address32 and RIP-relative operands with
Clang and GNU as. Invalid register and memory-width operands are rejected.

The guest bridge passes static GDT/IDT images to the generated payload, then
uses trusted SGDT/SIDT instructions to compare the resulting state against
linker-known bases and literal limits. The generated payload cannot supply
its own claimed observation. The bridge preserves the boot selectors, keeps
IF clear and installs no exception recovery. Negative controls return without
loading either table or load the tables in swapped order; each must return
FAIL normally, rather than fault or time out.

```sh
KEEL_DESCRIPTOR_LOAD_EXPORT_DIR="$PWD/target/descriptor-load-exports" \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding
python3 suite/x86-cpl0/descriptor_campaign.py --table-loads \
  --exports target/descriptor-load-exports --timeout 5
```

All twelve generated load variants passed under QEMU 11.1.2 TCG on macOS
ARM64. Both negative controls returned the exact READY/FAIL transcript and
exit 35. The literal encoding, Clang/GNU assembler comparisons, codegen parity
and ten runner unit tests passed in the same session.

See the [guest runner](../suite/x86-cpl0/README.md) for tool-path overrides and
isolation. This is bounded emulated evidence. It does not qualify native
bare-metal execution, exception resumption or every input.

Instruction semantics and privilege requirements follow Intel's
[instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [system programming guide](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf).

## Generated encoder integration

The privileged-instruction branch includes the shared-table migration and
its AArch64 disassembler/import-pilot prerequisite. The generator freshness
check and thirteen generator/pilot unit tests pass. The full assembler and
disassembler package suites, the workspace compile check and thirteen
focused privileged codegen tests pass against the generated encoder.

All five CPL0 campaigns pass using freshly regenerated objects: 14 table
observations, 14 table loads, 18 selector/status cases, 19 MSR cases and 18
CPU/cache-control cases. These are 83 expected guest verdicts. The individual
campaigns retain their documented limits. This integration check does not
establish that all workspace tests pass or complete the remaining system
instruction APIs.


The descriptor/status mutation and INVLPG adapters add 35 further expected
guest verdicts: 18 system-load cases and 17 invalidation cases. The five
previous campaigns also pass after the mode-selection refactor, for 118
verdicts across seven campaigns. The assembler/disassembler package suites,
16 focused codegen tests, generator freshness check, ten runner unit tests
and workspace compile check pass with these adapters.

## ABI validation backlog

The stale D8 aggregate rejection check now exercises a defined byval/sret tail
boundary. Natural integer and f32 D8 aggregates compile with text/typed object
parity; untagged f64x1 still rejects. The obsolete tagged-f64x1 tail rejection
was removed, retaining the existing positive recursive producer-ABI tests.

The restored [SysV tail oracle](../crates/keel-codegen/tests/sysv_tail/README.md)
passes all 36 exported selector, allocator, register-pool and CPU-level
configurations with GCC 16.2.0 and Clang 23.1.3 on x86-64 Linux. These are 72
native executions. Both compilers reject a deliberately broken constant-return
tail through normal value-check failures. The evidence covers this finite
fixture roster and does not complete the remaining privileged APIs.

The repaired privileged-instruction checkout passes `cargo test --workspace
--no-fail-fast`: 2,649 tests pass, none fail and 13 are ignored across 480
reported test groups. This run precedes the optional-extension adapter import;
it establishes the ABI repair against the original generated-table migration.

## Generated identity and extension integration

The privileged branch also includes the generated fixed/v2/v3 identity and
optional-extension adapter checkpoint, imported locally as `f52a0791` after
the generated-file attributes commit `cf30013a`. The import leaves privileged
intrinsic IDs, CoreOpcode identities and compiler lowering unchanged.

The integrated checkout passes the workspace compile check, 16 generator/pilot
tests and generator freshness check, all 432 assembler-IR/assembler/disassembler
tests, and 23 focused privileged/AES/PCLMUL codegen tests. Fresh payloads pass
all 118 expected CPL0 verdicts across seven campaigns. Fresh tail objects pass
72 native GCC/Clang executions and reject both broken-tail controls.
The guest evidence retains its emulated, single-CPU and fixture-specific limits.
The full workspace test run reported above predates this adapter import; the
post-import checks listed here are targeted integration evidence.

## Control/debug register integration

The control/debug assembler adapters preserve existing RegisterClass IDs and
symbolic tags. The 26 pinned typed operations extend the registry through ID
813. The 416-combination Clang/GNU matrix, compiler tier/allocator parity and
independent guest state/liveness fixture pass. All eight CPL0 campaigns pass
137 expected verdicts using fresh exports, including 19 register-access cases.
The full workspace run passes with 2,658 tests, zero failures and 13 ignored
across 483 reported groups. This run includes optional-extension adapters and
precedes the later feature-field and baseline/x87 identity refactors.

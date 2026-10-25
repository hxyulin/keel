# Baseline x86-64 system instruction checkpoint

The goal is to complete baseline long-mode instruction support,
including privileged instructions. The psABI v1 profile describes userspace
CPU capabilities, not a kernel execution environment. The existing
[userspace contract](userspace-isa-status.md) still has its documented limits;
full baseline system support remains unfinished.

The assembler and decoder share generated ISA forms. Public mnemonic/operand
adapters and typed lowering determine which compiler paths are exposed. The
remaining work below tracks those layers and their validation. The
[generated structural ledger](x86-coverage/README.md) accounts for imported
forms and declarative candidates; it does not infer execution qualification
from these historical records.

The [remaining instruction audit](x86-system-audit.md) inventories 122 selected
source records and separates public assembler gaps, missing compiler contracts,
existing interfaces needing qualification, and optional extension work.

## Checkpoint boundary

The implementation checkpoint is `583297c7`. Its recorded validation is
2,674 workspace tests passed, zero failed and 13 ignored, plus 194 expected
guest verdicts across eleven campaigns. The guest evidence uses QEMU TCG;
it does not establish native hardware, multicore or arbitrary fault recovery.
The detailed validation ledger below records which exports were reused.

| Family | Exposed compiler interface | Execution evidence |
|---|---|---|
| SGDT/SIDT/LGDT/LIDT | Typed observation/load APIs | Owned CPL0 table fixtures |
| SLDT/STR/SMSW | Memory and register-width APIs | CPL0 state, width, old-value and canary checks |
| LAR/LSL/VERR/VERW | Typed query APIs | CPL0 execution qualification remains open |
| RDMSR/WRMSR, CLTS/INVD/WBINVD | Typed raw operations | Bounded CPL0 state checks; physical cache effects unqualified |
| CR/DR access, LLDT/LTR/LMSW/INVLPG | Typed register, memory and address operations | CPL0 state, liveness and remap checks |
| CLI/STI/HLT, adjacent STI;HLT | Typed raw operations | Owned IRQ/IST fixture with independently checked HLT resumption |
| SWAPGS | Typed raw operation | Both GS bases, effective reads, selector preservation and liveness |
| IRETQ | Owned terminal GPR/frame adapter | CPL0 and CPL3 landings, high canonical RIP/RSP and negative controls |
| Scalar and string port I/O | Typed operations with permission/state contracts | Compile/encoding checks; privileged device execution remains open |
| Other entry/return, arbitrary INT, far/segment transfers, RSM | Incomplete native interfaces/contracts | Follow-up implementation; see [transition audit](x86-system-transitions.md) |

This is a checkpoint of the interfaces and finite evidence above. Imported ISA
forms, public assembler identities and callable compiler interfaces are
separate layers. Neither imported forms nor the v1 CPU profile imply complete
system support or an operating-system entry ABI.

## Current coverage

SGDT, SIDT, SLDT, STR and SMSW have typed observation APIs and assembler
encodings. Disposable CPL0 campaigns cover SGDT/SIDT and the twelve
SLDT/STR/SMSW forms with known selectors, two CR0 values and two old register
containers. See [descriptor/status observations](x86-descriptor-queries.md).
LAR, LSL, VERR and VERW have typed descriptor-query APIs. Scalar and string
port I/O have typed APIs, with their own permission and FLAGS contracts.
The [query/I/O binding migration](x86-query-bindings.md) generates metadata for
42 existing APIs. It also records four pre-existing assembly gaps in the
32/64-bit register LAR/LSL APIs; those paths need an operand spelling/matcher
fix before CPL0 qualification.

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

CLI, STI and HLT have pinned typed APIs, including adjacent STI;HLT as a
single IR operation. An owned CPL0 IRQ fixture checks IF, wide-value liveness,
interrupt-frame RIP and actual HLT fall-through boundaries. See
[interrupt control and halt](x86-interrupt-control.md).
SWAPGS has a pinned raw API with independent base, effective-address,
selector and liveness checks. See [raw GS-base exchange](x86-swapgs.md).
IRETQ now has an [owned terminal GPR/frame transport](x86-iretq.md), with
CPL0 and CPL3 landing checks. SYSRET and other terminal families still need
distinct contracts.

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
- Define RSM's SMM saved-state contract over its generated public assembler adapter.
  The remaining fixed control transfers need the terminal contracts below;
  the existing typed MSR operations retain model/permission preconditions.
- Define arbitrary-vector software-interrupt transport and handler
  preservation requirements. `debugtrap` already emits INT3, while INT imm8
  currently has assembler support without a native IR interface.
- Define entry/return, segment state and physical stack/frame ownership for
  remaining IRET variants, SYSCALL/SYSRET, SYSENTER/SYSEXIT, far transfers
  and segment loads.
  Preserve CPU/mode preconditions rather than treating v1 as a generic
  permission or transition ABI. RSM also needs its SMM saved-state contract;
  an ordinary function return does not implement these transitions. The
  [transition audit](x86-system-transitions.md) records current interfaces,
  the RSM execution gap and requirements for owned terminal paths.
- Extend guest validation to LAR/LSL/VERR/VERW and scalar/string port I/O.
  Keep known raw FLAGS/x87 and saved-context limits visible. SLDT/STR/SMSW
  now have bounded
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

The later feature-field and baseline/x87 identity refactors merge cleanly.
Post-import validation passes the workspace compile check, all 440
assembler-IR/assembler/disassembler tests, 26 focused codegen tests, 25 Python
generator tests and both freshness checks. All 98 freshly regenerated guest
objects retain the exact bytes qualified by the 137-verdict campaign run.
The full workspace count above precedes these two refactors.

## Interrupt control integration

IDs 814 through 817 add CLI, STI, HLT and adjacent STI;HLT as pinned raw
operations. The new owned IRQ fixture checks IF, active/distinct IST stacks,
wide-value liveness, canaries and unmodified hardware-frame RIPs. GNU objdump
binds those RIPs to HLT fall-through addresses and checks pair adjacency.
All 18 new expected verdicts pass, and all nine CPL0 campaigns pass 155
expected verdicts using fresh exports. The full workspace run passes with
2,666 tests, zero failures and 13 ignored across 485 groups. This run precedes
the later register-schema import; it does not complete the remaining entry,
return, segment and system-state contracts.

After register-schema import `b26ba773`, targeted integration checks pass:
workspace compile check, 444 assembler-IR/assembler/disassembler tests,
28 focused codegen tests, 33 generator/pilot tests, both freshness checks
and 15 runner/evidence tests. All 110 fresh guest objects are byte-for-byte
identical to those qualified by the 155 expected verdicts. The full workspace
result above remains scoped to the source before this import.

## GS exchange integration

ID 818 adds raw SWAPGS as a pinned memory barrier. The owned guest checks both
base MSRs, selector preservation, independent GS-relative observations,
round trips, wide canonical bases and eight live SSA inputs. All 19 expected
verdicts pass. All ten CPL0 campaigns pass 174 expected verdicts using 122
fresh generated objects. The 110 existing objects remain byte-for-byte
identical to those qualified before this addition.

Both generator freshness checks, all 33 generator/pilot tests and all 15
runner/evidence tests pass. A separate experimental IRETQ assembly reference
restores fifteen GPRs onto a distinct stack at CPL0, and three broken controls
fail normally. This experiment establishes no public return API or privilege
transition; see the [transition audit](x86-system-transitions.md).

The full workspace `cargo test --workspace --no-fail-fast` run after the
register-schema import and SWAPGS addition passes: 2672 passed, zero
failed and 13 ignored across 487 reported groups. This result does
not complete the remaining terminal, segment and SMM contracts.

## Owned IRETQ integration

The public terminal adapter defines a 160-byte GPR/frame image and a SysV
AMD64 `noreturn` import. Its final stack installation, R10 restoration and
IRETQ have no intervening helper, spill or ordinary epilogue. This adapter
and its two-privilege fixture replace the earlier experimental reference.

All twelve explicit-V1 caller variants and an independent assembly reference
restore fifteen GPRs onto distinct CPL0/CPL3 stacks. The CPL3 landing uses high
canonical RIP and RSP, with a separate kernel TSS stack for the trusted return
gate. Different flag patterns, image preservation, live inputs and canaries
are checked. Seven broken controls fail normally, for twenty expected verdicts.
All eleven guest campaigns pass 194 expected verdicts in this round. The ten
existing campaigns reuse their 122 previously qualified payload objects;
existing instruction lowering and generated metadata are unchanged.

The full workspace `cargo test --workspace --no-fail-fast` passes: 2674
passed, zero failed and 13 ignored across 488 reported groups.
The workspace compile check, both generator freshness checks, 33 generator
and 15 runner tests pass. The adapter's 68 instruction bytes independently
match GNU as and Clang. See [the image and return contract](x86-iretq.md).
Remaining terminal, software-interrupt, segment and SMM work is still open.


## Combined main and generated-metadata integration

Merge `ea2c6e87` joins privileged checkpoint `e5f83489`, the generated
register/identity/feature work, and main through `c5e27200`. It retains both
histories and main's newer frontend and optimizer changes.

The exposed privileged instructions use the shared generated ISA catalogue
and generic assembler/decoder. This includes descriptor loads and status
operations, MSR and CPU/cache control, CR/DR moves, interrupt control,
SWAPGS and IRETQ. No new instruction-specific encoder or duplicate ISA
catalogue is introduced. A literal-byte regression checks the generated fixed
adapters against the exact forms selected by the decoder.

Compiler intrinsic signatures, effects and reserved-register lowering remain
handwritten. The adjacent STI;HLT node and terminal IRETQ image/stack contract
also remain explicit compiler interfaces. Generated encoding metadata does
not establish privilege, valid machine state or safe entry/return ownership.
The remaining implementation gaps in the transition audit still apply.

Validation on the combined source passes: 2,695 workspace tests, zero failures
and 13 ignored across 492 reported groups; workspace compile check; all 33
ISA generator tests; both generator freshness checks; and all 15 guest runner
and evidence tests. All eleven CPL0 campaigns pass 194 expected verdicts
under QEMU 11.1.2 TCG using 135 freshly exported objects. This rerun includes
all older campaigns rather than carrying their prior object evidence forward.
It remains bounded emulated evidence, not native hardware qualification.

Local logs, per-campaign results and SHA-256 object digests are retained under
`/tmp/keel-privileged-generated-integration/`.

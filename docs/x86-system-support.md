# Baseline x86-64 system instruction work

The active goal is to complete baseline long-mode instruction support,
including privileged instructions. The psABI v1 profile describes userspace
CPU capabilities, not a kernel execution environment. The existing
[userspace contract](userspace-isa-status.md) still has its documented limits;
full baseline system support remains unfinished.

## Current coverage

SGDT, SIDT, SLDT, STR and SMSW have typed observation APIs and assembler
encodings. Disposable CPL0 campaigns cover SGDT/SIDT and the twelve
SLDT/STR/SMSW forms with known selectors, two CR0 values and two old register
containers. See [descriptor/status observations](x86-descriptor-queries.md).
LAR, LSL, VERR and VERW have typed descriptor-query APIs. Scalar and string port I/O have typed
APIs, with their own permission and FLAGS contracts.

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

CLI, STI, HLT, CLTS, INVD, WBINVD, RDMSR, WRMSR, SWAPGS, IRET and SYSRET have
assembler encodings, but no typed system-state/transition contract. An
assembler encoding alone is not evidence of working kernel code generation.

## Remaining baseline work

- Add LLDT, LTR and LMSW operand forms and contracts, including descriptor
  side effects and permitted state changes.
- Add control/debug register operands and MOV encodings. Design typed access
  around the registers whose mutation permits ordinary compiled code to
  continue; mode-changing operations need an explicit transition contract.
- Add INVLPG and qualify address translation/cache operations in disposable
  guests. Define the distinction between compiler barriers and hardware
  invalidation, including local-CPU scope.
- Add typed MSR and CPU-control operations, with fixed-register liveness tests
  and explicit model/permission preconditions.
- Define entry/return, segment state and physical stack/frame ownership for
  SWAPGS, IRET, SYSRET and related transitions. Ordinary function returns do
  not establish these contracts.
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

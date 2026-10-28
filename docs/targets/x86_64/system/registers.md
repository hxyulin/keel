# Raw control and debug register access

Keel exposes `x86_read_cr0`, `x86_read_cr2`, `x86_read_cr3`, `x86_read_cr4`,
`x86_read_cr8`, and `x86_read_dr0` through `x86_read_dr7`. Each takes no
arguments and returns one `i64`. The corresponding `x86_write_*` operation
takes one `i64` and returns nothing. IDs 788 through 813 are append-only,
ordered as read/write pairs for CR0/2/3/4/8, then DR0 through DR7.

These are raw long-mode operations at CPL0. All reads and writes are pinned
compiler memory barriers, including reads whose results are unused. They emit
one MOV to or from the named register, using a 64-bit GPR. Ordinary spill and
result moves may surround that access. Values retain their full SSA bit patterns;
the hardware applies register-specific writable and reserved-bit behavior.
There is no feature probe, masking helper, interrupt masking, synthesized
state save, exception recovery or extra hardware fence. AArch64 rejects them.

The caller owns the execution state, including permission, reserved bits,
legal value combinations, event delivery and any virtualization interception.
DR4 and DR5 alias DR6 and DR7 only with CR4.DE clear; they fault with DE set.
Debug access also requires a compatible DR7.GD state. Enabling breakpoints
requires a working debug exception environment. The API adds no such handler.

Every write must leave all subsequent compiled code, stack/frame accesses,
return addresses and ABI obligations valid. CR3 changes must preserve the
mappings and permissions used by the function and its callers. Changes to
CR0/CR4 must preserve long-mode execution and any processor state needed by
the selected instructions, including SIMD spills. These ordinary-function
APIs do not implement arbitrary paging, mode or stack transitions. CR8 changes
can affect interrupt priority. Compiler barriers do not establish a C11
synchronization operation, cross-CPU TLB invalidation or a uniform hardware
serialization guarantee across these registers.

The assembler accepts the two MOV directions between any 64-bit GPR and
CR0/2/3/4/8 or DR0–7. Typed validation rejects reserved CR indices, DR8 and
above, narrower GPRs, memory operands, LOCK and malformed register metadata.
CR8 uses REX.R;
extended GPRs use REX.B. REX.W is unnecessary for the canonical encoding.
Control and debug registers have separate symbolic-cache tags.

## Validation

The literal matrix covers all 416 register/direction combinations, checks
canonical formatting and Intel/AT&T parsing, and compares bytes with Clang
and GNU as. Decoder witnesses include CR8, extended GPRs and the DR4/5 aliases, with truncation
checks. Compiler tests cover V1/V2/V3, three selection tiers, both allocators,
normal/all-spill pools, optimized unused reads and forced rematerialization
of a constant write input. Text, typed and chunked object routes must agree.

The disposable CPL0 fixture independently seeds each register, calls a
generated function, reads the actual resulting hardware state and compares
literal expected output with canaries. Eight distinct 64-bit SSA arguments
remain live across each register function's accesses. CR0 changes AM, CR4 changes PGE, CR2 and
debug addresses use distinct wide values, and CR8 changes task priority.
CR3 switches between two aligned copies of the boot page map. Debug breakpoint
enables and GD remain clear, and DE stays clear for the alias cases. The bridge
restores the saved control/debug register values before returning.

```sh
KEEL_SYSTEM_REGISTER_EXPORT_DIR="$PWD/target/system-register-exports" \
  cargo test -p keel-codegen --test x86_system_registers
python3 suite/integration/x86-cpl0/descriptor_campaign.py --system-registers \
  --exports target/system-register-exports --timeout 5
```

All twelve explicit-V1 standard variants, one forced-rematerialization variant and an
independent assembly reference pass under QEMU 11.1.2 TCG on macOS ARM64.
Five broken controls return the exact READY/FAIL transcript and exit 35:
omitted writes, narrowed reads, a wrong debug register, a clobbered live value
and a no-op payload. These are 19 expected verdicts. Faults and timeouts cannot
satisfy a negative control.

This is bounded emulated evidence. It does not qualify native hardware,
active breakpoints, DR7.GD traps, DE-set alias faults, PCID/no-flush behavior,
arbitrary CR0/CR4 transitions, multicore state changes or every input.
Instruction contracts follow Intel's [instruction reference, MOV to/from
control and debug registers](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).

The full workspace run passes with this implementation: 2,658 tests pass,
none fail and 13 are ignored across 483 reported groups. Fresh exports also
pass all 137 expected guest verdicts across the eight CPL0 campaigns. Generator
freshness, all 16 generator/pilot tests and the ten runner unit tests pass.
This full run includes the optional-extension import but precedes the later
feature-field and baseline/x87 identity boilerplate refactors.

After importing the feature-field and baseline/x87 identity refactors as
`021a6a6c` and `cf562b1b`, the workspace compile check, all 440
assembler-IR/assembler/disassembler tests, 26 focused codegen tests, 25
Python generator tests and both generator freshness checks pass. All 98
freshly rewritten generated guest objects are byte-for-byte identical to
the objects qualified by the 137 expected verdicts above. The full workspace
result remains scoped to the source before these two refactors.

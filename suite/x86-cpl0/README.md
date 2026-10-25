# Disposable x86 CPL0 checkpoint

This small runner boots one freestanding payload in a bounded, disposable QEMU
TCG guest. It does not execute privileged instructions on the host. Its default
smoke checks CPL0, long-mode entry, SMSW.PE, zeroed BSS and call-stack alignment.
This is finite **emulated** behavior, not native bare-metal qualification,
userspace/OS-observed behavior, or an ISA-wide completion claim.

## Run

Requirements: Python 3, GNU x86-64 `as`, `ld`, `objcopy`, and an existing
`qemu-system-x86_64` with PC BIOS/multiboot and TCG support. Nothing is installed.
Tool paths/prefixes can be selected with `--as`, `--ld`, `--objcopy`, `--qemu`.
The macOS system assembler/linker are not GNU ELF tools. For example, local
cross-binutils can be selected with `--as x86_64-elf-as --ld x86_64-elf-ld
--objcopy x86_64-elf-objcopy`. `--build-only` needs no QEMU and establishes only
assembly/link/image-placement evidence.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s suite/x86-cpl0 -v
python3 suite/x86-cpl0/run.py --timeout 5
```

Each invocation creates a fresh `/tmp/keel-cpl0-*` directory containing ELF,
flat boot image, intermediate objects, debugcon transcript and QEMU stdout/
stderr. JSON on stdout reports the artifact path, exact QEMU argv, transcript,
return code, status and evidence level. CLI exit zero means `pass`, or `built`
when explicitly using `--build-only`. All errors/timeouts exit one. Timeout is
positive and at most 30 seconds (default five); build subprocesses have separate
20-second bounds. No generated artifacts belong in Git.

## External payload contract

Supply one or more existing ELF64 little-endian x86-64 **relocatable** objects:

```sh
python3 suite/x86-cpl0/run.py --payload bridge.o --payload case.o
```

Exactly one object defines `uint32_t guest_main(void)`, called using SysV AMD64.
Return EAX=0 for success; any nonzero value fails. Preserve the SysV callee-saved
registers, restore RSP, and return with DF clear. No arguments or other initial
register values are promised. RSP is 16-byte aligned immediately before CALL
(8 modulo 16 on entry); the dedicated stack is 64 KiB. BSS is zeroed before
page-table setup and the first call. Payload text, rodata, data and BSS are linked
into the same low-memory image; all unresolved helpers must be supplied. The
linker and flat-image validator reject an image/BSS beyond the 32 MiB guest
RAM limit. Payload accesses must also stay within RAM and the identity map.
No ELF executable loader, dynamic linking, TLS, libc or operating system exists.

C bridges should use an existing x86-64 ELF compiler with `-ffreestanding
-mno-red-zone -mgeneral-regs-only -fno-stack-protector -fno-pic -fno-pie`.
The generic harness does not initialize FP/SIMD state. Interrupts stay disabled;
no exception handlers are installed. Payloads must not change execution mode,
interrupt state or mappings needed by the harness. A fault/reset/triple fault
fails through the exit/transcript check or timeout; it is not recovered.

The multiboot flat image begins at physical/linear `0x100000`; its 32-byte
address-bearing header and entry/load/BSS bounds are checked after GNU objcopy.
`guest.elf` is a link/debug container; **QEMU receives `guest.bin`**, never that
ELF64 container. The boot/link source is adapted from an earlier
x86 system harness, with the entry call redirected to this small harness. It zeroes BSS, constructs identity-mapped
2 MiB pages, enables PAE/LME/paging, loads a 64-bit GDT code selector, and calls
through a correctly aligned stack. This is boot implementation, not a compiler
API for arbitrary control-register or page-table mutation.

## Status and isolation

Before calling the payload, guest assembly checks CS.RPL=0 and EFER.LMA=1 after
the 64-bit far transfer, then emits `KEEL CPL0 LONG64\n`. A zero payload result
emits `KEEL PASS\n` and writes 0x10 to the debug-exit device (process exit 33).
A failure emits `KEEL FAIL\n` and writes 0x11 (exit 35). Output uses debugcon
port **0xF1**, and debug-exit uses **0xF4**, avoiding pc-testdev's E8–EB ports.
There is no serial UART protocol. Payload diagnostics may appear between the
initial ready line and terminal status; reserve the three `KEEL` status lines
for the harness.

A pass requires exit 33, exactly one ready line at the start, exactly one PASS
line at the end, and no FAIL marker. A timeout containing PASS still fails.
QEMU runs explicitly with single-thread TCG, one vCPU, no networking, no shared
mounts, no attached host devices/disks, no monitor, and no user configuration.
Only the generated flat image and output files are passed to QEMU. There is no
KVM, root setup, host permission change or native privileged execution.
`-no-reboot` is set; `-no-shutdown` is deliberately absent.

## Validation

The host unit tests, the default smoke (exact two-line transcript, exit 33), a
payload returning one (FAIL, exit 35), and a payload that prints PASS and then
hangs (reported as timeout, not pass) have been run on x86-64 Linux with GNU
binutils and QEMU 11.1.1.

## SGDT/SIDT campaign

The assembly bridge `cases/descriptor_bridge.S` defines `guest_main` and calls
one separately linked `descriptor_observe`. It checks the boot selectors
(CS=8; DS/ES/SS=16), loads a static 32-byte GDT preserving those descriptors and
a static 512-byte IDT, and leaves mode, mappings and IF=0 intact. No exception
resumption is provided. All tables and buffers live for the whole guest.
The bridge fills 32 output bytes with 0xa5 and compares every byte after the
opaque call against a separate expected image: literal limits 31/511 and
linker-known seed-table addresses, never values read from SGDT/SIDT output.
This checks SGDT offsets 1–10, SIDT offsets 17–26 and all twelve canary bytes.
The bridge uses integer-only assembly, no red zone, C runtime or SIMD helpers.

Export all twelve explicit V1 variants from the current source tree, then run
on a host with QEMU and GNU binutils (override tool paths as above):

```sh
KEEL_DESCRIPTOR_EXPORT_DIR=target/descriptor-cpl0-exports \
  cargo test -p keel-codegen \
  --test x86_descriptor_cpl0 --test x86_descriptor_queries_encoding
python3 suite/x86-cpl0/descriptor_campaign.py \
  --exports target/descriptor-cpl0-exports --timeout 5
```

The campaign accepts exactly the twelve standard object filenames, links each
with the bridge independently, and requires the exact two-line READY/PASS
transcript and exit 33. Two substituted observation payloads must instead
produce the exact READY/FAIL transcript and exit 35: one returns without any
observation, the other observes correctly then corrupts a canary. A fault or
timeout cannot satisfy these controls. Each campaign stores all commands,
images, transcripts and per-case JSON in a fresh
`/tmp/keel-descriptor-campaign-*` directory.

All twelve variants pass under QEMU on x86-64 Linux, both negative controls
fail as expected, an oversized-BSS object is rejected at link time, and all
twelve linked reference images match `descriptor_expected.py` using only linker
seed symbols.

This qualifies only the bounded emulated SGDT/SIDT comparison and canaries.
The generic smoke and compile/encoding gates are separate evidence. Native
bare-metal behavior, the other twelve descriptor/status APIs, SMSW wide
containers and arbitrary FLAGS/x87 state remain outside this checkpoint.

## LGDT/LIDT campaign

Export the twelve table-load objects, then use the same campaign with
`--table-loads`:

```sh
KEEL_DESCRIPTOR_LOAD_EXPORT_DIR="$PWD/target/descriptor-load-exports" \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding
python3 suite/x86-cpl0/descriptor_campaign.py --table-loads \
  --exports target/descriptor-load-exports --timeout 5
```

The bridge calls the generated `descriptor_load` with known GDTR/IDTR images
instead of loading them itself. Trusted SGDT/SIDT observations then check both
bases and limits. Negative controls omit the loads or swap the table images.
Both must return FAIL with exit 35. Normal exports must return PASS with exit
33. See [system support](../../docs/x86-system-support.md) for API contracts.

All twelve load variants pass under QEMU 11.1.2 TCG on macOS ARM64. Both
negative controls return the exact READY/FAIL transcript and exit 35. This is
bounded emulated evidence, not native bare-metal qualification.

## SLDT/STR/SMSW campaign

```sh
KEEL_DESCRIPTOR_STATUS_EXPORT_DIR="$PWD/target/descriptor-status-exports" \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding
python3 suite/x86-cpl0/descriptor_campaign.py --status \
  --exports target/descriptor-status-exports --timeout 5
```

This mode runs exactly twelve generated objects, one independent assembly
reference and five negative controls. Each positive payload must pass two
seeded LDTR/TR/CR0/container cases with an exact 128-byte comparison. The bridge
owns GDT/LDT/TSS lifetime and privileged setup; the generated payload only
observes state. The reference must pass. Negative controls for omitted
observations, normalized r16, widened m16, narrowed SMSW and a clobbered old
value must return FAIL and exit 35 normally. Faults and timeouts fail the
campaign. `--status` and `--table-loads` are mutually exclusive.

All eighteen verdicts match under QEMU 11.1.2 TCG on macOS ARM64. See
[descriptor/status observations](../../docs/x86-descriptor-queries.md#cpl0-selectorstatus-fixture)
for constants, memory layout and the limits of this emulated evidence.

## MSR campaign

```sh
KEEL_MSR_EXPORT_DIR="$PWD/target/msr-exports" \
  cargo test -p keel-codegen --test x86_msr
python3 suite/x86-cpl0/descriptor_campaign.py --msr \
  --exports target/msr-exports --timeout 5
```

This mode requires twelve standard explicit-v1 MSR exports and one forced
rematerialization export. The bridge checks
reads and writes of two canonical GS-base MSRs, original input liveness,
eight additional live values, incoming stack arguments and output canaries.
A separate constant-index/value function checks constant operands and its
returned result. The bridge independently checks post-write MSR state and restores both original MSRs
on normal success or mismatch. The fixture uses no GS-relative accesses or
SWAPGS. It does not recover from faults.

All thirteen generated variants and an independent assembly reference pass
under QEMU 11.1.2 TCG on macOS ARM64. Five broken controls must return the
exact READY/FAIL transcript and exit 35: omitted write, narrow read, wrong
index, clobbered live value and no observations. All five fail as expected.
The other campaign modes are mutually exclusive with `--msr`. See
[raw MSR access](../../docs/x86-msr.md) for the API contract and evidence limits.

## CLTS and cache-control campaign

```sh
KEEL_CPU_CONTROL_EXPORT_DIR="$PWD/target/cpu-control-exports" \
  cargo test -p keel-codegen --test x86_privileged_control
python3 suite/x86-cpl0/descriptor_campaign.py --cpu-control \
  --exports target/cpu-control-exports --timeout 5
```

This mode requires exactly twelve v1 exports. It checks CLTS against three
CR0 states, original integer inputs, incoming stack arguments and a 128-byte
output with canaries. The generated WBINVD/INVD instructions are adjacent.
The bridge restores its original CR0 on normal success or mismatch.

All twelve variants and an independent reference pass under QEMU 11.1.2 TCG
on macOS ARM64. Four controls fail normally with exit 35: omitted CLTS,
altered CR0.WP, a clobbered live value and no operations. A scope control that
omits both cache instructions also passes, demonstrating that these state
checks do not prove physical cache writeback or invalidation. All eighteen
verdicts match. Faults and timeouts fail the campaign.

`--cpu-control` is mutually exclusive with the other campaign modes. See
[privileged CPU/cache control](../../docs/x86-privileged-control.md) for the
contracts and remaining evidence limits.

## LLDT/LTR/LMSW and INVLPG campaigns

```sh
KEEL_SYSTEM_LOAD_EXPORT_DIR="$PWD/target/system-load-exports" \
KEEL_INVLPG_EXPORT_DIR="$PWD/target/invlpg-exports" \
  cargo test -p keel-codegen --test x86_system_write
python3 suite/x86-cpl0/descriptor_campaign.py --system-loads \
  --exports target/system-load-exports --timeout 5
python3 suite/x86-cpl0/descriptor_campaign.py --invlpg \
  --exports target/invlpg-exports --timeout 5
```

Each mode accepts exactly twelve v1 exports. The system-load bridge checks
register and memory mutations against trusted selector/CR0 observations,
TSS busy bits, original input values and canaries. Its reference passes and
five controls fail normally, for eighteen expected verdicts.

The INVLPG bridge replaces a 4 KiB mapping after reading its old value. The
generated invalidation must expose the new value, while the omission control
must fail. Unmapped and noncanonical address-only calls also return normally.
An independent reference passes and four controls fail normally, for seventeen
expected verdicts. Mapping changes are restored on normal success or mismatch.

Both campaigns pass under QEMU 11.1.2 TCG on macOS ARM64. They provide bounded
emulated evidence, with no native hardware, multicore or exception-resumption
claim. The modes are mutually exclusive with all other campaign modes. See
[system mutation/invalidation](../../docs/x86-system-write.md) for seeds,
contracts and limits.

## Control and debug registers

`--system-registers` runs the 13 exported payload variants, an independent
assembly reference and five normally returning broken controls. The fixture
covers CR0/2/3/4/8 and DR0–7, including DR4/5 aliases with DE clear. It checks
actual hardware state and output values separately, switches CR3 between two
copies of the same page map, and restores the original state. No breakpoint
is enabled. See [contracts and finite evidence](../../docs/x86-system-registers.md).

## Interrupt control and HLT resumption

`--interrupt-control` requires GNU objdump (`--objdump`) in addition to the
usual tools. The guest owns a private GDT/TSS/IDT with an IRQ0 gate on an IST
stack and a one-shot PIT timer. It checks CLI/STI state, raw HLT resumption and
an adjacent STI;HLT pair entered with IF clear and an IRQ already pending.
Eight wide values and canaries check the compiled continuation.

Positive transcripts include two fixed-format RIP records between READY and
PASS. The host binds them to independently decoded HLT fall-through addresses;
the pair also requires adjacent STI. This mode rejects a skipped halt even if
it races the timer and returns a complete report. A negative control must
return normally with exact READY/FAIL and exit 35, or with a well-formed
success report whose HLT evidence is rejected. Faults, malformed transcripts,
tool errors and timeouts never satisfy a negative. Existing modes keep their
previous exact transcript and exit checks. See
[contracts, observation rules and limits](../../docs/x86-interrupt-control.md).

## GS exchange

`--swapgs` runs twelve generated payloads, an independent reference and six
normally returning broken controls. It checks both base MSRs, selector zero
and 16, independent GS-relative reads, round trips, wide canonical bases,
eight live SSA values and canaries. The bridge restores the original selector
and both base MSRs. IF stays clear. See
[contracts and finite evidence](../../docs/x86-swapgs.md).

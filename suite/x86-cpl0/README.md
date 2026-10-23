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

Run all tests, builds and guest campaigns under the existing shared lock:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 suite/fast_opt_locked.py \
  /tmp/keel-cpl0-unique.jsonl \
  python3 -m unittest discover -s suite/x86-cpl0 -v
python3 suite/fast_opt_locked.py /tmp/keel-cpl0-unique.jsonl \
  python3 suite/x86-cpl0/run.py --timeout 5
```

Each invocation creates a fresh `/tmp/keel-cpl0-*` directory containing ELF,
flat boot image, intermediate objects, debugcon transcript and QEMU stdout/
stderr. JSON on stdout reports the artifact path, exact QEMU argv, transcript,
return code, status and evidence level. CLI exit zero means `pass`, or `built`
when explicitly using `--build-only`. All errors/timeouts exit one. Timeout is
positive and at most 30 seconds (default five); build subprocesses have separate
20-second bounds. No generated artifacts or evidence receipts belong in Git.
The wrapper serializes against `/tmp/keel-perf-20261019.lock`.

## External payload contract

Supply one or more existing ELF64 little-endian x86-64 **relocatable** objects:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-cpl0-unique.jsonl \
  python3 suite/x86-cpl0/run.py --payload /tmp/bridge.o --payload /tmp/case.o
```

Exactly one object defines `uint32_t guest_main(void)`, called using SysV AMD64.
Return EAX=0 for success; any nonzero value fails. Preserve the SysV callee-saved
registers, restore RSP, and return with DF clear. No arguments or other initial
register values are promised. RSP is 16-byte aligned immediately before CALL
(8 modulo 16 on entry); the dedicated stack is 64 KiB. BSS is zeroed before
page-table setup and the first call. Payload text, rodata, data and BSS are linked
into the same low-memory image; all unresolved helpers must be supplied. The
linker rejects an image/BSS outside the first identity-mapped GiB. Guest RAM is
32 MiB, so payloads and their accesses must also fit that smaller physical RAM.
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
ELF64 container. The archived working boot/link source from
`db1778f4^:suite/x86_system_harness/{boot.S,link.ld}` is retained with the entry
call redirected to the small harness. It zeroes BSS, constructs identity-mapped
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

## Recorded finite validation

On 2026-10-23, based on main `bfa86ed7`, nine deterministic host tests passed
locally and on LinuxHost. Local GNU cross-binutils built and validated the smoke's
flat image. LinuxHost's existing GNU binutils and existing QEMU 11.1.1 at
`/nix/store/adr1v05h9jcfjfdky8hdrcyrl9vhgzjx-qemu-for-vm-tests-11.1.1/bin/qemu-system-x86_64`
ran the default smoke with the exact expected two-line transcript and exit 33
(`/tmp/keel-cpl0-dvxofcdz`). An external `guest_main` returning one produced
FAIL and exit 35 (`/tmp/keel-cpl0-55g1g9tp`). Another external payload emitted
PASS then halted forever: it was killed at one second and reported timeout,
not pass (`/tmp/keel-cpl0-tklzkni3`). Temporary negative-control source, exact
commands and results remain in `/tmp/keel-cpl0-validation.l8rQot` on LinuxHost;
lock logs are `/tmp/keel-cpl0-unique.jsonl`. No Rust changed or compiled.

Descriptor execution remains a subsequent combination. The maintained
`cases/descriptor.keel` exports `descriptor_observe(out)`, not `guest_main`:
a separate bridge must seed its known tables, fill 32 output bytes with 0xa5,
call that function, compare the independently expected complete image, then
return the result. Link the bridge and exactly one exported descriptor variant
using repeated `--payload`. The generic smoke does not qualify descriptor
footprints, SMSW wide-container semantics, or arbitrary FLAGS/x87 state.

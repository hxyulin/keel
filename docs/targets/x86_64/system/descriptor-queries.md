# Long-mode descriptor and status observations

Keel exposes fourteen x86-64 intrinsics that observe descriptor-table and
machine-status registers.

## Memory forms

`x86_sgdt(ptr)` and `x86_sidt(ptr)` write exactly ten bytes, in packed order:
16-bit limit followed immediately by 64-bit base, little endian. There is no
padding write. `x86_sldt_mem(ptr)`, `x86_str_mem(ptr)`, and `x86_smsw_mem(ptr)`
write exactly two bytes. The emitted instruction performs the memory access;
there is no temporary image or synthesized wider store.

## Register forms

`x86_{sldt,str,smsw}{16,32,64}(i64 old) -> i64` exposes a physical register
container:

- r16 replaces bits 15:0 and preserves `old` bits 63:16.
- r32 and r64 use the actual corresponding hardware encoding, not a normalized
  r16 observation.
- STR: r32/r64 zero-extend the selector.
- SLDT in IA-32e mode: the wide forms zero-extend the selector. Historic
  non-IA-32e undefined upper bits do not define this long-mode API.
- SMSW: following the Intel SDM's explicit 64-bit-mode examples, r32 is
  CR0[31:0] (with the architectural upper 32 bits clear) and r64 is CR0[63:0].
  The opcode table's undefined-high-16 wording and non-64-bit description are
  not used to override these long-mode semantics. No non-64-bit guarantee is
  made.

`old` is preserved as an independent SSA value even if the allocator reuses its
physical home for the result. r32/r64 accept `old` for a uniform container
interface, but the hardware overwrites its defined bits.

## Properties

All fourteen APIs are pinned barriers, observe implicit machine state, may
fault, and remain when results are unused. They add no frontend syntax or IR
fields and need no CPU feature bit beyond the baseline (NONE/V1/V2/V3). Non-x86
backends reject them. The instructions preserve flags, but this API does not
establish a general RFLAGS preservation contract across ABI boundaries.

## Preconditions

- 64-bit execution, and either CPL0 or CR4.UMIP clear.
- Valid writable memory for the memory forms.
- SLDT/STR require protected or IA-32e mode.

Hardware #GP/#SS/#PF and #AC remain hardware behavior. Unaligned memory is
allowed when alignment checking permits it; no universal unaligned/no-fault
claim is made. Keel inserts no descriptor restoration, table mutation, CR4 read,
privileged setup, software fallback, or permission test.

Linux can emulate UMIP #GP queries, and CPUID.(7,0):ECX[2] reports UMIP
capability rather than CR4 enablement. A successful user-mode run can therefore
show OS-observed behavior without demonstrating the raw hardware contract.
Native bare-metal CPL0 validation has not been done. The emulated CPL0
fixtures below cover SGDT/SIDT and, separately, all twelve SLDT/STR/SMSW forms
with bounded seed values.

## CPL0 table-observation fixture

`suite/integration/x86-cpl0/cases/descriptor.keel` exports one SysV AMD64 function:
`void descriptor_observe(unsigned char out[32])`. RDI points to 32 writable
bytes initially filled with `0xa5`; the caller provides normal SysV stack
alignment. SGDT writes offsets 1-10 and SIDT writes offsets 17-26 (both
deliberately unaligned). Every other byte, including those immediately before
and after each ten-byte image, must remain `0xa5`. The payload has no external
helpers, table setup, control-register mutation, or host-execution path.

The guest harness seeds a known GDT and IDT, enters 64-bit CPL0, checks CS/CPL
and long mode, initializes the output, calls the function, and compares all 32
bytes. It owns boot, BSS initialization, stack setup, an integer-only bridge,
table lifetime, and guest termination. It must load a flat Multiboot image when
using that boot contract; an ELF64 container is not an equivalent QEMU Multiboot
input. Only the expected exit status together with the expected transcript
counts as a pass; a timeout containing PASS, a reset, or a triple fault does
not. Runs are bounded by a QEMU timeout, use no network, shared mounts, or host
devices, and TCG results are labeled as emulated.

`cases/descriptor_bridge.S` loads a static GDT (limit 31) and IDT (limit 511).
Its code/data descriptors preserve boot selectors 8/16, and it leaves IF=0 and
the execution mode intact. Both tables live for the whole guest run. It
provides an opaque call boundary, uses no SIMD or red zone, needs no C compiler
helpers, and installs no exception resumption.

`descriptor_expected.py` independently packs the harness's seeded limits and
linear bases as little-endian u16/u64 images. Feed it linker/setup values,
never values read by the generated queries. The addresses below are example
inputs, not an assumed boot layout:

```sh
python3 suite/integration/x86-cpl0/cases/descriptor_expected.py --self-test
python3 suite/integration/x86-cpl0/cases/descriptor_expected.py \
  --gdt-base 0x1000 --gdt-limit 0x27 \
  --idt-base 0x2000 --idt-limit 0xfff > descriptor-expected.bin
KEEL_DESCRIPTOR_EXPORT_DIR="$PWD/descriptor-exports" \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding
```

The export test writes 12 ELF objects plus assembly, one per selection tier,
allocator, and normal/empty pool combination. `descriptor-Full-Full-false.o` is
the optimized normal-pool payload; link one variant at a time. Text, typed, and
chunked objects must match at explicit V1; the Full selector also runs IR
optimization. A separate compile check confirms all fourteen queries remain when
their results are unused. Literal encoding tests cover operand widths. SMSW
wide-container semantics and `old`-value liveness are not covered by the guest
fixture.

Run the guest campaign with `suite/integration/x86-cpl0/descriptor_campaign.py --exports DIR`,
with optional `--qemu`, `--as`, `--ld`, `--objcopy`, and `--timeout` overrides.
See `suite/integration/x86-cpl0/README.md` for the complete commands. All twelve explicit V1
variants, including Full optimization and forced spills, pass the 32-byte
comparison under QEMU 11.1.1 TCG on x86-64 Linux (READY/PASS, exit 33).
Negative controls (an observation-free function, or a correct observation
followed by a canary overwrite) produce READY/FAIL and exit 35. This is
emulated-guest evidence for SGDT/SIDT only; it is not a native, Linux UMIP, or
full fourteen-API qualification.

## CPL0 selector/status fixture

`cases/descriptor_status.keel` exports
`void descriptor_status(unsigned char out[128], uint64_t old)`. It exercises
the memory, r16, r32 and r64 forms of each of SLDT, STR and SMSW. The original
`old` SSA value remains live past every query and is stored separately.
The output starts filled with `0xa5`:

- SLDT, STR and SMSW memory outputs occupy offsets 1–2, 5–6 and 9–10.
- Nine full register containers occupy offsets 16–87, grouped by mnemonic and
  then increasing operand width.
- The original `old` occupies offsets 96–103. Every other byte is a canary.

The assembly bridge installs a GDT preserving boot selectors 8 and 16, plus
16-byte LDT and TSS descriptors. GNU assembly loads LDTR=0x18 and TR=0x28.
These are fixture setup, not compiler APIs for LLDT or LTR. The TSS and LDT
live for the whole disposable guest, and LTR may set the TSS descriptor busy.
No stack switch, task switch or exception recovery occurs.

The bridge compares the entire output against two literal expected images:
first LDTR=0x18, CR0=0x80010033 and `old=0xdeadbeefcafe7654`; then a null LDTR,
CR0=0x8005003b and `old=0x0123456789abfedc`. TR remains 0x28. CR0 keeps PE,
PG and the existing page tables active. The second case adds AM and TS; the
fixture uses no FP/SIMD and keeps AC and IF clear. Expectations derive from
the setup constants rather than queries of the observed state. Nonzero bits
above CR0 bit 15 distinguish wide SMSW from a zero-extended memory result.

```sh
KEEL_DESCRIPTOR_STATUS_EXPORT_DIR="$PWD/target/descriptor-status-exports" \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding
python3 suite/integration/x86-cpl0/descriptor_campaign.py --status \
  --exports target/descriptor-status-exports --timeout 5
```

All twelve explicit-V1 selection/allocator/pool variants pass both cases under
QEMU 11.1.2 TCG on macOS ARM64, with matching text, typed and chunked objects.
An independently assembled reference also passes. Five deliberately broken
payloads must return the exact READY/FAIL transcript and exit 35: no
observations, normalized r16, a canary overwrite simulating widened m16,
narrowed SMSW wide results, and a clobbered original value. All five fail as
expected without faults or timeouts. This covers these finite value and
memory checks; it does not establish fault resumption, every possible CR0
state or native bare-metal behavior.

## References

Intel SDM 325462-093US (September 2026), Vol. 2B: SGDT 4-629/630, SIDT
4-655/656, SLDT 4-657/658, SMSW 4-659/660, STR 4-680/681.
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf

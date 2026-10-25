# Baseline privileged CPU and cache control

`x86_clts()`, `x86_invd()` and `x86_wbinvd()` take no arguments and return
nothing. Each emits its named instruction using the existing fixed encoding:

- CLTS: `0f 06`.
- INVD: `0f 08`.
- WBINVD: `0f 09`.

The intrinsic IDs append as 778, 779 and 780. All three are pinned compiler
memory barriers, remain after optimization and require no optional feature
bit at v1/v2/v3. No permission probe, helper call, extra fence or substitute
instruction is inserted. The AArch64 backend rejects them.

## Preconditions and effects

The caller must run in 64-bit mode at CPL0, with the operation permitted by
the CPU and current system or virtual-machine state. Hardware faults remain
hardware behavior. There is no exception recovery or software fallback.

CLTS clears CR0.TS (bit 3). It does not restore a saved x87/MMX/SSE/XSTATE
context, clear CR0.EM or establish FPU ownership. The caller owns those state
requirements. A compiler memory barrier does not establish a precise
exception boundary for surrounding ordinary FP/SIMD expressions or fix the
placement of pure arithmetic. This API does not create a lazy-FPU handler ABI.

INVD invalidates caches without writing modified data back. The caller must
arrange that discarding dirty lines leaves the code, stack and data needed
by subsequent compiled instructions valid. WBINVD requests writeback followed
by invalidation according to the CPU's cache hierarchy. These operations are
not interchangeable. Cache scope and external-cache behavior follow the
hardware; Keel adds no interprocessor broadcast, DMA ownership protocol or
TLB-shootdown sequence.

The caller owns cache and machine-state transitions. Compiler barriers order
memory effects; they do not replace the CPU's architectural ordering rules.
The APIs do not promise a full RFLAGS image at function boundaries or a
general physical stack/frame contract. An adjacent WBINVD/INVD sequence in
one fixture does not establish a general compiler-owned cache region.

## Disposable CPL0 validation

`cases/cpu_control.keel` exports `cpu_control(out, live0, ..., live7)` using
SysV integer arguments. It stores one input before CLTS, WBINVD and INVD,
then stores all eight original inputs after them. The 128-byte output retains
canaries at bytes 0–7 and 80–127. Three inputs arrive on the stack.

The bridge compares CR0 against three literal seed-derived expectations:
0x8001003b becomes 0x80010033, 0x8005003b becomes 0x80050033, and 0x80010033
remains unchanged. This checks TS clearing, retention of other CR0 bits and
idempotence with TS already clear. All cases keep long mode, paging and the
existing page tables active. The bridge saves and restores its original CR0
on normal success or mismatch. IF, AC and DF start clear. The fixture uses no
FP/SIMD or exception recovery.

The compile matrix covers three selection tiers, both allocators and
normal/empty pools at explicit v1, v2 and v3. The Full tier runs IR
optimization. Text, typed and chunked objects must match. Literal encodings,
operand counts, pinned/barrier effects and foreign-target rejection are
checked. Each generated body retains all three instructions. Its WBINVD and
INVD are adjacent machine instructions, with no intervening compiler spill
store. Only the twelve v1 variants are exported for the guest campaign.

```sh
KEEL_CPU_CONTROL_EXPORT_DIR="$PWD/target/cpu-control-exports" \
  cargo test -p keel-codegen --test x86_privileged_control
python3 suite/x86-cpl0/descriptor_campaign.py --cpu-control \
  --exports target/cpu-control-exports --timeout 5
```

All twelve generated variants and an independent assembly reference pass all
three cases under QEMU 11.1.2 TCG on macOS ARM64. Four broken payloads must
return the exact READY/FAIL transcript and exit 35: omitted CLTS, altered
CR0.WP, a clobbered live value and no operations. All four fail as expected
without faults or timeouts.

A separate scope control omits INVD and WBINVD while retaining CLTS and the
same data stores. It also passes. The CR0/data oracle therefore cannot prove
cache writeback or invalidation. Literal encodings and instruction counts
establish compiler emission; the guest campaign establishes bounded emulated
execution, CR0 and data behavior. Native cache coherency, external buses,
multicore effects, permission faults and lazy-FPU exception delivery remain
unqualified. The campaign checks eighteen verdicts in total.

The instruction contracts follow Intel's
[instruction reference](https://cdrdv2-public.intel.com/874240/325462-090-sdm-vol-1-2abcd-3abcd-4.pdf)
and [system programming guide](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3a-part-1-manual.pdf).
See the [guest runner](../suite/x86-cpl0/README.md) for tool-path overrides and
isolation.

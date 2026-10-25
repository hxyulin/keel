# Descriptor/status mutation and address invalidation

Keel exposes seven new baseline long-mode intrinsics:

- `x86_lldt(i16 selector)` and `x86_lldt_mem(ptr selector)`.
- `x86_ltr(i16 selector)` and `x86_ltr_mem(ptr selector)`.
- `x86_lmsw(i16 word)` and `x86_lmsw_mem(ptr word)`.
- `x86_invlpg(ptr address)`.

They return nothing, are pinned compiler memory barriers and remain after
optimization. IDs append as 781–787. The register forms consume the low
sixteen bits directly. Each `_mem` form uses one hardware m16 operand, with
no synthesized load or widened explicit operand. LLDT/LTR also access the
selected GDT descriptor; LTR modifies its busy bit. Those implicit descriptor
accesses are additional to the two-byte selector read.

Four public assembler identities select the existing generated ISA forms.
No instruction-specific encoder or new generated form was added. Register,
Intel/AT&T memory, extended-register, address32 and RIP-relative cases have
literal byte witnesses and Clang/GNU comparisons. Invalid register classes
and explicit memory widths are rejected. These instructions need no optional
CPU feature bit at v1/v2/v3. The AArch64 backend rejects the intrinsics.

## State and privilege contracts

All APIs require 64-bit execution at CPL0, with the operation permitted by
the CPU and current system/virtual-machine state. Memory operands and any
implicit descriptor accesses must be valid. Faults are hardware behavior;
Keel inserts no permission probe, fallback, restoration or exception handler.

LLDT installs an LDT selector and hidden descriptor state. A null selector
makes LDTR unusable for ordinary LDT descriptor accesses. A non-null selector
must identify a suitable present LDT descriptor in the GDT. The caller owns
its sixteen-byte long-mode descriptor and the table's lifetime.

LTR requires a valid available 64-bit TSS descriptor in the GDT. Hardware
loads TR and marks the descriptor busy. The descriptor must permit that
write. The caller owns descriptor/table lifetime, TSS contents and their use
by later events. The intrinsic does not perform a task or stack switch, clear
the previous TSS busy bit or implement an interrupt-entry ABI.

LMSW applies source bits 3:0 to the machine-status controls, with PE remaining
set when already set. Bits above CR0 bit 3 remain unchanged. It cannot leave
protected/long mode by clearing PE. The caller owns the effects on MP, EM and
TS and the associated FPU state. A compiler memory barrier does not create a
precise exception boundary for ordinary FP/SIMD expressions.

INVLPG computes the supplied address and requests invalidation for its page
on the executing logical processor. It does not dereference an operand byte,
require a present mapping or synthesize a data read. The encoded m8 describes
the address form. In the baseline 48-bit environment, a noncanonical address
has the architectural no-operation behavior. Hardware may invalidate more
entries than one page. The API adds no interprocessor broadcast, PCID protocol
or page-table ownership scheme. The caller must arrange mapping updates,
other CPUs and subsequent code/stack/data accesses correctly.

These APIs do not preserve a full RFLAGS image at function boundaries or
establish general saved-context, TLS, physical-frame or event-return contracts.
LGDT/LIDT now share their single-operand lowering path; their existing API,
packed ten-byte format and privilege contracts remain unchanged.

## Descriptor/status guest campaign

`cases/system_load.keel` exports `system_reg` and `system_mem`. Both perform
LLDT, LTR and LMSW, then store their original input values and a live i64 into
a 32-byte output with canaries. The memory inputs are deliberately unaligned
and have a separate expected image, checking that they remain unchanged in
this fixture's disjoint layout.

The trusted assembly bridge installs a GDT preserving the boot code/data
selectors, one LDT and two available TSS descriptors. It starts with LDTR=0
and TR=0x38, then checks two generated calls:

- Register inputs load LDTR=0x18 and TR=0x28. LMSW source 0xfffe changes
  CR0 from 0x80010033 to 0x8001003f while retaining PE.
- Memory inputs load null LDTR and TR=0x38. LMSW source 0xfff0 changes
  CR0 from 0x8005003b to 0x80050031, retaining the higher CR0 bits and PE.

Trusted SLDT/STR/MOV-CR0 observations compare against literal expectations.
The bridge checks both TSS busy bits, making the second TSS available again
before its generated LTR. It restores original CR0 on normal success or
mismatch. Tables live for the whole guest. IF/AC/DF start clear; the fixture
uses no FP/SIMD, task switching or exception recovery.

```sh
KEEL_SYSTEM_LOAD_EXPORT_DIR="$PWD/target/system-load-exports" \
  cargo test -p keel-codegen --test x86_system_write
python3 suite/x86-cpl0/descriptor_campaign.py --system-loads \
  --exports target/system-load-exports --timeout 5
```

All twelve v1 variants and an independent assembly reference pass under QEMU
11.1.2 TCG on macOS ARM64. Five controls fail normally with exact READY/FAIL
and exit 35: omitted LLDT, omitted LTR, omitted LMSW, a clobbered live input
and no operations. The campaign checks eighteen expected verdicts.

## INVLPG guest campaign

The bridge replaces the boot mapping at virtual address 4 MiB with a 4 KiB
page table, initially mapping a known page A. It reads A to populate the TLB,
changes only the PTE to page B and calls the generated payload with address
0x400fff. The payload emits INVLPG and stores its original address and marker.
The bridge must observe B and the exact output with canaries.

Two further calls use unmapped address 0x401fff and noncanonical address
0x0000800000000000. Both must return normally with the original address bits
and marker intact. The next virtual page is unmapped, so this fixture also
exercises an address at the end of a page without a data read. The bridge
checks that its entire image/BSS stays below the test window and restores the
original page-directory entry and CR3 context on normal success or mismatch.

```sh
KEEL_INVLPG_EXPORT_DIR="$PWD/target/invlpg-exports" \
  cargo test -p keel-codegen --test x86_system_write
python3 suite/x86-cpl0/descriptor_campaign.py --invlpg \
  --exports target/invlpg-exports --timeout 5
```

All twelve variants and an independent assembly reference pass the remap and
two address-only calls under QEMU 11.1.2 TCG. Four controls return exact
READY/FAIL and exit 35: omitted invalidation, a corrupt marker, a corrupt
address result and no operations. In this bounded TCG run, the no-invalidation
control retains A after the PTE update and therefore cannot pass the remap
check. The campaign checks seventeen expected verdicts.

Both compile matrices cover all selection tiers, both allocators and
normal/empty pools at explicit v1/v2/v3. Full optimization runs, and text,
typed and chunked objects must match. Only v1 objects are exported. The guest
results qualify these finite emulated states. Native MMU behavior, multicore
shootdowns, PCIDs, arbitrary descriptor faults and exception resumption remain
unqualified. A timeout or fault cannot satisfy a negative control.

The contracts follow Intel's [instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [paging interactions reference](https://cdrdv2-public.intel.com/671368/architecture-instruction-set-extensions-programming-reference.pdf).
See the [guest runner](../suite/x86-cpl0/README.md) for isolation and tool paths.

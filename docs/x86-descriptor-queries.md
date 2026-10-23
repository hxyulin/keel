# Long-mode descriptor and status observations

`x86_sgdt(ptr)` and `x86_sidt(ptr)` write exactly ten bytes, in packed order:
16-bit limit followed immediately by 64-bit base, little endian. There is no
padding write. `x86_sldt_mem(ptr)`, `x86_str_mem(ptr)`, and `x86_smsw_mem(ptr)`
write exactly two bytes. The emitted instruction performs the memory access;
there is no temporary image or synthesized wider store.

`x86_{sldt,str,smsw}{16,32,64}(i64 old) -> i64` exposes a physical register
container. r16 replaces bits15:0 and preserves old bits63:16. r32 and r64 use the
actual corresponding hardware encoding, not a normalized r16 observation.
For STR, r32/r64 zero-extend the selector. For SLDT in IA-32e, the wide forms
zero-extend the selector; historic non-IA-32e undefined upper bits do not define
this long-mode API. For SMSW, Intel093's explicit 64-bit-mode examples define
r32 as CR0[31:0] (with the architectural upper32 clear) and r64 as CR0[63:0].
Its opcode table's undefined-high16 wording and non-64-bit description are not
used to erase these explicit long-mode semantics. No non-64-bit guarantee is
made. Old is preserved as an independent SSA value even if the allocator reuses
its physical home for the result. r32/r64 accept old for a uniform physical
container interface but their hardware overwrites its defined bits.

All fourteen APIs are pinned barriers, observe implicit machine state, may
fault, and remain when results are unused. They add no frontend syntax or IR
fields. CPU baseline NONE/V1/V2/V3 needs no invented extension bit. Foreign
backends explicitly reject them. The instructions preserve flags, though this
API does not establish an arbitrary ABI-boundary RFLAGS preservation contract.

Preconditions: 64-bit execution, CPL0 or CR4.UMIP clear; valid writable memory
for a successful memory query. SLDT/STR require protected/IA-32e mode. Hardware
#GP/#SS/#PF and #AC remain hardware operations. Unaligned memory is allowed
when alignment checking permits it; no universal unaligned/no-fault claim is
made. No descriptor restoration, table mutation, CR4 read at CPL3, privileged
setup, software fallback or permission test is inserted.

Linux can emulate UMIP #GP queries. CPUID.(7,0):ECX[2] reports UMIP capability,
not CR4 enablement. Successful user-mode forced-assembly comparisons can
therefore prove OS-observed behavior without proving this raw permitted-hardware
contract. A native receipt must collect CS/CPL, CPUID capability and observed
results and retain raw-mode proof pending unless independently established.
CPL0 proof remains pending. The bounded emulated checkpoint below prepares a
controlled table observation; it does not establish native bare-metal behavior.
No all-context or full-profile completion follows from this stage.

## Bounded CPL0 table-observation fixture

`suite/x86-cpl0/cases/descriptor.keel` exports one SysV AMD64 function:
`void descriptor_observe(unsigned char out[32])`. RDI carries a pointer to 32
writable bytes initially filled with `0xa5`. The caller supplies the normal
SysV stack alignment (RSP aligned to 16 before CALL). SGDT writes offsets 1–10;
SIDT writes offsets 17–26. Every other byte must remain `0xa5`, including the
byte immediately before and after each ten-byte image. Both writes are
deliberately unaligned. The payload has no external helpers, table setup,
control-register mutation, or host-execution path.

The external disposable-guest harness must seed a known GDT and IDT, enter
64-bit CPL0, record actual CS/CPL and long-mode evidence, initialize the output,
call the function, and compare all 32 bytes. It owns boot, BSS initialization,
stack setup, freestanding/no-red-zone C compilation, table lifetime and guest
termination. It must load a flat Multiboot image if using that boot contract;
an ELF64 container is not an equivalent QEMU Multiboot input. Accept only the
expected exit status together with the expected transcript, never a timeout
containing PASS, a reset or a triple fault. Guest campaigns must be serialized
under the existing suite lock, bounded by a QEMU timeout, with no network,
shared mounts or host devices. TCG results must be labeled emulated.

`descriptor_expected.py` independently packs the harness's **seeded** limits
and linear bases as little-endian u16/u64 images. Feed it linker/setup values,
never values read by the generated queries. Example addresses below are oracle
test inputs, not an assumed boot layout:

```sh
python3 suite/x86-cpl0/cases/descriptor_expected.py --self-test
python3 suite/x86-cpl0/cases/descriptor_expected.py \
  --gdt-base 0x1000 --gdt-limit 0x27 \
  --idt-base 0x2000 --idt-limit 0xfff > /tmp/descriptor-expected.bin
env KEEL_DESCRIPTOR_EXPORT_DIR=/tmp/keel-descriptor-cpl0-exports \
  python3 suite/fast_opt_locked.py /tmp/keel-cpl0-descriptor-unique.jsonl \
  cargo test -p keel-codegen --test x86_descriptor_cpl0 \
  --test x86_descriptor_queries_encoding --offline --locked -j2
```

The export gate creates 12 ELF objects plus assembly, one per selection tier,
allocator and normal/empty pool combination, under the requested temporary
directory. `descriptor-Full-Full-false.o` is the optimized normal-pool payload;
link one variant at a time. Text, typed and chunked objects must match at
explicit V1; the Full selector also runs IR optimization. A separate compile
gate confirms all fourteen descriptor/status queries remain when their results
are unused across those paths. Existing literal complete-body encoding tests
remain the independent operand-width control. These are compile/encoding and
export checks, not guest memory-footprint evidence or whole-fourteen-API CPL0
qualification. SMSW wide-container semantics and old-value liveness are outside
this two-instruction guest checkpoint.

At this checkpoint the local environment has `cargo` but no
`qemu-system-x86_64` on PATH. Combined controlled-CPL0 execution is explicitly
pending parent harness integration; there is no native or emulated guest-pass
claim. No toolchain installation or host privilege changes are required by the
source/export gate.

Primary authority: Intel SDM 325462-093US, September 2026, official combined PDF
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
SGDT Vol2B 4-629/630, SIDT 4-655/656, SLDT 4-657/658, SMSW 4-659/660,
STR 4-680/681; SGDT begins at PDF page1918. Exact extracted pages and their
hashes are retained in the proof packet. These are observations only, not
permission to weaken the environment precondition.

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
CPL0 proof remains pending; a separate private bare-metal proof can follow root's
design. No all-context or full-profile completion follows from this stage.

Primary authority: Intel SDM 325462-093US, September 2026, official combined PDF
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
SGDT Vol2B 4-629/630, SIDT 4-655/656, SLDT 4-657/658, SMSW 4-659/660,
STR 4-680/681; SGDT begins at PDF page1918. Exact extracted pages and their
hashes are retained in the proof packet. These are observations only, not
permission to weaken the environment precondition.

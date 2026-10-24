# Bounded CLFLUSH compiler boundary

`intrinsic x86_clflush(ptr)` is x86-only, returns nothing, and is pinned with
`MemoryEffects::Barrier`. Optimization retains every execution and preserves
compiler ordering against ordinary memory operations. Lowering emits exactly one
`CLFLUSH m8` at the original pointer: no alignment, masking, probing load, software
fallback, or implicit fence. GPR/SIMD operands live across it remain SSA values.
The caller must supply a valid readable address in ordinary live memory. Read-only
memory is valid. Invalid addresses retain architectural faults; this is not an
advisory prefetch or a fault-suppression API. No frontend mapping is added.

## Architectural references

[Intel SDM 253666-093US, September 2026, CLFLUSH, pp. 3-144–145](https://cdrdv2.intel.com/v1/dl/getContent/671199):
CPUID leaf 1 EDX bit 19 independently indicates CLFSH; SSE2 does not imply it.
The operation writes back modified data and invalidates the addressed line across
the coherence domain. EBX bits 15:8 times eight give line bytes. Permission and
fault checks follow a byte load, including the execute-only-segment exception;
page-table A can be set, D is not. Noncanonical addresses and paging failures
fault. Speculative fetching may repopulate the line. Intel orders CLFLUSH against
CLFLUSH, writes, locked read-modify-write operations and fences; this does not
promise general load ordering, cache residency or persistence. Other cache
maintenance ordering needs the separately documented synchronization protocol.

[AMD APM 24594 revision 3.37, July 2025, CLFLUSH, pp. 150–151](https://docs.amd.com/api/khub/documents/j44LvPXzuuXgM0WyHKQfeQ/content):
the same independent capability and line-size fields apply. Dirty cache copies
are written back before invalidation; local matching write-combining buffers are
also drained. Read-like permissions and A/D behavior apply. AMD distinguishes
processors without CLFLUSHOPT: CLFLUSH is weakly ordered and MFENCE is needed
before/after for the corresponding earlier-store/later-memory guarantees;
LFENCE, SFENCE and serializing operations do not order it. With CLFLUSHOPT,
locked operations and fences order CLFLUSH; writes and flushes to the same line
are ordered, while other-line writes/flushes are not. This API inserts no fences;
callers needing cross-vendor hardware order must supply explicit synchronization.
CLFLUSHOPT is mentioned only to state AMD's ordering distinction, not exposed.

[The psABI Processor Architecture table and runtime-detection rule](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
list no CLFSH in baseline, v2 or v3. Unlisted features require detection even if
current CPUs implement them together. `Features::clflush`, `+clflush` and
`-clflush` therefore represent an independent capability. NONE, legacy and all
three levels leave it false; `Features::all()` includes it explicitly. The
compiler and both assembler paths require that bit, so CLFLUSH is never legal
under a plain `--cpu-level v1`, `v2` or `v3` profile without `+clflush`.

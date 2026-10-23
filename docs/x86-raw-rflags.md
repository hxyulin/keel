# Raw RFLAGS: partial implementation and production blocker

`fullflags = false`; the full v1 profile and full user goal remain incomplete.
The existing 729 registry IDs are unchanged. ID 729 is `x86_rflags_read()->i64`;
ID 730 reserves standalone `x86_rflags_swap(i64)->i64` and is rejected by x86 codegen.
The explicit terminal-monitor wrapper now implements owned swap; see
[x86-rflags-owned-region.md](x86-rflags-owned-region.md). Native acceptance is false.
Both are x86-only, pinned, possibly faulting memory barriers, retained when dead.
Neither needs LAHF/SAHF, SSE, or a post-v1 feature.

## Read contract

Read emits actual `PUSHFQ; POP rax`, then flag-neutral MOV publication/spill
transport. The result is the architectural PUSHFQ image, including RF/VM cleared
by hardware, not a software reconstruction or an arbitrary 64-bit register image.
It observes current machine flags at that emitted PUSHFQ, after prologue, argument
transport, and previous lowering. It does not observe function-entry flags.
Compiler arithmetic, comparisons, branch lowering, and other optimization can
change flags; successive reads do not imply continuity of a FLAGS SSA value.
GCM is conservatively disabled for functions containing either API so it cannot
move pure arithmetic across the observation. Other transformations still define
the emitted boundary; source arithmetic is not a way to seed raw FLAGS.

Only reserved RAX is clobbered. All allocated live SSA values are preserved.
PUSHFQ writes exactly eight bytes at [old RSP-8, old RSP), POP reads the same eight
bytes, and successful completion restores RSP exactly. The backend has no red-zone
locals; fixed objects are above current RSP, even with dynamic alloca. No call or
stack realignment occurs within the pair. Intrinsics disable frameless functions;
the existing rbp+16 CFA remains valid at the intermediate PC. Normal return,
callee saves, and epilogue remain unchanged. Faults are retained and pinned;
there is no promise to recover RSP after a fault, emulate inaccessible stack
memory, or operate in an arbitrary interrupt/signal context. The caller supplies
a normal ABI-compatible execution state and accessible stack.

## Swap is blocked, not restricted to application flags

The intended instruction sequence captures old flags with PUSHFQ/POP, captures
input before aliased output publication, then PUSH r64/POPFQ. Both stack accesses
are exactly eight bytes. POPFQ must apply actual CPL/IOPL, reserved-bit, RF, VM,
VIF/VIP, IF, DF, TF, AC/SMAP and ID semantics, including faults. In particular,
unwritable privilege bits can remain unchanged without a privilege fault; a
software mask or an application-only bit subset would not implement this API.

Concrete IR blocker: intrinsic metadata has only purity, memory effects and trap
properties; no owned FLAGS token, CFG region, permitted instruction set, or
restoration proof exists. Pinning swap alone cannot stop pure arithmetic from
moving across it. Disabling GCM alone cannot stop later lowering from generating
ADD/CMP/TEST, library calls or an epilogue after POPFQ. The verifier cannot prove
that arbitrary input restores DF=0 and legal TF/AC/IF/IOPL/execution state before
ordinary calls/return, or account for debug/alignment faults inside that interval.
Returning the old image also cannot restore RF/VM: PUSHFQ clears them in its image.
The existing low-flags boundary API and x87-specific region are not such a proof.

Production swap therefore fails before allocation/emission, including dead-result
uses outside ownership. The explicit terminal-monitor wrapper separately installs
exact POPFQ expansion with structural and machine-resource checks under declared
entry/environment preconditions. Root-owned terminal/debug/interrupt and native
proof remains outstanding; the wrapper does not enable the standalone API. Privileged
states remain in scope; they are not declared impossible or complete. Runtime
acceptance of those states belongs to a disposable root-owned guest later.

## Primary review

Reviewed cached official Intel SDM September 2026 revision 093, Volume 2B
253667-093US: POPF/POPFD/POPFQ pp. 4-407–4-410 (especially Table 4-20 and the
64-bit pseudocode), PUSHF/PUSHFD/PUSHFQ pp. 4-528–4-530; Volume 1 sections
3.4.3.2–3.4.3.3 (DF, TF, IOPL, AC including SMAP). Table 4-20 explicitly includes
64-bit operand size for AC/ID; the nearby prose's 32-bit wording alone is not a
reason to mask them out. Official combined source:
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
Local review cache: `/tmp/keel-r4-intel093/v2b.txt` and `v1.txt`.
The full-scope primary audit in
`suite/evidence/full-scope-contract-audit-20261023/dispositions.json` still lists
PUSHF/POPF and their Q forms as physical contracts; this change does not close
that audit or claim 16-bit/compatibility-mode implementations.

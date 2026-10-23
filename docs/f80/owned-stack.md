# Raw principal leaves in an owned legacy x87 region

`x86_x87_ffree0()` through `x86_x87_ffree7()`, `x86_x87_fdecstp()`,
`x86_x87_fincstp()` and `x86_x87_fnop()` have zero arguments/results. IDs732..742
append after root's RDPMC731; all preceding IDs are preserved. They are impure
pinned memory Barriers, baseline x86-64 operations; AArch64 explicitly rejects
them. No frontend mapping is added.

Each emits exactly the named principal instruction. FFREE uses the fixed logical
ST(0)..ST(7) operand. No FLD/FSTP, FWAIT, numerical SSA transport, hidden save,
scratch state, synthesized environment, pointer rewrite or helper call is added.
There is no implicit status snapshot; callers may capture full images using the
existing owned image intrinsics at the next IR position. Ordinary GPR/SIMD state
and FLAGS have no new clobber.

Use the existing [caller-owned region contract](legacy-images.md). The physical
stack may be coherent and nonempty at any TOP. The caller exclusively owns it,
uses no ordinary F80 numerical work or ABI call/return within the region, and
ends the region with an explicit existing save/reset/empty restore before an ABI
boundary. Images, payloads and memory must meet that contract. Every leaf here
requires no pending unmasked x87 exception; masked sticky flags are supported.
No delivery, signal-resume or pending-exception equivalence is claimed for FNOP.
GCM treats these leaves as ownership markers and retains F80 producers/consumers
in their source blocks, including functions without a local image intrinsic.

FFREE changes only the selected physical tag pair ((TOP+i)&7) to empty, retaining
TOP and physical register contents. FDECSTP/FINCSTP change TOP modulo8 by−1/+1
and clear C1, retaining full tags and physical register contents. FNOP performs
its actual architectural opcode with its native environment effects. CW remains
unchanged. For FFREE/FNOP, the existing structural/exception SW mask is b8ff;
for FDECSTP/FINCSTP it is baff, including cleared C1. Raw snapshots retain every
status bit, pointer field, full tag and payload byte as diagnostics.

FIP/FCS/FOP/FDP/FDS follow the actual instruction and host/vendor/mode/feature
behavior. No universal seed-FDP retention or masked-current-FOP result is
synthesized or promised. The independent primary review at root c6e95b80 leaves
opcode/feature pointer obligations unresolved and approves no non-gating fields.
All old finite comparison masks, generators, packets and both Linux/KVM failed
histories remain unchanged. These new leaves do not accept or repair the old
failed recipe gate. New tests must retain exact raw observations, independent
restore-only/transparent defined-pointer controls and separately qualify any
address normalization or native observational equality domain.

Local compiling/byte/effect tests are not native acceptance. Root owns target
execution, complete new immutable proof, review and any later qualification.
All full flags remain false.

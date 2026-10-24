# Raw principal leaves in an owned legacy x87 region

`x86_x87_ffree0()` through `x86_x87_ffree7()`, `x86_x87_fdecstp()`,
`x86_x87_fincstp()` and `x86_x87_fnop()` have zero arguments and results. They
use intrinsic IDs 732..742, appended after RDPMC (731). They are impure
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
TOP and physical register contents. FDECSTP/FINCSTP change TOP modulo 8 by -1/+1
and clear C1, retaining full tags and physical register contents. FNOP performs
its actual architectural opcode with its native environment effects. CW remains
unchanged. For FFREE/FNOP, the existing structural/exception SW mask is b8ff;
for FDECSTP/FINCSTP it is baff, including cleared C1. Raw snapshots retain every
status bit, pointer field, full tag and payload byte as diagnostics.

FIP/FCS/FOP/FDP/FDS follow the actual instruction and host/vendor/mode/feature
behavior. No universal seed-FDP retention or masked-current-FOP result is
synthesized or promised, and the opcode/feature-dependent pointer fields are not
part of the contract. Tests must keep exact raw observations, use independent
restore-only controls for defined pointers, and separately qualify any address
normalization or native equality domain.

Current tests cover compilation, encoding and effect modeling only; native
validation has not been completed.

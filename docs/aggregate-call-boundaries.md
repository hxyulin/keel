# Two-word x86 aggregate wrappers

The x86 backend keeps small, straight-line aggregate wrappers in SSA when the
external System V ABI is identical to a pair of integer words. This removes
argument snapshots and return-buffer copies without inlining an exported
function or replacing a result pointer in target-independent IR.

Admission requires a single block, one direct call, and a complete two-i64
result. Byval layouts must be exactly `[2 x i64]` or `{i64, i64}`. The backend
compares every original argument location with the proposed i128 location,
including register-pair rollback, stack offsets, total stack bytes and stack
alignment. A pair starting at an eight-byte stack offset that would acquire
extra i128 alignment is rejected. The result must use rax and rdx.

The bounded analysis interprets only private stack homes and incoming byval
copies. Every observed word must be initialized. Addresses may only be used
for bounded constant offsets, nonvolatile word loads/stores, and ABI aggregate
transport. External memory accesses, address observation/capture, multiple
calls, indirect calls, explicit operand ABI descriptors, nonlocal control flow,
variadic calls, frame observers and debugger-visible functions fall back to
ordinary lowering. Public symbols, binding, calling conventions and attributes
are retained. Other targets use their existing lowering.

The temporary backend function uses i128 values only after the location proof.
It is not substituted into the public module or used to infer source-level
alias contracts. Unchanged pairs reuse their incoming SSA value; initialized
constant words are packed without redundant zero operations. Constant shifts
by one 64-bit word lower to half moves rather than a variable-shift sequence.

Validation includes negative admission checks, text/native-object parity and
a Clang-built C caller/callee oracle. The oracle checks integer extremes and
512 deterministic word pairs with both allocators, baseline/full selection and
empty register pools. It also checks constant signed and unsigned 128-bit
shifts by 64.

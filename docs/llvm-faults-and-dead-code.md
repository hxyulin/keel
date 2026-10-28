# LLVM memory faults and dead code

Keel preserves potentially faulting accesses on executed paths, even when their
results are unused. Removing a path that cannot execute does not remove an
observable fault. Symbol emission and linker retention are separate decisions.
The LLVM bridge currently preserves neither every Keel memory fault nor every
memory effect before that fault.

## What may disappear

| Case | Keel behavior and intended contract |
| --- | --- |
| Executed ordinary load with unused result | Keep unless a valid `notrap` promise permits deletion. |
| Executed volatile load with unused result | Keep, including with `notrap`. |
| Unreachable block containing a load or trap | Remove the block when control-flow reachability proves it cannot execute. |
| Unreferenced local function containing a load or trap | May remove the function when no emission root reaches it. |
| Public function with no local callers | Keep during compilation; external callers may exist. Final link policy is separate. |
| Local function marked `used` | Emit its symbol; the linker may still discard it. |
| Definition marked `retain` | Emit and request protection from linker garbage collection. |
| Definition assigned a custom section | Placement alone does not imply retention. |
| Undemanded archive member containing `used` or `retain` | Neither attribute alone requests archive extraction. |

Keeping a function does not make every instruction in it observable. Conversely,
a potentially faulting instruction does not make a dead function live.
Constructors, destructors, address references, relocations and assembly references
must participate in reachability. Keel's function cleanup already accounts for
these roots conservatively. It does not currently collect data objects.

The native linkers collect whole input sections, so unrelated symbols sharing
a live section can survive. Retention is not a promise that a function executes,
and dead stripping is not a promise to remove every unused symbol.
See [section and retention rules](c-sections-retain.md).

## Reproduced behavior

On Apple arm64, using Apple Clang 21.0.0 and the current native Keel executable,
the following function faults when called with null through native Keel:

```text
func @fault_probe(ptr) -> i64 {
block0(v0: ptr):
    v1 = load.i64 v0
    v2 = iconst.i64 0
    return v2
}
```

The exported ordinary LLVM load does not preserve this behavior. Both Clang
`-O0` and `-O2` executables returned zero. At `-O0`, emitted LLVM IR still had the
load, but instruction selection omitted it. At `-O2`, the load disappeared from
LLVM IR as well. Disabling optimization is insufficient.

Adding `null_pointer_is_valid` did not prevent deletion at either level. That
attribute changes the validity of address zero; it does not make a read with
an unused result observable. Changing the load to `volatile` produced SIGSEGV
at both levels. Native Keel with full optimization retained ordinary and volatile
loads; an ordinary `notrap` load was deleted.

Separate reachability checks removed an unreferenced local Keel function with
a load and an unreachable block with a trap. A local LLVM function containing
a volatile load disappeared at O2 without retention metadata. With
`llvm.compiler.used`, it survived object emission but disappeared during system
linking with `-dead_strip`. With `llvm.used`, it survived both stages.

## Volatile alone does not preserve the state before a fault

This LLVM example keeps the faulting load but loses an earlier store:

```llvm
define void @ordering(ptr noalias %p, ptr noalias %q) {
  store i64 1, ptr %q, align 1
  %unused = load volatile i64, ptr %p, align 1
  store i64 2, ptr %q, align 1
  ret void
}
```

Clang O2 removed the first store. A native test used a shared `mmap` allocation
for `%q`, forked a child, passed null as `%p`, and inspected the allocation after
the child's fault. Equivalent native Keel code left value 1; the LLVM volatile
variant left value 0. Both children terminated with SIGSEGV. This tests process
behavior on this platform, not portable C signal-handler semantics.

LLVM explicitly permits reordering volatile operations relative to ordinary
operations. Volatile is therefore evidence for retaining the access itself,
not a complete implementation of Keel's fault boundary. Blanket volatile
lowering would also prevent otherwise legal read elimination and add stronger
access-count requirements to successful executions.

## Recommended bridge contract

Keep Keel's existing distinction between ordinary potentially faulting accesses,
`notrap` promises, and volatile accesses. Do not weaken it to make the exporter
appear exact. LLVM import can refine behavior outside LLVM's defined domain;
Keel export cannot silently turn a defined Keel fault into LLVM undefined behavior.

For a fault-preserving LLVM path, first qualify an opaque native access helper
with conservative memory effects. The helper must perform the access outside
LLVM optimization, and its call must preserve surrounding effects and control
dependence. Do not expose a body or promises that let LLVM recover the same
deletion or reordering. This is a proposed baseline, not an implemented or
performance-qualified lowering. Target-specific assembly with explicit effects
can be evaluated afterward to avoid call overhead.

An ordinary-load path can remain useful under an explicit, enforced LLVM
defined-memory profile. `notrap` alone is not proof of all LLVM memory rules,
including object bounds, provenance, lifetime and alignment. Such a profile must
state its restrictions and report excluded fault behavior; it cannot claim
arbitrary Keel round-trip equivalence.

The differential harness should record successful execution, fault outcome,
state before the fault, compiler symbol emission, and linker retention separately.
Exercise each with dead results, untaken branches and dead local functions.
Do not use `used` or `retain` to repair instruction-level semantics.

The bridge currently rejects custom sections, `retain`, lifecycle registration,
and imports of LLVM's reserved retention globals. These are diagnosed coverage
gaps. Function `used` export already maps to `llvm.compiler.used`; importing that
output is still unsupported. Unsupported constructs must be reported as rejected,
even when they occur in definitions that could later be removed. A future
optional cleanup before export must retain validation and emission roots.

## Evidence and references

The investigation artifacts are in the ignored directory
`target/fault-investigation`: LLVM and Keel inputs, optimized output, executables,
`results.json`, and `ordering-results.json`. These cases were run during this
investigation; broader backend qualification remains open.

- [LLVM volatile accesses](https://llvm.org/docs/LangRef.html#volatile-memory-accesses)
  specify access preservation, target-specific faults, and ordering limits.
- [LLVM retention globals](https://llvm.org/docs/LangRef.html#the-llvm-used-global-variable)
  distinguish `llvm.used` and `llvm.compiler.used`.
- [LLVM undefined behavior](https://llvm.org/docs/UndefinedBehavior.html)
  explains why an invalid ordinary access does not promise a hardware fault.
- [Existing scoped-contract design](llvm-contracts.md) covers pointer facts and
  accessibility preservation across calls.

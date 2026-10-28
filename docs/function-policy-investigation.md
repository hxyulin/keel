# Function policy scheduling investigation

The failed kernel links are optimizer failures. The investigation isolates four
problems in the function-policy implementation from `55f097ac`. No compiler
behavior is changed by this checkpoint.

The [evidence ledger](../suite/linux-kernel/audit-policy-investigation.json)
records reduced checks, input/object hashes and controlled kernel-unit results.
The optimizer sources tested here are unchanged from that benchmark revision.

## Cold selects a pipeline that never inlines

`function_policy::selected` implicitly replaces an optimized cold function's
invocation pipeline with Size. That pipeline has no inlining pass, so even a
small helper returning a known predicate stays opaque:

```c
extern void absent(void);
static int predicate(unsigned long x) { return x == 8; }
__attribute__((cold)) void test(void) {
    if (!predicate(8)) absent();
}
```

The reduced IR [fixture](../suite/linux-kernel/policy-regressions/cold.keel)
reproduces this. Full optimization without cold removes the impossible call.
With cold, the call remains. `mm/slab_common.i` has the same shape:
`is_power_of_2(8)` guards `__compiletime_assert_325`.

Cold is an execution-frequency hint. Treating it as an unconditional replacement
with the current Size pipeline removes optimization Linux needs for linking.
Cold costs, placement and growth preferences should not erase that cleanup.

## Explicit optimization requests also block helper inlining

Giving the reduced caller `optimize(3)` instead of cold still retains the call.
So does `optimize("inline-functions")`. The inliner rejects ordinary calls when
the caller's stored option list differs from the callee's list. An annotated
caller and an unannotated helper necessarily differ, even when the request
should enable optimization.

This is a policy comparison bug, independent of cold's implicit Size preset.
Compare supported resolved policy compatibility, rather than source argument
lists. Preserve explicit noinline, optnone, disabled-optimization and target/ABI
boundaries. An optimization-effort request must not itself prohibit every
ordinary unannotated helper.

## Mixed dispatch changes Full pass order

The ordinary Full pipeline runs three rounds of inlining followed by cleanup.
Cleanup makes some helpers eligible for the next round. The scoped dispatcher
instead counts inliner occurrences, runs them all first, then runs the local
passes with the module hooks hidden. That transformation is not equivalent.

The [ordering fixture](../suite/linux-kernel/policy-regressions/rounds.keel)
contains a helper with 80 unused constants, above the current inlining size
threshold. Its parameter controls an impossible call. Normal Full cleanup
shrinks the helper, a later round inlines it with a constant argument, and the
bad call disappears. Adding an unrelated public optnone function activates the
scoped dispatcher. Every inliner runs before the helper shrinks; the caller
still calls the helper afterward, and the unresolved reference remains.

Per-function schedules must preserve pass dependencies and round boundaries.
An unrelated annotation must not reorder the inherited Full schedule of an
ordinary function.

## Module cleanup is disabled too broadly

`LocalPass` hides every module hook. Besides avoiding ABI rewrites of protected
functions, this disables constant-argument propagation, unused-function deletion
and other module work throughout the translation unit.

The [dead-local fixture](../suite/linux-kernel/policy-regressions/dead-local.keel)
is independent of inlining: an unreachable local function calls an absent
symbol. Normal Full removes it. An unrelated optnone function keeps it in the
emitted module. A separate precleaned ordering control shows that even when the
entry function is successfully simplified to return, its unused helper remains.

Passes need individual scope rules. Reachability analysis can inspect protected
bodies without rewriting them. It must retain exported, escaping and
assembly-referenced roots. Constant-argument propagation needs guarded callee
candidates and must inspect all callers. ABI-changing passes need stronger
caller/callee checks. Omitting every module pass is not an adequate substitute.

## Kernel controls

The replay covered the eight units below plus the saved `mm/slab_common.i`
witness. The union of their original O3 bad references is exactly the 21 names
from the failed full-kernel O3 GNU link:

- `arch/x86/kernel/setup.i`: disabled EFI calls.
- `arch/x86/kernel/alternative.i`: disabled mitigation table boundaries.
- `arch/x86/kernel/fpu/signal.i`: disabled compatibility conversion helpers.
- `mm/vmscan.i`: compile-time assertion helpers.
- `mm/memory.i`: disabled huge-page helpers.
- `mm/page_alloc.i`, `mm/sparse.i`, `mm/slab_common.i`: assertion helpers.
- `lib/maple_tree.i`: disabled lockdep helper.

For eight fetched inputs, fresh unoptimized IR was generated with the same
kernel target/features and executable binding. Every variant was compiled to
an x86-64 ELF object. GNU nm checked actual undefined symbols, not unused IR
declarations. The saved slab control supplies the ninth unit.

Removing cold metadata from declarations and definitions eliminates all 21
names in these O3 unit controls. Adding one unrelated optnone function back
reintroduces bad references in several units. These controls intentionally edit
IR for diagnosis. They are not a fix, full kernel relink or boot qualification.
No missing 32-bit implementation or additional instruction encoding should be
added to satisfy these references: the disabled paths should disappear.

Without cold metadata, O2 still leaves problematic references in alternative,
vmscan and memory. Os does so in additional units. These ordinary pipelines
therefore have kernel optimization limits independent of the new policy
regression. The earlier qualified full pipeline cannot establish their support.

## Repair order

1. Preserve inherited pass order and inline/cleanup rounds under mixed policies.
2. Replace blanket module-hook suppression with pass-specific scope guards.
3. Separate cold frequency/size preferences from disabling useful optimization.
4. Replace raw option-list equality with resolved inlining compatibility.
5. Establish sufficient bounded helper inlining and cleanup for O2/Os separately.

Keep optnone/O0 bodies protected and retain recursion, interposition, ABI,
stack and growth limits. The repair belongs in the optimizer manager and
inlining policy, not instruction encoding or register allocation.

First validate the reduced checks, including their unannotated controls and
unchanged protected bodies. Then recompile and link the full pinned corpus,
package and boot it, and only then repeat performance measurements. Do not
suppress undefined symbols, add stubs, or force every function to O3.

## Reproduction and validation

```sh
cargo build -p keel
python3 suite/linux-kernel/check-policy-regressions.py \
  --keel target/debug/keel --out /tmp/policy-regressions.json
```

On the investigated source this explicit check exits 1: five regression cases
fail, while three unannotated controls pass. It is separate from default Python
unit-test discovery. Its expected final behavior after repair is exit 0 with
all cases passing. Temporary IR outputs are removed automatically.

The local compiler build and the existing five optimizer function-policy tests
passed this session. All 14 kernel-fixture Python unit tests passed. Forty
additional object-emission controls cover eight fresh kernel inputs at five
policy/preset variants each. The reduced checks and the saved slab witness
provide the remaining controls. Full workspace tests were not repeated because
no Rust implementation changed. No runtime or performance claim is made here.

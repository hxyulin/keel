# Ordered integer reductions

`adjacent-diff` and `run-balance-predicate` are explicit experiments, available
through `keel opt -p` or an explicit `compile`/`cc` pipeline. They are not in
Fast or Full defaults. They do not change register allocation or weaken the
generic vectorizer's restrictions on multiple potentially faulting streams.

Current application validation is recorded in
[the ARM applicability report](ordered-real-apps-20261021.html). Neither pass
activated on any of the 44 Lua, QuickJS, and SQLite translation units. Both
activated on their separate kernel controls. Adding the passes produced
identical application IR, objects, and executables compared with the same
scalar-cleanup pipeline. Kernel speedups therefore do not establish a benefit
for these applications, and neither pass is promoted into Full.

Recheck applicability with a frozen compiler and prepared application sources:

```sh
python3 suite/fast_opt_locked.py target/ordered-lock.jsonl \
  python3 suite/ordered_reductions_validate.py \
  --keel /absolute/path/to/keel --inputs target/suite \
  --output target/ordered-activation-new
```

Use a fresh output directory. The validator discovers the compiler's actual
default schedule from its report and compares each pass, their combination,
and scalar cleanup alone. It checks kernel activation separately from real
application activation; it performs no native timing.

`adjacent-diff` recognizes a bounded rotated loop summing absolute differences
between consecutive signed i32 elements. It discovers the pointer and bound
through dataflow, including ordinary two-argument functions. Both the merged
body/latch shape emitted by C lowering and a separate latch are supported.
Other instruction sequences, extra memory effects and volatile accesses are
rejected. The first iteration reads a[1] before a[0]. Later iterations reuse
successful ordinary reads and load new samples in ascending order, exactly two
i32 elements at a time. A runtime guard on n*4 and base+n*4 retains the original
scalar loop if address arithmetic could wrap. Empty and single-element inputs
perform no reads; the scalar tail does not overread. Differences are widened
before subtraction, so INT32_MIN/MAX can produce magnitude 4294967295. Vector
and final sums use wrapping i64 arithmetic. Practical native tests cannot cover
a complete u64 sum overflow from a zero seed; reassociation additionally relies
on modular arithmetic and the existing vector-add semantics.

The window is expressed as an i64x2 shuffle [1,2] of previous/current values.
The backends retain this narrow shuffle natively as NEON EXT #8 or SSE2 SHUFPD
1. Equal-sized floating vector shuffles also use this bit-preserving path;
other shapes and lane lists retain general legalization. Exact-low64 load
covers read eight bytes directly into NEON/SSE2 registers only when the sole-use
splat/bitcast/widen chain cannot observe the upper eight bytes. Constraints,
distinct debug lines and shared values prevent those covers.

`run-balance-predicate` recognizes a scalar recurrence that credits a positive
i32 input through unsigned extension, otherwise debits 17 down to zero using
an unsigned comparison. Balance and total both remain loop-carried. Only pure
wrapping arithmetic moves, and the single input load stays in place. New
selects inherit their corresponding branch locations. General parity/multiply
recurrences such as Collatz remain excluded.

Full's separate `adjacent-abs` pass converts a private scalar absolute-value
triangle without moving memory. Its sign comparison, negation and select can
use a bounded ARM CMP/CSNEG cover for i32/i64. This preserves wrapping MIN,
custom allocation constraints, source boundaries and live inputs. Functions
containing inline assembly keep their original NZCV behavior.

To compare an experiment without changing defaults, the standalone runtime
runner accepts `--baseline-keel` and `--full-passes`. The latter overrides only
current Full. Frozen Full, None, Fast and Clang keep their original flags;
correctness runs before randomized native measurements. Keep architecture,
array length, raw variation and unchanged-code controls separate when judging
whether a pass should enter a default pipeline.

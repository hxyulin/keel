# Performance changes

This guide says which measurement decides a change, what result accepts it,
and what the commit records. The commands are documented in the
[suite guide](../../suite/README.md).

## Gate order

Apply the first gate that fits the change. A later gate does not replace an
earlier one.

1. **No intended change to generated code.** Refactors, cleanups and
   compile-time work must leave every suite object byte-identical. Build the
   base and changed compilers and run
   `cargo suite codegen-diff --baseline <base keel> --candidate <keel> --expect-identical`.
   It compiles every suite translation unit for AArch64 and x86-64 in the
   `keel` and `keel-O` configurations. Identical output needs no runtime
   measurement. Add a crate test when the corpus does not reach the code.
2. **Compile time.** Measure retired compiler instructions and peak resident
   memory on the same corpus, with output still identical.
   `codegen-diff --icount` sums retired instructions per target and
   configuration. It does not record memory: measure peak RSS for the largest
   translation units with `/usr/bin/time -l` on macOS or `/usr/bin/time -v` on
   Linux. Retired instructions vary little between runs, but repeat the run
   before reading a difference under one percent. Wall time on a shared machine
   does not decide a compile-time change.
3. **Runtime.** Changes to generated code are decided by
   [`cargo suite ab`](../../suite/README.md#timing-two-compilers) on the
   application benchmarks, under the acceptance rule below.
   `suite/asm/sequence_bench` only screens candidates: it ranks x86
   instruction sequences in a loop and cannot accept or reject a compiler
   change. A sequence that wins there can still lose in an application,
   through code size, alignment or register pressure.

Before timing, check that the change reaches the workloads.
[`cargo suite codegen-diff`](../../suite/README.md#comparing-generated-code)
lists the functions whose code changed, and
[`cargo suite stats`](../../suite/README.md#pass-and-backend-statistics) shows
which optimizer passes change which functions, and the backend's spill,
reload and move counts. A change that touches no hot
function will measure as neutral. To measure one
[cover](../compiler/design.md#covers), use the same binary on both sides:
`--candidate-isel all,-<cover>` on `codegen-diff` and `ab` drops that cover
from the candidate, and the `ab` trailer records it. Batch changes that are each expected to
stay inside the control spread and measure them together.

## Acceptance rule

The acceptance hosts are an AMD Zen 5 x86-64 Linux desktop and an Apple M5 Max.
The metric is cycles. On Linux `cargo suite ab` counts user-mode cycles
(`cycles:u`) with `perf stat`. On macOS `/usr/bin/time -l` counts the whole
process, user and kernel. The trailer's protocol line names the metric used.

Run `cargo suite ab` on each acceptance host whose target's generated code
changes. Use at least the default four placements. On Linux, pin the benchmark
and the controller to separate idle CPUs with `--cpu` and `--controller-cpu`.
Each run gives one of three verdicts:

- **Neutral:** the cycles geomean over workloads is inside the
  identical-control spread. The control is a copy of the baseline image, timed
  in the same blocks. A speedup smaller than 0.5% is also neutral, even when
  it is outside the spread. A larger speedup must exceed the spread to count
  as accepted.
- **Reject:** one workload is more than 1.5% slower at every placement, and its
  smallest slowdown is larger than its control's largest deviation. A geomean
  slowdown outside the control spread is also a reject.
- **Accept:** the geomean is -0.5% or better and no workload is rejected.

A runtime change lands when at least one host accepts it and no host rejects
it. A neutral result is not a runtime gain: a neutral change needs another
reason, such as simpler code or smaller output, stated in the commit.
Size or simplicity changes may land when every host is neutral. Record the
measured size or maintenance benefit and run the same slowdown checks; smaller
output does not excuse a rejected runtime result. Compiler-time gains still
require the separate instruction-count and RSS measurements above.

`cargo suite ab-report <report.json>` re-evaluates a saved report with the
current rule and prints its original verdict and a current verdict/trailer.
It does not retime workloads or change the input file. Keep the original
measurements and identify a policy re-evaluation in the commit body.

`cargo suite ab` prints the verdict and saves the plan, raw runs and report
under `target/keel-suite/ab/runs/`. Those files stay out of Git; the commit
trailer is the record.

## Commit trailer

The body of a runtime `perf` commit ends with the trailer block that
`cargo suite ab` prints, one block for each host that ran, before any
`Co-Authored-By` lines. For example, the rerun of an x86 integer-idiom change:

```text
AB-Baseline: f4b924a1ef
AB-Candidate: 8a50764d2b
AB-Host: AMD Ryzen 9 9950X3D 16-Core Processor, Linux 7.3.0-5-cachyos
AB-Protocol: keel-O, 15 workloads, 4 placements x 3 rounds, ABBA with an identical-image control, cycles:u
AB-Result: geomean +0.07%, control spread ±0.31%, worst quickjs/fib +0.51% (placements +0.01% to +0.96%)
AB-Verdict: neutral
```

| Key | Content |
| --- | --- |
| `AB-Baseline`, `AB-Candidate` | The revision or binary on each side |
| `AB-Host` | CPU model and operating system |
| `AB-Protocol` | Configuration, workload count, placements and rounds, metric |
| `AB-Result` | Cycles geomean, control spread and the slowest workload with its placement range |
| `AB-Verdict` | `accept`, `neutral` or `reject` |

A rejected experiment records its result the same way. The tip commit of the
experiment branch carries the trailer with `AB-Verdict: reject` and a sentence
on what was tried.

A compile-time `perf` commit has no trailer. Its body states the codegen-diff
result (all objects identical) and the change in retired instructions and peak
RSS.

# Executable page backing and runtime measurements

On x86-64 Linux (Ryzen 9 9950X3D), byte-identical QuickJS executables showed
large, repeatable timing differences associated with their executable backing.
This is a benchmarking limitation, not a compiler speedup or a diagnosed RAM
fault. The exact hardware indexing mechanism remains unresolved.

## Observations

The investigation used the same program, input, output, environment and CPU,
with byte-identical executables.

* Affinity and scheduling priority did not remove the approximately 28% gap.
  Four fresh tmpfs copies also had substantially different runtimes. Selecting
  tmpfs or repeatedly launching one inode is insufficient control.
* With process-local ASLR disabled, all images loaded at `0x555555554000` and
  `JS_CallInternal` at `0x555555565960`; the difference persisted.
* Privileged, read-only pagemap observations found stable but distinct physical
  code frames for the files. A diagnostic loader verified every RX byte, then
  substituted identical code from another backing at the same virtual address.
  All three executable identities took about 0.83 billion user cycles with
  either fast backing and about 1.09 billion with the slow backing. The effect
  transferred with the code backing.
* A second experiment held two private, locked 16 MiB page pools. One selected
  set of 317 frames within a sparse 21.68–21.74 GiB address envelope took
  1.008 billion cycles in sorted assignment and 0.760 billion when shuffled.
  The frame set and code bytes were unchanged; code-to-frame assignment changed.
  Other sets had small or opposite ordering effects. Neither a broad physical
  RAM band nor contiguity alone classified all fast and slow cases.
* Linux's Zen 5-specific events reported roughly 3.4 times as many
  decoder-dispatched operations and 54% more op-cache misses for the original
  slow backing, with essentially equal retired instructions. These speculative
  front-end counters are clues, not proof of a particular cache conflict or
  direct attribution of stalled cycles.

These are finite, single-workload diagnostic results. Mapping interventions also
change file-VMA metadata, and page permutations can change VMA coalescing.
Generic cache-event aliases and model-specific raw events have different
definitions and must not be mixed. Physical address envelopes above are sparse
sets, not declarations that every address in those ranges is occupied or slow.

## Requirements for a controlled comparison

1. Keep executable bytes, code virtual addresses, workload and scheduling
   explicit. Use normal user application credentials and the same fixed core.
2. Retain physical backing across variant swaps. A persistent private file and
   read-only locked mapping can hold pages while their contents are rewritten
   between launches. Verify actual PFNs before and after writing; do not assume
   overwriting a pathname retains frames.
3. Verify the live child's code-frame assignment and loaded code bytes before
   measuring. Different variants must load their **own** code. Mapping the
   baseline's code into an optimized executable would invalidate the comparison.
4. Cross variants over several predetermined frame sets and assignments. Do not
   choose each variant's fastest placement. Retain identical-byte controls and
   every placement result, including unfavorable ones.
5. Treat physical sets and timing blocks as replication units. Repeated launches
   of one backing measure its conditional repeatability, not placement robustness.
6. Define the estimand. A barrier after loader preparation measures post-loader
   application execution, not whole-process startup or ordinary deployment with
   randomized layout. Report those limits and keep observer runs separate.

The controlled harness must first pass its identical-byte controls. The page
investigation does not authorize enabling an optimization by default.

## Controlled retest

The fixed-backing harness in `suite/integration/page-backing/` passed all eight
workloads' predefined identical-byte control tolerances on x86-64 Linux. The full run
used eight disjoint retained physical frame sets, both sorted and shuffled
assignments, and six balanced measured blocks: 3,456 correct launches including
gates and warmups. Every launch checked its own code bytes, physical frames,
virtual address, credentials, scheduling and output.

Null rematerialization showed favorable finite results on Lua string (2.38%
less wall time), QuickJS string (0.90%), Lua float (0.69%) and SQLite (0.53%).
Lua fib/sort and QuickJS fib/sort were inconclusive. In particular, QuickJS fib
varied from a ratio of 0.833 to 1.469 across frame-set/assignment estimates;
passing identical-byte controls does not make different code placement-neutral.

These are warm-page, fixed-virtual-layout, post-loader application measurements,
with intervals conditional on the sampled sets and timing blocks. They exclude
loader preparation, use a private preload helper, and have no multiple-comparison
correction. They establish a useful controlled comparison, not ordinary
deployment performance or a general default-setting win. Compiler defaults remain
unchanged.

## References

* [Kernel pagemap and physical page flags](https://docs.kernel.org/admin-guide/mm/pagemap.html)
* [Linux Zen 5 instruction-cache event definitions](https://github.com/torvalds/linux/blob/v6.14/tools/perf/pmu-events/arch/x86/amdzen5/inst-cache.json)
* [Linux Zen 5 decoder event definitions](https://github.com/torvalds/linux/blob/v6.14/tools/perf/pmu-events/arch/x86/amdzen5/decode.json)

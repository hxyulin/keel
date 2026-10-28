# Measuring small userspace timing changes on Linux

Use this procedure for resident-input, steady-state CPU benchmarks where a
change of a few percent matters. State the measured boundary first. A compiler
scan, a C-to-IR call, process startup and a complete build are different
measurements. Use separate experiments for cold inputs or startup costs.

## Freeze the experiment

- Record the source revision, overlays, toolchain, build flags, target, CPU,
  kernel, binary SHA256 and input SHA256. Build every image before timing.
- Keep the harness and target options identical. Compile instrumentation into
  separate diagnostic images. Keep diagnostic counters out of timed code.
- Give each input snapshot an immutable filename. Generated headers can change
  between build commands with the same flags. Include the preprocessed content
  hash in a capture filename, then deduplicate by content within each workload.
  Verify input hashes before and after timing.
- Select workloads before reading performance results. Keep failures out of
  successful-compilation timings and report them separately. Include realistic
  misses when measuring a lookup table, not just its known keys.

Acquire a shared performance lock before changing the benchmark executable or
starting measurement. Confirm that no other build or profiler is running.
Record process activity between runs and abort if competing work starts.
Release the lock when the controller exits.

## Hold executable and input placement stable

ASLR and CPU affinity alone did not eliminate a timing shift between copies of
an identical executable. A different filename can also change process layout.
An identical-image control is necessary even with these controls.

1. Choose one executable path and inode for every image and label. Keep argv
   and the environment identical except for intended workload arguments.
2. Read the ELF program headers and calculate the page-rounded extent of all
   file-backed `PT_LOAD` segments across all images. Debug sections need not be
   resident. Give the executable file enough space for the largest image.
3. Open a persistent read-only `MAP_SHARED` mapping of that extent in the
   controller. Touch each page and `mlock` it. Check the return value and the
   process's `RLIMIT_MEMLOCK`. Do not claim locking if it failed.
4. Between children, overwrite the same executable file in place. Do not rename,
   replace or truncate it. Never rewrite it while a child is executing. A writable
   mapping or a retained writable descriptor can make `execve` fail with
   `ETXTBSY`; close the writable descriptor before launching a child.
5. Keep every immutable input file mapped and prefaulted in the controller.
   Children map those same files and retain their mappings until they exit.
   Mapping and UTF-8 validation belong before warmup when measuring scanner CPU
   time. Input maps may be larger than the available locking allowance; record
   whether they are locked or merely prefaulted.
6. Disable ASLR only for each benchmark child and set its CPU affinity:

   ```sh
   setarch x86_64 -R taskset -c <measurement-cpu> <fixed-executable> <arguments>
   ```

   Use the machine's architecture rather than assuming x86-64. Check that the
   command succeeds. Run the controller and inspection helpers on another CPU.
   Avoid competition on the measurement CPU's SMT sibling. Record the CPU/cache
   topology; affinity does not isolate a core from interrupts or shared caches.

The fixed file-backed mapping keeps executable placement comparable across
images. It does not make corresponding functions have identical virtual
addresses: changing code can change function sizes and layout.

## Check backing pages around the timed region

Use an explicit child/controller handshake:

1. Child maps inputs, performs warmup passes, reports `ready`, then blocks.
2. Controller reads its own and that child's `/proc/<pid>/maps` and `pagemap`.
   Compare resident child executable-text and input pages with the controller's
   corresponding file offsets. Retain parent frame fingerprints for later runs.
3. Controller releases the child. The child performs one further untimed pass
   to restore the hot working set after page inspection, then measures passes.
4. Child reports `done` and blocks again. Controller repeats the page check,
   then lets the child exit.

Real page-frame numbers require sufficient privilege on many Linux systems.
Reject redacted frame numbers rather than treating zeros as a match. Inspect
only the benchmark controller and its children. Do not change global security
settings to expose page frames.

Verify that parent frames remain stable across the whole experiment, resident
child file-backed frames match them, and input virtual addresses are stable for
each workload. Record missing or nonresident pages as well as mismatches.
Report the actual coverage rather than claiming to have checked all process
memory. Count consecutive frame runs if physical adjacency is of interest.

`mlock` keeps pages resident; it is not a promise that physical frames cannot
migrate. These are endpoint checks, not continuous migration tracing. Heap,
stack, relocated private data and dynamic-library pages remain outside the
file-backed-text/input comparison unless explicitly measured. Consecutive
physical frames do not prove a speed benefit by themselves.

## Balance runs and include a null control

Use at least three `A B B A` blocks for each workload and candidate. Repeat the
same protocol with both labels referring to the exact same binary bytes. Use
the same executable path, input order, warmup, gates and measured passes for
that identical-image control.

The lexer experiment used two warmup corpus passes, one extra settling pass
after inspection, and three measured passes per process. Three ABBA blocks give
six processes per image. Another experiment used five warmup and six measured
passes. Pick warmup and pass counts that fit the workload, document them, and
keep them fixed between candidates. Rotate TU order consistently when measuring
a corpus. A warmup count alone does not prove thermal or frequency stability.

Each process contributes its median pass time. Compare medians of the process
medians, and also report reductions within each ABBA block. Treat processes as
the independent comparison units. Report the null-control shift alongside the
candidate's change; do not silently subtract it or call it a confidence
interval. Preserve raw passes, block order, page-check results and activity logs.

## Collect hardware counters separately

Use a separate run with the same placement and warmup controls. Enable user-mode
events after warmup with `perf stat --delay=-1 --control=fifo:<control>,<ack>`.
Wait for acknowledgement before releasing the child, disable after `done`, and
wait for that acknowledgement too. Record enabled/running time and reject
unsupported or multiplexed events when they would invalidate a comparison.

Useful events include cycles, instructions, branches, branch misses and supported
cache events. Counter coverage can include a settling pass and per-pass
setup/cleanup even when the elapsed-time timer excludes them. State that
boundary explicitly. A lower branch count does not imply fewer mispredictions;
a cache-miss count alone does not establish a memory-bandwidth bottleneck.

This procedure changes per-process affinity, ASLR, mappings and locking. It does
not require global cache drops, VM tuning, huge-page settings or CPU-frequency
changes. Record background activity and frequency/thermal behavior when they
remain plausible explanations for a result. Repeat on another workload or
machine before generalizing a small gain.

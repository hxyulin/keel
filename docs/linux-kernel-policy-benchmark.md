# Kernel compilation checkpoint

The captured-input compilation benchmark completed at compiler revision
`55f097ac`. Fresh Keel O2, Os and O3 objects do not currently link into the pinned
kernel, so there is **no current Keel runtime measurement** from this run.
Both Keel's linker and GNU ld reject them. Earlier boot-qualified images remain
historical evidence, not substitutes for these builds.

The [measurement ledger](../suite/linux-kernel/audit-policy-benchmark.json)
records every compilation sample, object digests, configuration hashes, linker
failures and independent GCC boot controls.

## Object emission time

The input set contains 484 preprocessed C files and 22 preprocessed handwritten
assembly files. Saved assembly generated from those same C files is excluded.
Both compilers process exactly the same captured inputs. Configuration and input
hashes are checked before compilation. Outputs are checked for determinism.

Compilation ran on an AMD Ryzen 9 5950X Linux workstation, with four concurrent
compiler processes. Keel was built in release mode; GCC is 14.4.0. Each of the
four configurations receives one excluded full warmup and three measured runs.
Configuration order reverses between rounds. The existing kernel instruction
policy, kernel address model and disabled optional unwind tables apply to Keel.
GCC retains the captured target, ABI and safety flags, replacing only its
optimization preset, input/output paths, dependency generation and save-temps.

| Configuration | Median wall time | Measured range |
|---|---:|---:|
| GCC O2 | 36.687 s | 36.055–37.003 s |
| Keel O2 | 10.693 s | 10.491–10.711 s |
| GCC Os | 34.366 s | 34.170–34.514 s |
| Keel Os | 10.424 s | 9.971–10.686 s |

These are object-emission times, **not successful kernel build times**. They
exclude preprocessing, host-tool builds, objtool, archive construction, linking,
packaging and compiler bootstrap. Keel's missing required optimization prevents
interpreting these numbers as an end-to-end compiler speed advantage.

The `.i` files were originally preprocessed under the pinned GCC Os configuration.
Preprocessor choices, including optimization macros, stay frozen for both
measured presets. This is a controlled replay, not raw-source Kbuild O2 versus
Os. Warm filesystem caches, four jobs and three repeats also bound the result.

## Link and runtime gate

Fresh objects replace the corresponding files in a disposable qualified input
tree. Objtool runs without nonzero exits, although the existing warning backlog
remains. Archive member order is recovered from the reference thin archives;
every rebuilt regular archive's payload hashes are checked against its ordered
object files. No old archive payload is accepted merely because its basename
matches. The separately recorded external boot/vDSO stages remain external.

GNU ld successfully links both new GCC variants. Packaging produces a
1,119,232-byte O2 bzImage and a 926,720-byte Os bzImage. Both pass the VFS,
memory/protection, fork/COW, wait and pipe fixture under QEMU 11.1.2 TCG,
with the exact success markers and exit 33. These are correctness controls,
not runtime performance measurements.

Keel O2, Os and an additional unmeasured O3 diagnostic build each emit all 506
objects but fail final linking. GNU ld reports respectively 28, 72 and 21 unique
undefined names, including compile-time assertion helpers and disabled-feature
entry points. Keel's linker also fails. No stubs, reference C replacements,
undefined-symbol suppression or manually corrected production IR are used.
No Keel bzImage is packaged or booted, and no KVM performance ratios are claimed.

One controlled diagnostic isolates part of the regression introduced by the
function policy schedule. At O3, `mm/slab_common.i` retains an undefined
`__compiletime_assert_325` reference. Removing only cold attributes from its
unoptimized IR before applying the ordinary full pipeline removes that
reference. This edited-IR control is diagnostic only; it is not a qualified
compiler output or runtime benchmark. Other unresolved names still need their
own witnesses.

Before repeating runtime measurements, restore the required inlining, constant
propagation and unreachable-code cleanup for independently optimized functions.
The successful workspace tests did not cover this integration requirement.
Repairing it should include a kernel link regression, followed by fresh object
compilation, packaging, boot checks and then the paired KVM workload suite.

The [follow-up investigation](function-policy-investigation.md) isolates the
scheduling, inlining-boundary and module-cleanup failures with reduced probes
and accounts for all 21 O3 undefined names in diagnostic unit controls.

## Reproduction

Prepare the pinned reference build and captured manifest as described in the
[kernel fixture](../suite/linux-kernel/README.md). Use a release compiler:

```sh
cargo build --release -p keel
python3 suite/linux-kernel/compile-bench.py \
  --source /path/to/linux-6.12 --build /path/to/reference/build \
  --manifest /path/to/inputs-rebuilt.json --keel "$PWD/target/release/keel" \
  --out /path/to/new-measurements --jobs 4 --repeats 3
python3 -m unittest discover -s suite/linux-kernel -p 'test_*.py'
```

The output directory retains one object set per configuration, the replay
commands and raw measurements. Keep small evidence files and remove disposable
objects and packaging trees after a run. The timings do not include the source
snapshot's Rust build.

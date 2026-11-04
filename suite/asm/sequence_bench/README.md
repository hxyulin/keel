# Native x86 sequence measurements

This harness screens x86 instruction sequences for compiler experiments. It
cannot accept or reject a compiler change: those go through `cargo suite ab`
on the application benchmarks, as the
[performance guide](../../../docs/development/performance.md) describes.

`run.py` compares complete instruction sequences assembled by Keel. It requires
Linux x86-64, a C compiler for the timing driver, and permission to lock
192 KiB of memory. It uses no LLVM analysis tools.

```sh
cargo build --release -p keel
python3 suite/asm/sequence_bench/run.py --keel target/release/keel \
  --cpu <allowed-cpu> --rounds 20 --sample-ms 10 \
  --output suite/reports/sequence-run-1
```

Choose an idle core and avoid its SMT sibling. `--controller-cpu` pins the Python
controller to a separate allowed CPU. On hybrid processors, run P-cores and
E-cores separately; the report captures Linux's core/atom CPU lists when present.
Repeat with a fresh output path and seed before acting on a small difference.
`--case` selects comparisons; `--unroll` controls loop size.

`--list` prints every case with its baseline and candidate instructions for one
stream, and runs on any host:

```sh
python3 suite/asm/sequence_bench/run.py --list
```

The default runs one and eight streams; repeat `--streams` to select 1, 2, 4 or
8 streams. Use `--unroll` to check sensitivity to loop size rather than treating
a single unrolled loop as an instruction cost.

Mask, sign-extension and arithmetic alternatives assume their arithmetic flags
are dead. CMP/TEST alternatives consume only ZF; CMP/SBB masks consume CF.
The zero-branch and Boolean-inversion cases start from inputs of 0 or 1, and
`zero_setcc32` from a zero-extended 32-bit input. The zero-branch case
alternates a Boolean state; each unrolled operation has its own branch site. It does not model unpredictable
branches. The mask and sign-extension cases add 17 before each operation to keep
their dependency chains changing. These shared instructions remain in the timed
boundary. Signed-load results are masked to a valid table index before the next
lookup. Word/dword cases use a separate aligned 256-entry table with varying
upper bits; only the low byte supplies the next index. The absolute-value case
quickly has predictable branches, with boundary seeds checking signed overflow.
Rotate and some multiply alternatives use a caller-saved scratch register; this
cost is present in their instructions, but live register pressure is not modeled.
None of these cases measures spill costs, cache misses or unpredictable selects.

The driver pins itself to the chosen CPU. It retains three locked anonymous
mappings and swaps baseline, candidate and identical-baseline control bytes into
the same mapping for each placement, with entry offsets of 0, 16 and 32 bytes
within a cache line. Writes use RW permissions and execution uses RX permissions.
The mappings remain alive across cases. Lock failures abort; physical page-frame
migration is not checked. The three placements combine different backing and
alignment, so they do not isolate either effect. Every placement remains a
separate result.

Before measurement, independent modular arithmetic and table-lookup oracles
check zero trips, wrapping seeds and nonzero counts. Separate one-operation
functions check predicates before repeated operations reach a fixed point.
Metrics must agree with the ordinary assembler's instruction bytes, and text
relocations are rejected. The constant case overwrites its operands and is
labelled throughput, including its one-stream form. By default other cases use
one dependent stream or eight independent streams. Byte-table cases use a hot
256-byte permutation table. Wide-load cases use a hot 1 KiB table; the low byte
of each loaded value supplies the next index.

Calibration chooses one trip count per case and stream count. Each sample has
an untimed 1,024-trip warmup, then one function call timed with
`CLOCK_MONOTONIC_RAW`. The measured boundary includes loop control, the call and
checksum. Results are nanoseconds per complete sequence, **not instruction
latencies or hardware cycles**. There are two discarded rounds per placement,
then randomized baseline/candidate/control order within paired rounds. The report
uses the geometric mean paired time ratio and a 95% percentile bootstrap interval
resampling whole rounds. It reports the identical-byte control separately and
does not subtract that control. These intervals describe repeatability within a
run and placement; they do not establish robustness across machines or workloads.

`results.json` retains raw samples, orders, checksums, actual entry addresses,
CPU identity, activity snapshots, source and assembler hashes, driver build
command, exact body sizes and instruction counts. The directory also contains
sources, objects, encoding reports and executable byte blobs. Run artifacts stay
under ignored `suite/reports/`. `cargo suite check asm` includes the runner's
protocol and oracle tests; it does not execute the native measurements.

## Optional TSC timing

Add `--timer dual` to collect ordered TSC ticks and wall time for each sample.
This requires CPUID support for RDTSCP and invariant TSC. The driver checks the
features after pinning to the measurement CPU. It records the CPU vendor,
hypervisor flag, ordering protocol and CPUID leaf 15 frequency when Intel reports
all required fields. An unavailable frequency is zero; ticks remain usable
without converting them to nanoseconds. Invariant TSC ticks are reference time,
**not current core cycles**. Use PMU measurements to study actual core cycles.

Intel endpoints use `LFENCE; RDTSCP; LFENCE`. Other vendors use the more expensive
`CPUID; RDTSCP; CPUID`, rather than assuming identical LFENCE dispatch semantics.
These protocols include compiler memory barriers. Each endpoint captures TSC_AUX;
a change rejects the sample. CPU affinity and stable entry addresses are checked
too. Matching endpoint markers cannot detect an interrupt or migration away and
back between the reads. The runner does not measure global store publication.
The instruction ordering follows the [Intel instruction reference](https://cdrdv2-public.intel.com/789581/325383-sdm-vol-2abcd.pdf).

The TSC interval brackets the function call and includes inner ordering/wrapper
costs. The outer wall interval also includes both TSC endpoint reads. Schema 2
reports retain both durations, separate paired intervals and ticks per sequence.
The driver records 32 empty indirect-call probes with the selected timer. They
show the timer/wrapper floor and are never subtracted from samples. They do not
calibrate the cost of every snippet wrapper or establish an accuracy bound.

Direct TSC reads skip clock conversion layers and can expose finer units. Unit
granularity does not establish timestamp accuracy. These reads do not remove
frequency changes, interrupts, front-end effects, cache state or placement
sensitivity. For millisecond samples, timer quantization is
usually much smaller than execution noise. Linux may already use TSC as its
clocksource; the report records that source. In that case the two durations are
not independent clocks. Repeat the same timer mode, CPU and loop shape, retain
identical-byte controls, and qualify small effects across fresh runs and loop
sizes before changing a compiler default.

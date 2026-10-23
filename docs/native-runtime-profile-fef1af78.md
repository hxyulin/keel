# Current native application profile checkpoint: fef1af78

This round owns native application correctness and current runtime sampling on
LinuxHost, starting from clean main `fef1af784a2179e08fcab49a4995f34a2cea29fe`.
It changes no compiler implementation, feature policy or default. Main and
sibling worktrees are untouched. The separate compile-cost investigation owns
compiler construction and compile timing; its timings are not runtime evidence.

## Frozen identities and raw-source boundary

Artifacts, commands, objects, binaries, source/IR/assembly and profiles are
outside Git at `/tmp/keel-runtime-fef1-r5/` on LinuxHost. Small receipts/scripts are
also copied to that path locally. `campaign.py`, `reference.py`, `profile.py`
and `summarize.py` retain the executable procedure. Command records are
`commands.jsonl`; shared-lock invocations have separate `*-lock.jsonl` files.

The shared release/default-feature compiler is the read-only
`/tmp/keel-compile-fef1-r5/frozen/keel`, SHA-256
`28ccf147d9cc46a4df93bd1eed6684c3b105fd397f63dc32c6d4de30b204e33d`.
The compile worker's exact `git archive` source is
`/tmp/keel-compile-fef1-r5/source`; archive SHA-256 is
`98877ad82562193cc233531ab29b3c6eccfd59106244bb198bf62c324c68702c`.
Its required locked cargo build retained the existing toolchain/default features,
managed target and `CARGO_CACHE_LINKS=0`. This worker did not rebuild the compiler.

The raw-source configuration is the previously qualified
[Linux declaration/layout application checkpoint](provenance/linux-binary128-tc-layout.md):
33 Lua 5.4.7 TUs, nine QuickJS 2024-01-13 TUs and two SQLite 3.46.1 TUs.
The original source roots remain under
`/home/hxyulin/dev/projects/keel/target/suite/`:

- Lua: `lua/lua-5.4.7/src`, excluding `luac.c`; `-DLUA_USE_LINUX`.
- QuickJS: `quickjs/quickjs-2024-01-13`, plus original `quickjs/gen/repl.c`
  and `qjscalc.c`; `-D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"'
  -DCONFIG_BIGNUM -I ORIGINAL_QUICKJS_DIRECTORY`.
- SQLite: `sqlite/sqlite-amalgamation-3460100`; `-DSQLITE_THREADSAFE=1
  -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double`.

All regular files in the retained distributions match their archives (Lua 75,
QuickJS 66, SQLite four). Both QuickJS C arrays regenerate byte-for-byte with
the retained original producer and upstream commands. Archive, source,
generator and generated-array hashes are in `provenance.json`. This compares
the retained distributions; it does not claim new upstream authentication.

Every application object consumes its original raw `.c`, native system headers
and original working directory/configuration. No Darwin IR replay, foreign
preprocessed input, fake header or macro identity override is used. The `.i`
files in `*/includes/` are include-provenance captures only. Per-TU Clang `-M`
and Keel `-E` line-marker dependency lists and their union of 436 file hashes
include native system/resource/application headers actually observed by those
mechanisms. Line markers do not establish a complete file-open trace: headers
that emit no marked tokens may be absent from Keel's list. The Clang list is
not asserted to equal Keel's include selection. Dynamic library resolved paths
and hashes, native compiler identities and driver expansions are retained too.

Keel uses `cc -O --opt-tier full --isel-tier full --regalloc full --target
x86_64-unknown-linux-gnu --cpu-level v3` (and explicit `v1` control), followed
by the unchanged app flags and `INPUT --emit-object -o OBJECT`. Release
instruction legality remains enabled. Clang uses explicit `/usr/bin/clang
-O2 -ffp-contract=off -march=x86-64-v3` (control: `-march=x86-64`), the same
raw-source flags and `-c`. Links use `/usr/bin/clang`, ordinary system libraries
and no LTO. No native tuning, fast-math, PGO or source transformation is used.
Initial Clang objects used default contraction; they were superseded before
correctness or profiling by the explicit contraction-off reference rebuild.
`superseded-initial-clang-build.json` and the command log retain that distinction.

`build.json` freezes every object/executable path and SHA-256;
`*/VARIANT/{nm.txt,disassembly.txt,ldd.txt}` retain linked encoding and library
inspection. Current raw Full IR and separate assembly emission for `lvm.c`,
`quickjs.c`, `sqlite3.c` are in the matching `keel-v{3,1}` directories. These
are current frontend/optimization/lowering artifacts; native execution and
profile PCs refer to the separately retained direct-object linked executables.
No IR replay or unverified source-line identity is used to map sample PCs.

## Correctness before profiling

All four configurations (Keel/Clang, v3/v1) pass the same six workload checks:
Lua fib/sort, QuickJS fib/sort, SQLite `suite/sqlite/queries.sql` and the larger
`suite/sqlite/bench.sql`. Stdout matches across all four variants per workload.
All four also pass the portable Lua `_U=true all.lua` tests to `final OK !!!`
and QuickJS closure/language/builtin/loop/bignum tests. `checks.json` has 30
passing records; its six workload records each contain all four outputs.
This is finite native Linux execution, not complete upstream conformance,
all FP environments or execution on v1-only hardware.

## Sampling and timing qualification

LinuxHost is a Ryzen 9 9950X3D, native Linux x86-64. The shared
`/tmp/keel-perf-20261019.lock` serializes this worker with compiler/build work.
Profiles pin CPU2; its SMT sibling is CPU18. The receipt records load,
per-CPU `/proc/stat` deltas, process activity, powersave governor, current
frequency and machine identity. No governor/sysctl/privilege changes or
installation occurred. The presence of a newer ISA on the host does not
prove deployment on an original v1-only CPU.

Unrelated Trilio/ROS workloads remain active. **Paired runtime timing is
deferred: zero timed pairs and no current Keel/Clang runtime ratios.** Profile
periods, calibration durations and lock elapsed times are not timing results.
A quiet repeat would need at least seven seeded interleaved measured pairs,
with separate workload ratios and no aggregate geometric mean.

Current process-local sampling uses nonprivileged `perf record -e cycles:u
-F 499 --call-graph dwarf,8192 --user-callchains`, separately from any timing.
The initial callgraph request was denied under `perf_event_paranoid=2`;
explicit user-only callchains work without changing host policy. Each workload
and compiler has two independent captures, repeating the original CLI workload
in fresh processes to obtain useful sample counts. The bounded Python driver
is included in the sample population and identified separately from app/system
DSOs. Its subprocesses inherit CPU affinity and perf collection. The same
workload body/arguments and executable identities are used in both repetitions.

The rankings are descriptive current process-local evidence under the recorded
external activity. They do not establish uncontended cost, elapsed-time gaps,
optimizer speedups or causal comparisons of percentages across compilers.
Self leaf-IP samples are primary. Separate self/inclusive reports retain user
callchains, whose completeness is not independently certified; inclusive
percentages are not additive. System-library and unresolved samples remain
in the denominator, and sampled user cycles exclude kernel time. Percentages
weight recorded event periods; raw sample counts are reported separately.

## Current v3 self-function ranking

The following pooled counts combine two independent captures per cell. Shares
are of all recorded user-event periods, including libraries/driver overhead.
The full per-DSO top-function and PC inventories are `rankings.json`; raw
sample records and self/inclusive reports are beside each `*.perf.data`.
These are measured current ranks, not historical substitutions or performance
ratios. Repetition counts can differ by compiler because calibration targets
roughly six seconds of profiling; total periods are not comparable runtimes.

| Workload | Keel leaf samples; leading self shares | Clang leaf samples; leading self shares |
| --- | --- | --- |
| Lua fib | 5,170; `luaV_execute` 84.67%, `luaD_precall` 15.00% | 6,284; `luaV_execute` 81.89%, `luaD_precall` 17.82% |
| Lua sort | 5,869; `lua_geti` 37.72%, `lua_seti` 12.94%, `partition` 12.45% | 5,761; `lua_geti` 43.98%, `auxsort` 15.14%, `lua_seti` 13.03% |
| QuickJS fib | 5,816; `JS_CallInternal` 97.55% | 4,833; `JS_CallInternal` 99.47% |
| QuickJS sort | 5,984; `JS_CallInternal` 44.92%, `JS_Call` 11.53%, `js_array_cmp_generic` 6.65% | 5,850; `js_array_cmp_generic` 69.66%, `JS_CallInternal` 16.18% |
| SQLite maintained queries | 5,652; `sqlite3VdbeExec` 27.79%, `sqlite3BtreeTableMoveto` 5.74% | 3,336; `sqlite3VdbeExec` 23.11%, `sqlite3BtreeTableMoveto` 8.10% |
| SQLite larger queries | 5,750; `sqlite3VdbeExec` 27.92%, `sqlite3BtreeTableMoveto` 7.59%, `dekkerMul2` 5.99% | 5,918; `sqlite3VdbeExec` 23.13%, `sqlite3_str_vappendf` 15.34%, `sqlite3BtreeTableMoveto` 9.91% |

Function attribution differs where Clang inlines/reshapes calls. In particular,
QuickJS sort's `js_array_cmp_generic` versus `JS_CallInternal`/`JS_Call`
distribution is not a direct function-for-function cost comparison. It is a
concrete reason to inspect those boundaries in current IR/assembly. SQLite
is headed by `sqlite3VdbeExec` on both maintained workloads; the old
`sqlite3FpDecode` investigation does not replace this new ranking.

The three primary function evidence bundles are `luaV_execute`,
`JS_CallInternal` and `sqlite3VdbeExec`, all selected from current samples.
They retain original source path/hash and mention lines, current Full function
IR, separate assembly and direct linked-function disassembly, plus current
sample-PC offsets and `perf annotate` output. The mapping manifest is
`function-evidence.json`, and its files are in `functions/`. Source/IR-to-PC
line equivalence is not claimed: direct binary symbols/offsets are the PC
anchor. Full TUs remain available for surrounding declarations/calls.

Examples of the current Keel v3 leaf-PC evidence (pooled repeats):

- Lua fib: `luaV_execute+0x6bce`, 266 samples, 5.15% of all event periods;
  `+0x61`, 248 samples, 4.79%; `+0x4350`, 205 samples, 3.98%.
- QuickJS fib: `JS_CallInternal+0x4ed0`, 135 samples, 2.33%; `+0x68`,
  123 samples, 2.12%; `+0xabe5`, 121 samples, 2.09%.
- SQLite larger queries: `sqlite3VdbeExec+0x1e9`, 59 samples, 1.03%;
  `+0x1cc`, 47 samples, 0.82%; `+0x1cf`, 39 samples, 0.68%.

These sampled instructions nominate inspection sites; sample skid and
surrounding control/data dependencies prevent attributing all recorded cost
to an isolated instruction. No implementation proposal or speedup is accepted
from these percentages alone. The separate gap interpreter owns pass/source
proposals based on these frozen artifacts. LEA/spill candidates are not
integrated or used in this baseline.

Across the two independent Keel v3 captures, the leading fib functions retain
their ranks: `luaV_execute` is 83.29%/86.04%; `JS_CallInternal` is
97.67%/97.43%. For SQLite's larger query workload `sqlite3VdbeExec` is
27.48%/28.36%. Individual capture shares and counts are retained in
`rankings.json`, rather than only pooled totals. Unresolved libc leaf periods
remain material: 8.75% of Keel's maintained SQLite user periods and 10.55% of its
larger-query user periods; these are library/unresolved evidence, not compiler
functions. `sqlite3FpDecode` is 3.65% on the larger workload, behind the current
VM and B-tree leaders.

One bounded v1 fib capture per app/compiler provides a control, without a
v1 timing comparison. Lua Keel has 2,622 leaf samples (`luaV_execute` 85.28%);
Clang has 2,920 (`luaV_execute` 84.88%). QuickJS Keel has 2,904
(`JS_CallInternal` 97.24%); Clang has 2,397 (`JS_CallInternal` 99.54%).
No v1 sort/SQLite profile is claimed; all their v1 correctness checks passed.
The recorded Clang transitive dependency census is for the v3 primary recipe;
v1 Clang include selection was not separately traced.

## Final audit and receipts

`final-audit.json` rechecks 792 frozen identities: compiler, raw TUs, observed
includes, workload files, resolved shared libraries, every app object and
executable, all perf captures, leaf-PC files and reports. Every one matches.
All 28 captures report zero lost samples; CPU18's measured busy fraction is
zero throughout collection. That establishes finite capture integrity and the
observed SMT condition, not absence of all shared-cache/frequency interference.

Final SHA-256 receipt identities:

| Receipt | SHA-256 |
| --- | --- |
| `provenance.json` | `f9ee454fbc476ee10f3504df98d171c8eddf43c7200d6334dde0844982d64a37` |
| `build.json` | `eeaea2ce1db9e266580048758e3abe5d0ff586bda700894ce1e43c17d754b0e0` |
| `checks.json` | `10e160c1af0a6a5888e8443518728fb1c2e0f090a55ac603db8e992191a7552d` |
| `profiles.json` | `7ad2afd72c1486589d4cde6bec0e431f7107d37ded4a50433393c1277b0d6a84` |
| `rankings.json` | `6774919bec983bdd0348376d3807fcf2ddf6944fb72e6b693cc722242afa8c08` |
| `function-evidence.json` | `0b1f563d0ac07518af0279dfd60054706f33c179bac8c32afc836e2e6a5979b2` |
| `final-audit.json` | `b8e7fafa495a026cd40f07456a527978e77fb2ee214eb3732553231064a890f3` |
| `manifest.json` | `e19cc99cc0b3c78db11fb6cd9af8029fca8fa4fa8964396a6388bd3556f0a027` |

`manifest.json` hashes the retained scripts, receipts, selected whole-TU
IR/assembly and extracted function files. Perf capture hashes are separately
in `profiles.json`; objects/binaries are in `build.json`. Files marked
`initial` or `superseded` do not define the final controls. An early
receipt-serialization error and the denied initial callchain command remain
in logs; neither is counted among the 28 completed captures.

Reproduction uses the frozen compiler and original source distributions, not
this documentation commit's compiler. Run `campaign.py prepare`, `campaign.py
build`, `reference.py` (explicit contraction-off rebuild and correctness),
`profile.py`, `control.py`, `summarize.py`, `extract.py`, then `audit.py` under
`suite/fast_opt_locked.py` where compiling/checking/sampling. Inspect host
activity before any new timing campaign. The scripts are specific external
checkpoint artifacts rather than a new generic benchmark API; they must not
be pointed at an existing completed evidence directory without preserving it.
No production source change or Rust regression run is associated with this
documentation-only record. The native application checks and identity audit
are the relevant validation; `git diff --check` also passes.

# Adjacent i32 mask/zero TEST selection

The third pilot in [the native selection audit](native-isel-audit-334d8466.md#3-single-use-i32-constant-mask-compared-with-zero)
is implemented in `042726912a253c3db7a00c230a5da3f3194dd421`, against
`2b69f9fb085d8f9116ef6be8e29abd16f6f465fa`.
This is an x86 backend selection change, with no frontend, optimizer, ISA-level,
ABI, FP, memory-width or default-target change. The maintained
[userspace ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md)
and [GNU vector/ABI](gnu-vector-mode.md) contracts still apply.

`Selection::fold_x86_mask_tests` runs after the supplied selector and before
existing compare/branch/select fusion. It recognizes exact i32 AND with a
literal i32 mask (either input order), one resolved result use, and a following
Eq/Ne comparison against literal zero in the same block. Only intervening
constants are permitted. Existing covers, machine matches, compare-condition
covers and memory-RHS covers on the AND/compare are excluded, as are constraints
on the AND, compare or intervening constants.
Debug functions, variables and instruction locations conservatively retain their
materialized path. Removed/aliased definitions and previously covered source
producers are excluded. Nothing traverses a call, load, store, fence, intrinsic,
raw expansion, FLAGS region or block boundary.

The recipe stores the literal mask bits separately and publishes the resolved,
**unmasked** source as the compare's allocation operand. It records the encoded
zero RHS as already folded, so later folding cannot accidentally remove an
equal-valued source operand. Existing branch/select fusion transfers this leaf
again to its own live boundary. The original source remains live for other
users; a shared masked result prevents the cover. `x86_64::compare` reloads the
source normally and emits only register `TEST r32,imm32`, returning Eq/Ne's ZF
condition to the existing immediate SETcc/Jcc/CMOV machinery. No new flag
lifetime, memory form, narrowed TEST or alternate predicate is introduced.
User loads/extensions/stores remain separate at their original positions and
widths; ordinary spill completion, boolean canonicalization and ABI transport
remain in their existing paths. AArch64 selection is unchanged.

## Focused validation

`crates/keel-codegen/tests/x86_mask_test.rs` covers zero, all-one, sign-bit,
alternating, small and arbitrary masks; Eq/Ne; both mask input orders;
materialized booleans, branch/select fusion, shared conditions, source/select-arm
aliases, nonadjacent consumers and a call after boolean capture. All three
selectors, both allocators, ordinary/one-register/empty pools and explicit
v1/v2/v3 profiles require exact text/typed/chunked object equality, no raw text
instruction fallback and per-instruction feature legality. Negative controls
include shared AND results, all other integer predicates, nonzero comparisons,
register masks, i8/i16/i64, source locations, constraints, block/effect/intrinsic
barriers and fusion conflicts. Constant-zero source/zero-RHS identity exercises
the operand-fold accounting. An ARM emission control retains AND.

Native LinuxHost debug and release runs now execute 100 functions through 54
pipeline/feature/pool configurations, linked separately with GCC 16.2.1 and
Clang 23.1.1. Independent unsigned C expressions check extrema and deterministic
random input patterns. Calls overwrite all caller-saved GPRs. Exact byte,
signed-byte, word and dword reads are checked at both protected-page boundaries,
including faulting crossings with shared marker stores proving the fault
position. Unaligned output stores, canaries and source/output aliases are
checked. This is execution on LinuxHost, not proof of execution on v1-only hardware.

The existing `x86_compare_select`, `x86_memory_cmp` and `x86_memory_rhs` suites
also pass natively in debug/release, including FP comparison controls and
independent memory/fault-order oracles. The emission feature-guard unit test
passes locally in debug/release and on LinuxHost in release; public `x86_cpu_level`
CLI gates pass locally in debug/release. Twenty-one handwritten register/mask
recipes independently match GNU and Clang `.text` bytes at v1/v2/v3; these are
encoding controls in addition to, not substitutes for, the generated-code tests.

## Raw native application recount

Both release compilers were built from the requested base, with only this pilot
added to the candidate. Original Linux raw C inputs, definitions and working
directories from the retained native audit were reused and source hashes
checked. Full optimization/selection/allocation and generic tuning were
retained. Nine base/candidate pairs (three TUs at v1/v2/v3) have byte-identical
optimized IR. The independently reviewed counts map emitted TEST lines to exact
encoded bytes within their ELF function symbol ranges, with an immediate ZF
consumer. The following are **static TU counts**, not dynamic execution counts:

- Lua `lvm.c`: 13 new immediate register TESTs at each level, including 11 in
  `luaV_execute`; 13 fewer raw disassembly instruction-looking lines. TU `.text`
  remains 39,732 bytes. `luaV_execute` remains 29,457 bytes at v1/v2 and 29,458
  at v3.
- QuickJS `quickjs.c`: 50 new immediate register TESTs at each level. The raw
  linear disassembly has 65 fewer instruction-looking lines at v1/v2, 66 at v3.
  TU `.text` decreases by 96 bytes at v1/v2 and 112 at v3. `JS_CallInternal` has
  no new mask TEST and remains 85,700 bytes. The original one-TEST baseline
  count was jump-table data decoded as code.
- SQLite `sqlite3.c`: 66 new immediate register TESTs at each level; 80 fewer
  raw disassembly instruction-looking lines at v1/v2, 84 at v3. TU `.text`
  decreases by 96 bytes at v1/v2 and 112 at v3. `sqlite3FpDecode` has no new
  mask TEST and remains 2,203 bytes. The original one-TEST baseline count was
  also table data decoded as code.

Raw linear disassembly totals include table data decoded as instructions,
alignment NOPs and allocation changes. They are not reliable executable
instruction totals or covered-AND counts. TU byte changes include
allocation/alignment effects and do not imply that every local TEST saves bytes.
The audited Lua v3 tag window still begins at object offset `0x73ee`: `and
edi,15; cmp edi,0` becomes `f7 c7 0f 00 00 00` (`test edi,15`), followed by the
same `setne dil; movzx edi,dil`. **Both recipes occupy six bytes.** The original
audit's textual candidate screens are not typed admission counts; this recount
uses the actual emitted objects at the newer requested base.

Whole applications were freshly rebuilt at explicit v3 with the changed release
compiler, without source/header overlays. GCC and Clang links both pass Lua's
bounded `all.lua` run (`_U=true`, final OK), QuickJS closure/language/builtin/loop/
bignum tests, and SQLite's maintained query corpus with output identical to the
retained GCC/Clang reference executables. This is finite correctness validation,
not a new qualification of every application feature or external library.

Whole-TU external-assembler byte equality is not claimed: an exploratory GNU
comparison already differs on the unchanged baseline (alignment NOP choices and
local-branch relaxation, including a three-byte symbol-size difference in
`luaV_finishset`). Dedicated text/typed/chunked equality and independent TEST
recipe equality are the applicable parity gates here.

## Receipts and limits

Artifacts are outside Git under `/tmp/keel-mask-pilot3/` on LinuxHost, including
`src/`, `base/`, `recount.py`, `recount-commands.json`, `recount-summary.json`,
`object-metrics.json`, raw IR/assembly/objects/disassembly, `apps.py`,
`app-commands.json`, `recipe-commands.json` and lock logs. Native test exports
are `/tmp/keel-mask-pilot3-native-<pid>/`. Local receipts/snapshots are under
`/tmp/keel-mask-pilot3-local*`; no generated artifact belongs in Git.

All Rust builds/tests used the existing pins/profile and default managed cache:

```sh
python3 suite/fast_opt_locked.py /tmp/UNIQUE.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test x86_mask_test --test x86_compare_select \
  --test x86_memory_cmp --test x86_memory_rhs
```

Use the same command with `--release` for the release gate. All application
compilation/encoding probes also used the shared benchmark/build lock. Observed
LinuxHost load exceeded the quiet-audit threshold (for example 1.52/1.23/0.91); no
runtime or compile timing campaign was attempted. There is no speedup,
throughput or general code-size claim. The original pilot excluded the
signed-byte and pointer LEA work. The reviewed TEST branch now rebases onto the
independently integrated signed-byte cover; the archived LEA pilot remains
excluded.

## Independent review on the signed-byte base

The review branch reapplies the pilot to main
`7edfaacd52b9129c8faf3d70cfb439b32ec561eb`, preserving its signed-byte load
cover. No selection or encoding defect was found in the bounded audit. Added
tests construct actual resolved SSA source aliases, commute the mask, and
exercise boolean, branch and select allocation roots across all selectors,
allocators and normal/one-register/empty pools. Additional composition cases
retain exact m8 sign-extending loads before sign-mask TESTs and run Eq/Ne
through all three consumers. The native oracle now contains 100 functions per
configuration. The existing guarded-load checks and signed-byte suite remain the
memory and fault-order controls; composition does not introduce a memory TEST.

The review found and corrected a concrete application-counting error. Raw GNU
linear disassembly interpreted inline jump-table bytes as TEST instructions in
both QuickJS and SQLite. It also missed QuickJS's real `test esi,8` at
`js_float_env_proto_get_status+0xab`: object offset `0xc49db` at v1/v2,
`0xc499b` at v3. Starting disassembly at those addresses decodes
`f7 c6 08 00 00 00; 40 0f 95 c6` (TEST then SETNE). Mapping every emitted register/literal TEST
to its function's bytes, including F7 /0 and accumulator A9 encodings, gives
**13/50/66** actual new sites in Lua/QuickJS/SQLite at each profile. Each mapped
site immediately captures ZF. The original raw decoded counts **13/49/66**
remain reproducible receipts, but are superseded as the actual selected-site
counts. The TU and hot-symbol byte sizes above are unchanged; raw decoded-line
deltas are explicitly qualified above.

Independent SHA-256 manifests match all tracked compiler, suite and tools files
in the retained base/pilot snapshots to `2b69f9fb` / `042726912`. All 18
retained object/compiler hash pairs match their recount receipts, nine
optimized-IR pairs match byte for byte, and 18 object-emission commands retain
the original flags, working directories and primary-input hashes. The review
also checks all 44 whole-application primary-input hashes and flags. These are
present-time checks of retained artifacts; the original receipts did not freeze
or hash every transitive application header or historical executable. Fresh
hashes of the six retained GCC/Clang executables are recorded separately; 16
retained-executable correctness/reference commands passed again, including Lua
final OK, the five QuickJS tests and byte-identical SQLite query outputs. This
rerun uses the retained pilot binaries, not newly rebuilt combined binaries.

Post-rebase debug/release checks passed on LinuxHost and locally for
`x86_signed_byte_load`, `x86_mask_test`, `x86_compare_select`, `x86_memory_cmp`,
`x86_memory_rhs` and `selection`. LinuxHost executes the GCC/Clang oracles; local
Apple ARM64 checks provide emission/parity coverage and skip Linux-only native
tests. Feature-legality guards pass in debug and release on both hosts; 31
allocator unit tests also pass, including resolved aliases, parallel copies,
interference, call liveness and the independent liveness reference. No timing
was measured.

Review scripts, SHA manifests, corrected opcode/site maps, logs and captures are
under `/tmp/keel-mask-review/` on LinuxHost. The final combined source snapshot is
its `combined/` directory; local logs are `/tmp/keel-mask-review-*.log`. These
checks validate the finite source/lowering/encoding/native cases stated here,
not arbitrary programs, a v1-only execution host, new frontend support, FP
semantics changes, ABI changes or an application speedup.

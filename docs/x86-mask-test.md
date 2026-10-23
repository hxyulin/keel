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
covers, memory-RHS covers and constraints on the AND/compare/gap are excluded.
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

Native LinuxHost debug and release runs execute 91 functions through 54 pipeline/
feature/pool configurations, linked separately with GCC 16.2.1 and Clang 23.1.1.
Independent unsigned C expressions check extrema and deterministic random input
patterns. Calls overwrite all caller-saved GPRs. Exact byte, signed-byte, word
and dword reads are checked at both protected-page boundaries, including
faulting crossings with shared marker stores proving the fault position.
Unaligned output stores, canaries and source/output aliases are checked.
This is execution on LinuxHost, not proof of execution on v1-only hardware.

The existing `x86_compare_select`, `x86_memory_cmp` and `x86_memory_rhs` suites
also pass natively in debug/release, including FP comparison controls and
independent memory/fault-order oracles. The emission feature-guard unit test
passes locally in debug/release and on LinuxHost in release; public `x86_cpu_level`
CLI gates pass locally in debug/release. Twenty-one handwritten register/mask
recipes independently match GNU and Clang `.text` bytes at v1/v2/v3; these are
encoding controls in addition to, not substitutes for, the generated-code tests.

## Raw native application recount

Both release compilers were built from the requested base, with only this
pilot added to the candidate. Original Linux raw C inputs, definitions and
working directories from the retained native audit were reused and source
hashes checked. Full optimization/selection/allocation and generic tuning were
retained. Nine base/candidate pairs (three TUs at v1/v2/v3) have byte-identical
optimized IR. GNU objdump independently decodes their directly emitted ELF
objects; the following are **static TU counts**, not dynamic execution counts:

- Lua `lvm.c`: 13 new immediate register TESTs at each level, including 11 in
  `luaV_execute`; 13 fewer decoded TU instructions. TU `.text` remains 39,732
  bytes. `luaV_execute` remains 29,457 bytes at v1/v2 and 29,458 at v3.
- QuickJS `quickjs.c`: 49 new immediate register TESTs at each level; 65 fewer
  decoded instructions at v1/v2, 66 at v3. TU `.text` decreases by 96 bytes at
  v1/v2 and 112 at v3. `JS_CallInternal` has no new mask TEST and remains
  85,700 bytes. One immediate TEST already exists in both versions.
- SQLite `sqlite3.c`: 66 new immediate register TESTs at each level; 80 fewer
  decoded instructions at v1/v2, 84 at v3. TU `.text` decreases by 96 bytes at
  v1/v2 and 112 at v3. `sqlite3FpDecode` has no new mask TEST and remains 2,203
  bytes. One immediate TEST already exists elsewhere in both versions.

Instruction totals include alignment NOPs and allocation changes, so their
deltas are not simply the number of covered ANDs. TU byte changes include
allocation/alignment effects and do not imply that every local TEST saves bytes.
The audited Lua v3 tag window still begins at object offset `0x73ee`:
`and edi,15; cmp edi,0` becomes `f7 c7 0f 00 00 00` (`test edi,15`), followed
by the same `setne dil; movzx edi,dil`. **Both recipes occupy six bytes.**
The original audit's textual candidate screens are not typed admission counts;
this recount uses the actual emitted objects at the newer requested base.

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
compilation/encoding probes also used the shared benchmark/build lock.
Observed LinuxHost load exceeded the quiet-audit threshold (for example
1.52/1.23/0.91); no runtime or compile timing campaign was attempted. There is
no speedup, throughput or general code-size claim. The signed-byte and pointer
LEA pilots were neither imported nor integrated; their owners retain their
separate source regions and validation responsibilities.

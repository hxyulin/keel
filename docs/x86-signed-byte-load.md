# Adjacent signed byte load to i32

This implements pilot 2 from the [native selection audit](native-isel-audit-334d8466.md#2-adjacent-single-use-signed-byte-load-to-i32).
It started at `f7c01fc6` and was rebased onto main `2b69f9fb` in the isolated
`feat/sext-i8-load-isel` worktree. Ownership is limited to `select.rs`,
`x86_64.rs`, `tests/x86_signed_byte_load.rs` and this documentation. No sibling
branch was imported, main changed, or subagent spawned.

## Selection and emission contract

The only admitted shape is ordinary nonvolatile `load.i8` followed by
`sext.i32`, with exactly one resolved use of the loaded byte. Both results
must remain their own canonical definitions. The pair must be in one block,
with identical complete source locations (including column and scope, and
including absence). Only intervening integer constants at that same location
are skipped; constrained, matched, condition-owned or removed constants are
boundaries. Any other instruction separates the pair. Loads, stores, calls,
atomics, fences, raw regions and potentially faulting operations cannot be
crossed. Existing covers, matches, constraints and condition ownership on
either endpoint reject the pair.

The cover runs after existing address and packed-load selection and before
allocation. It transfers the load's complete selected operand list to the
conversion, preserving every constant-address, indexed-address and symbolic
address leaf. Raw IR use counts resolve aliases and include uses outside the
pair. Existing compare/binary/packed memory covers remain separate.

Emission resolves/reloads the address before defining the destination and
emits `movsx r32,byte ptr [address]`. The memory operand remains exactly m8;
`movsx edi,[rdi]` and the spilled `movsx eax,[rax]` case read the address before
writing the destination. Indexed spilled leaves use the existing distinct
scratch registers. Ordinary `def_done` completes the i32 result, including the
existing full scalar spill home. There is no byte-sized result-spill shortcut.
The 32-bit destination clears its upper 32 bits; a subsequent `uext.i64`
therefore retains its separate unsigned-i32 semantics.

Word/dword loads, other extension result widths, unsigned extension, shared
loads, volatile loads and atomic loads retain their existing paths. This adds
no feature requirement, FLAGS lifetime, ABI, FP or MXCSR operation. The
[userspace ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md)
and [GNU vector/ABI](gnu-vector-mode.md) contracts remain unchanged. The
original byte access position and width are preserved; no dereferenceability,
`notrap` or alias assumption is introduced.

## Focused validation

`x86_signed_byte_load` has five integration tests. The selection/allocation
matrix covers v1/v2/v3, Baseline/Trees/Full selection, Fast/Full allocation and
normal, one-GPR and empty pools: 54 configurations. Text, typed and chunked
objects must agree exactly, and typed operand-dependent feature requirements
must fit the requested profile. Positive chains contain exactly one m8 MOVSX
and no residual m8 MOVZX. An independent Clang assembler oracle checks direct,
address/destination-alias, indexed/high-register and negative-displacement bytes.

Private selection tests reject existing conditions, covers, matches, endpoint
constraints and aliased producer/root results, and verify complete selected-leaf
transfer. Integration negatives cover shared loads, volatile/atomic accesses,
other widths/results, unsigned conversion, intervening loads/stores/fences/calls
and nonconstant arithmetic. Debug boundaries cover file, line, column, scope
and both directions of missing locations; use and definition constraints reject
both endpoints. Resolved input aliases, symbolic and stack addresses also compile.

On LinuxHost, GCC and Clang independently pass all 54 exported configurations each.
The C oracle defines the signed-i32 answer as `b < 128 ? b : b - 256`, avoiding
out-of-range signed-byte casts. It checks every byte at both guarded-page edges,
direct/displaced/indexed addresses, overlapping input/output, complete output
canaries, live pointers, calls that clobber caller-saved registers, normal
and forced spills, unchanged MXCSR, i32 ABI returns and symbolic loads.
Invalid pointers at either protected edge must fault before any output write.
The separate following `uext.i64` case checks zero-extended i32 bit patterns.
The tiny-pool return fixture emits `movsx edi,byte ptr [rdi]`; empty pools also
exercise the address/result scratch alias. All 54 debug/release ELF objects
and C harnesses are byte-identical.

The local debug batch passes all 118 codegen library tests and 26 active tests
across the new suite and existing signed m64/m128 load, unsigned byte compare,
compare/select and constant vector-address suites. The existing byte-compare
measurement test remains ignored. Release passes all five dedicated tests and
the illegal-feature emission guard. Focused Clippy completes with repository
warnings; no new dedicated-test warning was reported.

All Rust commands retain `CARGO_CACHE_LINKS=0` and use the shared default cargo
cache, per-checkout managed targets and the shared investigation lock:

```sh
KEEL_SEXT8_EVIDENCE=/tmp/keel-sext8-rebased \
  python3 suite/fast_opt_locked.py /tmp/keel-sext8-final-check.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test x86_signed_byte_load
```

Use `--release` for the release gate. Native execution uses the exported
`run_native.py --cc gcc` and `--cc clang`, each under the same lock wrapper.
No toolchain, feature/profile pin, wrapper, remote cache or host configuration
was changed; no existing build output was deleted.

## Native application evidence

Final application receipts use fresh source snapshots on LinuxHost at
`/tmp/keel-sext8-base2` (main `2b69f9fb`) and `/tmp/keel-sext8-src2` (the
candidate production sources). An earlier archive overlay left the reused
baseline build stale; its differing optimized IR exposed the problem. The
`/tmp/keel-sext8-{apps,rebased-apps}` results are superseded and must not be used
as matched final evidence. Fresh builds retain separate cargo-managed targets.

Both fresh compilers produce byte-identical optimized native v3 IR for each
raw translation unit. Raw C is compiled with Full optimization, Full selection,
Full allocation, generic tuning and explicit v1/v2/v3. Qualified definitions
remain QuickJS `-D_GNU_SOURCE -DCONFIG_VERSION="2024-01-13" -DCONFIG_BIGNUM -I.`
and SQLite `-DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double`.
Original QuickJS source hashes are checked by the application runner; the
retained qualified command receipts supply the original source paths and flags.

Actual emitted m8 MOVSX counts, independently recounted in GNU objdump output,
are 0 -> 67 in `quickjs.c` and 0 -> 244 in `sqlite3.c` at every level. These
agree with this round's emitted assembly counts, rather than assuming the old
audit's textual screen is an admission proof. `JS_CallInternal` and
`sqlite3FpDecode` each contain three activated loads. Exact ELF function sizes,
excluding inter-function alignment, are:

- QuickJS summed function bytes: v1/v2 947,153 -> 946,885; v3
  947,055 -> 946,787. The reduction is 268 bytes at each level.
  `JS_CallInternal` is 85,700 -> 85,688 bytes.
- SQLite summed function bytes: v1/v2 1,091,577 -> 1,090,608; v3
  1,091,601 -> 1,090,632. The reduction is 969 bytes at each level.
  `sqlite3FpDecode` is 2,203 -> 2,192 bytes.

These are static translation-unit/function counts, including allocation changes;
they are neither dynamic execution counts nor a whole-executable size claim.
The output decoder line counts in the JSON may include alignment instructions;
the byte counts above use ELF symbol extents.

Fresh complete QuickJS builds at v1/v2/v3, linked separately with GCC and Clang,
pass `test_closure`, `test_language`, `test_builtin`, `test_loop` and
`test_bignum` (the last with `--bignum`). Fresh complete SQLite shell/amalgamation
builds at those same levels and both link drivers produce query-suite output
identical to their retained independent native references: 36 application
correctness invocations total. SQLite retains its explicitly qualified
`LONGDOUBLE_TYPE=double` configuration. These finite checks cover character
paths but do not certify every program/configuration.

No compile-time or runtime profitability campaign was run. LinuxHost load rose
above 2 while other work was active. Quiet shared-lock paired measurements
with at least seven rounds are deferred; build wall times and cache hits are
not speedup evidence. Native execution uses a newer x86 host, not v1-only/v2-only
hardware or a CPUID-masked environment. Feature metadata and release legality,
independent encoding/decoding, and native correctness remain distinct evidence.

## Receipts outside Git

Local Rust command/lock records are `/tmp/keel-sext8-final-check.jsonl`,
`/tmp/keel-sext8-final-release.jsonl` and `/tmp/keel-sext8-clippy.jsonl`.
The debug/release exports are `/tmp/keel-sext8-rebased` and
`/tmp/keel-sext8-rebased-release`; the native export is also on LinuxHost.
Final native reports are `/tmp/keel-sext8-rebased-{gcc,clang}-results.json`
on both hosts, with `/tmp/keel-sext8-rebased-native-lock.jsonl` on LinuxHost.

Final raw application objects, assembly, disassembly, exact commands, IR parity,
static summary and correctness records are under
`/tmp/keel-sext8-verified-apps/` on LinuxHost. Runners are
`/tmp/keel-sext8-verified-{apps,correctness,ir-parity}.py`; build and application
lock records use `/tmp/keel-sext8-verified-{build,app-lock}.jsonl`.
`/tmp/keel-sext8-final-manifest.json` records production source hashes and final
receipt identities. Initial harness parse/build warnings and the superseded
snapshot parity failure remain in the earlier task-owned artifacts; no failed
run is counted as passing evidence.

## Independent review on main 2b69f9fb

The isolated `review/sext-i8-load-isel` worktree reapplied candidate
`05332234f72186931a68f82d13d4ff82d345b518` onto the requested clean main.
The audit found no production correctness defect in the bounded cover or its
emitter. It found two validation gaps: an empty inline-assembly clobber list
allowed the original native callback to retain caller-saved register contents,
and the original fault handler accepted any SIGSEGV without identifying its
address. Those earlier checks alone did not prove physical call pressure or
read/store fault ordering.

The maintained harness now physically clears RAX, RCX, RDX, RSI, RDI and
R8–R11. Independent GNU objdump output confirms all nine XOR instructions.
SA_SIGINFO records the actual fault address. Distinct protected read and store
addresses check both orderings; a valid preceding store must become visible,
a following store must remain absent, and a valid byte read followed by an
invalid store must report the store address. Output canaries remain intact on
fault. Raw FLAGS ownership, division and cross-block negatives were added to
the existing 54-configuration selection/allocation/feature matrix. An attempted
inline-assembly emission negative was dropped because x86 inline assembly is
unsupported; it is not counted as passing evidence.

Fresh review validation passes 118 library tests and 26 focused integration
tests (one existing measurement test ignored). The final five dedicated tests
pass in debug and release, including text/typed/chunked object parity,
operand-dependent feature checks and the independent Clang encoding oracle.
The release illegal-feature emission guard passes separately. Both GCC and
Clang pass all 54 final native configurations in each profile: 216 native
runs, including all 256 byte values at both guarded edges, physical callback
clobbers, pointer/result aliases, normal/tiny/empty pools, symbolic addresses,
unchanged MXCSR and the separate following unsigned extension. All 162 final
object/assembly/C exports agree byte-for-byte between debug and release.
These results distinguish lowering/encoding from native execution; no new
frontend or full-program semantic claim follows from the backend matrix.

The application provenance audit independently hashes all tracked `crates/`,
`suite/` and Cargo/toolchain inputs in the retained LinuxHost snapshots: 1,849
baseline files match main `2b69f9fb`, and 1,850 candidate files match the
original reviewed commit, with no missing or differing files. Compiler binary
hashes, receipt hashes and candidate production source hashes agree with the
original manifest. Normalizing only compiler/output paths, emission form and
CPU level leaves the raw application commands identical to their original
qualified audit commands. Retained optimized v3 IR remains byte-identical
between baseline and candidate. A fresh objdump/nm recount of all twelve raw
objects confirms 67 QuickJS and 244 SQLite m8 MOVSX activations and the stated
268/969 summed-function-byte reductions at each level. All 36 retained
application correctness invocations were executed again successfully. This
review reuses those verified application binaries; it does not claim a new
application rebuild or timing measurement.

Review artifacts are `/tmp/keel-sext8-review-{debug,release}` locally and on
LinuxHost. Local logs use `/tmp/keel-sext8-review-{debug,debug-focus,release,guard}.log`
and lock records use `/tmp/keel-sext8-review-{check,release,guard}.jsonl`.
Native reports on both hosts are
`/tmp/keel-sext8-review-{debug,release}-{gcc,clang}-results.json`;
LinuxHost native lock records are `/tmp/keel-sext8-review-native.jsonl`.
Independent application/source audits and scripts are
`/tmp/keel-sext8-review-{audit,source-audit}.{json,py}`;
`/tmp/keel-sext8-review-clobber.dis` records physical callback writes.
`/tmp/keel-sext8-review-manifest.json` identifies the final reviewed source
and receipts. Failed exploratory checks remain in task-owned logs and are
not counted as passes. Repository-wide formatting has existing differences;
the changed test file passes its direct rustfmt check and `git diff --check`.

Production changes remain exactly the signed-byte candidate; review fixes own
only this test and this document. Main and sibling worktrees remain untouched.
The current main remains `2b69f9fb`; combined validation after any later mask
pilot integration belongs to the parent integration round. No archived LEA
candidate was restored. No profitability claim, CPUID-masked execution or
exhaustive application coverage is established by this review.

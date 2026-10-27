# Linux64 C acceptance checkpoint

The pinned Linux v6.12 audit measures source-to-object acceptance for its
64-bit inputs. It does not qualify a Keel-linked or Keel-booted kernel.
The latest ledger is [recorded with the corpus](../suite/linux-kernel/audit-c-contracts.json).
The later [link audit](linux-kernel-link-audit.md) records assembly-only linking
and the remaining full-C link failures.

## Full-acceptance goal

The target is 484/484 captured C inputs producing x86-64 objects under the
opt-in [kernel audit policy](../suite/linux-kernel/kernel-audit-policy.json).
The pinned source-to-object goal is complete at 484/484. The user approved this
explicit permission list; ordinary v1 defaults and historical audit policies
remain unchanged. Keep the reference inputs and resolved config fixed, preserve
prior acceptances, and record each compiler checkpoint and its feature policy.
Do not count feature-permission rejections as missing instruction encodings or
silently enable instructions to improve the result. Kernel linking, booting and
runtime correctness are separate milestones.

The user approved designing and implementing the remaining operand, lifetime
and storage contracts in parallel. Their designs are recorded in
[shared asm registers](c-inline-asm-shared-registers.md),
[cleanup jumps](c-cleanup-jumps.md), and [storage metadata](c-storage-metadata.md).
Future significant IR/backend changes still need a design before expansion.

## Implemented contracts

- X86 scalar GPR matching operands may have different input and output widths.
  Each operand keeps its own type and template register view. Matching does not
  extend an input into unspecified upper bits. Float/GPR crossings and 128-bit
  GPR operands reject. AArch64 keeps its equal-type tie restriction.
- The parser keeps enough lexer lookahead to recognize `fallthrough` at token
  chunk boundaries. Static-assert messages consume adjacent string literals.
- Static aggregates can copy constant compound-literal initializers, including
  block-scope static objects. Scalar leaves still pass constant/effect/binding
  checks. Addresses of automatic compound literals are not made static.
- Static flexible-array initializers extend emitted object storage to include
  the initialized tail. The nominal C type and `sizeof` remain unchanged.
  Extents and the data emitter's 32-bit size ceiling are checked before writes.
- Pointer-only transparent unions use the existing native one-word aggregate
  ABI, which matches their first pointer member's ABI. The union must be complete,
  naturally aligned and eight bytes, with only pointer members. Implicit member
  conversion applies to prototyped call arguments, including indirect calls;
  ordinary assignment and return conversion do not gain that permission.
- GNU `__label__` declarations have a lexical namespace separate from ordinary
  identifiers. Definitions, direct/computed gotos and asm-goto targets resolve
  the same scoped name. Declarations must precede other block items.
- X86 return-address queries accept constant depths 0 through 64. They follow
  saved frame records and read the selected return slot. Nonzero depths require
  readable caller frame records; missing records are not replaced with null.
- X86 `ms_abi` function-pointer declarations retain ABI identity for storage,
  layout and compatibility. Typed calls, definitions, direct foreign symbol
  addresses and implicit conversions to native function-pointer types reject.
  No Microsoft calling convention is emitted. Other ABI attributes retain
  their existing unsupported status.
- GNU `extern inline` bodies have inline-only ownership. Required direct calls
  expand the local implementation; the implementation is then removed, while
  its addressable symbol remains an external declaration. A non-static GNU `inline` definition without `extern` emits its external
  definition. Non-mandatory extern-inline calls remain external. Weak/visibility/retention/lifecycle combinations on inline-only
  bodies reject. Aliases cannot target an unemitted body.

The ordinary IR mandatory inliner retains its public and weak binding checks.
The C frontend supplies separate ownership for inline-only GNU bodies and
strong public mandatory-inline definitions. Constructor/destructor roots survive cleanup, and
placement metadata does not remain on declarations after a body is discarded.
Normal and profiled C compilation share this finalization path.

## Validation

The mixed-tie matrix covers all 8/16/32/64-bit input/output pairs with generic
and fixed registers, pointer/integer ties, and a kernel-shaped pointer-in-RAX,
status-in-EAX call. All 24 objects pass GCC and Clang callers. GCC compiles the
unadapted reference; the tested Clang rejects different-width integer matching
constraints and supplies caller coverage only. Three controls fail normally.

A second 24-object matrix checks return-address depths zero, one and two against
independent GNU assembly CALL continuation labels. Two compiler references pass.
Four wrong-depth controls fail normally without relying on faults or timeouts.

Transparent-union and GNU inline fixtures cross native compiler boundaries.
The latter distinguishes an inline body from a separately linked external
implementation with different results, checks stored/returned function
addresses, and retains a constructor. Host checks also execute scoped labels,
compound copies and flexible-array storage. Attribute placement tests now check
all 65 retained Clang oracle rows, including seven newly accepted declarators.

## Original stop boundary: empty aggregate ABI values

`mm/vma.i` passes an empty `struct vm_userfaultfd_ctx` by value, including in
public function signatures. The IR requires nonempty `byval`/`sret` layouts.
The frontend now diagnoses this boundary instead of constructing invalid IR.
Empty storage/layout declarations alone remain available.

Correct support needs a C-to-IR value mapping for omitted ABI arguments and
returns, per-target classification, callee parameter objects, argument-expression
effects, indirect calls and debug parameter indexing. Passing a dummy pointer
would change following register/stack arguments. Dropping expressions would lose
effects. This is the architecture change at which the user requested a pause.

## X86-64 SysV follow-up

The [empty aggregate mapping](c-empty-aggregate-abi.md) now admits complete,
fieldless, alignment-one GNU structs and unions on x86-64 SysV. They retain local
object storage and expression effects while consuming no call operand or result
slot. Other targets and empty shapes retain their diagnostic. IR nonempty-layout
rules and the machine backends are unchanged.

`mm/vma.i` now accepts. C acceptance is 398/484 under the unchanged explicit
optional-feature policy and 391/484 under the original pinned policy. Assembly
remains 506/506 and 490/506 respectively. All prior accepted objects are retained
and byte-identical. The [follow-up ledger](../suite/linux-kernel/audit-empty-aggregate-abi.json)
records qualification and remaining first errors. There are still 86 rejected C
inputs under the optional policy; this increment does not establish full C
acceptance or kernel link/boot support.

## GNU conditional qualifier follow-up

GNU `a ?: b` evaluates `a` once. Its internal capture holds an unqualified value;
copying the source object's outer `const` or `volatile` onto that temporary is
incorrect. The frontend now removes those outer qualifiers only from the
capture. The original expression retains its read qualifiers, and pointer
pointee qualifiers survive. Ordinary const-write checks are unchanged.

This removes all 18 prior const-write first errors. Thirteen inputs now accept;
five expose cleanup-goto, constant-expression, output-bearing asm-goto or feature
permission boundaries. C acceptance is 411/484 under the unchanged optional
policy and 404/484 under the original pinned policy. Assembly remains 506/506
and 490/506 respectively. All 904 previously accepted optional-policy objects
are byte-identical. The [qualifier ledger](../suite/linux-kernel/audit-gnu-conditional-qualifiers.json)
records source hashes, first errors and retained objects.

Three regression tests cover const scalar/pointer/member/typedef values,
single evaluation and fallback effects, one volatile source read with no
volatile temporary writes, and rejection of actual const writes. Generated
code runs in four debug/optimization combinations on the local Arm host and
with both GCC and Clang callers on Linux. Two compiler-built references also
pass. Debug objects from different build directories differ in debug data;
code, data and unwind sections match. Full validation passes 2,887 tests and
one doctest, with 13 skipped tests. Kernel link/boot qualification remains outside
this source-acceptance checkpoint.

## ELF section storage follow-up

Ten prior section conflicts mixed plain constants and relocated constants in
`.init.rodata`. One mixed zero and nonzero objects in `.ref.data`. Shared
classification now gives explicit conventional readonly ELF sections readonly
PROGBITS storage even with pointer relocations, and `.ref.data` zero objects
file-backed storage. C preflight, IR validation and text/typed emission use the
same policy. Default `.data.rel.ro` and retention conflicts are unchanged.

All 11 first errors disappear. Nine more C inputs accept under the unchanged
optional policy, for 420/484 C and 506/506 assembly inputs. The original policy
accepts 411/484 C and 490/506 assembly inputs. The other two inputs reach existing
fixed-register overlap and unaligned pointer-relocation limits. The
[storage ledger](../suite/linux-kernel/audit-section-storage.json) records these
boundaries and the qualified source hashes.

All prior acceptances are retained. Two prior objects, `arch/x86/kernel/reboot.i`
and `lib/decompress.i`, change only `.init.rodata` flags from writable/alloc to
alloc; contents, symbols and relocations remain unchanged. All other prior
objects remain byte-identical. Captured inputs and reference artifacts were
rehashed unchanged. No kernel link or boot qualification is claimed.

Clang object oracles cover x86-64 and AArch64 ELF flags, storage, symbol bytes and
pointer relocations; text/typed objects agree. GCC and Clang each link and run the
generated x86 fixture and an independent reference with non-PIC/non-PIE flags.
The fixture reads relocated pointers and initial zeros, then mutates their
writable targets and checks the new values. These checks qualify the static
kernel contract; they do not establish PIC-mode parity.

Full validation passes 2,891 tests and one doctest, with 13 tests skipped.
Workspace check, scoped Clippy, documentation and generator freshness pass.
Broad all-target Clippy stops at the unchanged `clippy::almost_swapped` error in
`keel-ir/tests/call_operand_abi.rs:15`; that test is outside this change.

## Native constant-expression follow-up

The constant evaluator now handles `__builtin_bswap16/32/64` on native LP64
integer expression trees. Arguments keep their converted widths before swapping.
Enums, switch labels, static assertions, array bounds, bitfield widths and static
initializers can use these results. Runtime builtin lowering is unchanged.
The proof checks the full integer operand tree, preserves comma restrictions,
and excludes effects, reads and intermediates wider than 64 bits. Its extra
walk is bounded to 64 levels. Floating, runtime-bearing and deeper trees remain
unproven; the private ILP32 profile does not acquire builtin support.

Native pointer-width integer arithmetic can also preserve a single symbol plus
an addend, including `(unsigned long)&init_stack + sizeof(init_stack)`. These
values have an opaque relocation representation, not guessed numeric bits or
nonnullness. Casts and pointer offsets preserve that distinction. Existing
strong-pointer truth and final weak binding remain separate. Narrow integers,
address masks, multiple-symbol arithmetic, address bitfield initializers and
addends outside the existing nonnegative 32-bit range remain diagnosed. No IR
format or backend change is required.

All ten byte-swap first errors and the separate stack-address initializer error
are removed. Under the unchanged optional policy, 426/484 C and 506/506 assembly
inputs accept. The original policy accepts 416/484 C and 490/506 assembly inputs.
Six more C inputs accept under the optional policy. Others expose cleanup-goto
and const-qualified-member errors. The [constant-expression ledger](../suite/linux-kernel/audit-constant-expressions.json)
records the new first diagnostics. All 926 prior optional-policy acceptances
are retained and byte-identical. All captured inputs and reference artifacts
were rehashed unchanged; no kernel link or boot was performed.

Five tests cover static contexts, runtime equivalence, signed/truncating inputs,
side effects and strict comma policy, wide/narrow value rejection, opaque-address
truth, bitfield relocation rejection and weak metadata. Four debug/optimization
variants run on the local Arm host with a Clang reference. The four x86 objects
pass both GCC and Clang PIE callers, for eight generated executions and two
compiler references. The native object code/data/unwind matches the local
exports; debug paths differ by build directory.

Full validation passes 2,896 tests and one doctest, with 13 tests skipped.
Workspace check, C all-target/all-feature Clippy, documentation and generator
freshness checks pass. Existing Clippy warnings remain.

## Integer builtins, generic pointers and assembly successors

The frontend supports `__builtin_ffs`, `ffsl`, `ffsll` and `isdigit` on native
targets. Bit searches convert to the declared argument width, evaluate once and
return zero for zero. The existing defined-at-zero IR `ctz` supplies the bit
position; no new IR opcode or machine lowering is added. Pure native LP64 integer
trees also fold in static contexts. Capability queries advertise these builtins
on native targets and retain the conservative Wasm policy.

`_Generic` matching preserves qualifiers below the outer value layer. A writable
structure pointer no longer matches a pointer-to-const association. Typedefs and
nested pointer qualifiers use the same canonical qualification identity as
function parameters. Actual writes through const pointers still reject.

X86 Linux `asm goto` accepts register, flag and memory outputs. Assembly paths
join the existing restoration trampoline, then register outputs are written back
before C dispatch. The hidden selector shifts matching constraints, and numeric
label references count each read/write source output twice as GNU requires.
Output addresses evaluate once. Reserved-stack outputs, cleanup/VLA crossings
and other existing unsupported contracts retain their checks.

Switches can share a function with cleanup locals. Existing scope-exit actions
handle break, continue, return and case fallthrough. Direct/computed goto and
nonlocal exits with cleanup locals remain unsupported. The native fixture checks
callback order and distinguishes switch break from loop continue.

The [goal checkpoint ledger](../suite/linux-kernel/audit-c-acceptance-goal.json)
records the corpus results, source hashes, prior-object comparison and native
witnesses. This is an acceptance checkpoint, not full kernel qualification.

The checkpoint accepts 446/484 C inputs under the unchanged optional-feature
policy and 436/484 under the original pinned policy. Assembly remains 506/506
and 490/506 respectively. No prior acceptance is lost. Of 932 prior accepted
optional-policy objects, 930 are byte-identical. The other two change only
const-correct `__dev_fwnode_const` call relocations and its import where needed;
all section bytes, storage properties and other relocations match. The captured
GCC serial-port assembly also calls that const helper.

Full validation passes 2,900 tests and one doctest, with 13 skipped tests. The
new Linux fixtures pass 64 generated executions and six compiler references;
the Clang digit reference adapts GCC's unavailable builtin spelling. Local Arm
execution covers eight generated variants and two references. The remaining
38 C first errors include 14 explicit feature permissions, eight shared-register
asm contracts, four cleanup gotos and four public mandatory-inline ownership
checks. Other first errors are recorded in the ledger. The 484/484 goal is open.

## Explicit kernel audit policy

The user approved an opt-in kernel-audit permission list while retaining normal
v1 defaults. The [policy](../suite/linux-kernel/kernel-audit-policy.json) is audit
data, selected with `audit.py probe --feature-policy`. It keeps C at v1 and
assembly at v3, then grants the listed extra features. Reports record the policy
name, hash and resolved flags; separate override flags cannot be mixed with it.

Under this policy the unchanged compiler accepts 460/484 C inputs and all
506 assembly inputs. All 14 preceding feature-permission rejections clear, and
all 952 prior accepted objects remain byte-identical. Default and explicit v1
still reject RDTSCP and XGETBV witnesses. No CPU profile or compiler source is
changed. The [policy ledger](../suite/linux-kernel/audit-kernel-policy.json)
records seven policy tests, real CLI gate witnesses and the complete corpus
rerun. The remaining 24 C rejections are implementation gaps; the 484/484 goal
remains open. Feature permission does not establish kernel runtime support.

## Union casts, case ranges and inline finalization

The [contained-gap ledger](../suite/linux-kernel/audit-contained-gaps.json)
records 464/484 C inputs and 506/506 assembly inputs under the unchanged named
kernel policy. AMD IBS, signal handling, ELF loading and mutex locking now
compile. All 966 prior accepted inputs remain accepted. Of their objects, 957
are byte-identical; nine change with compact case-range dispatch. All 992 saved
inputs, reference artifacts and binary dependencies were rehashed unchanged.

GNU union casts construct an independent union value from a matching member
type and evaluate the operand once. Scalar, pointer and aggregate members use
existing stores and aggregate copies. Bit-field casts and static aggregate
cast initializers also follow Clang's accepted extension; GCC 14 rejects those
fixture spellings. The GCC reference substitutes ordinary initializers at
those three occurrences. The Clang reference uses the unmodified source.

Switch case ranges remain converted intervals rather than an enumerated list
of values. Existing integer comparisons handle large and full-width intervals
without new IR operations. Tests check signed and unsigned boundaries,
conversion before overlap detection, selector evaluation, fallthrough and
cleanup order. LAR/LSL accept 16-bit selector-register spellings with 32-bit or
64-bit destinations while retaining their pinned decoded register views.
Required-inline discard synchronizes retained callee imports with their owning
declarations; verifier and linkage checks remain intact.

Full workspace validation passes 2,909 tests and one doctest, with 13 skipped.
Workspace checking, documentation builds, both generator freshness checks and
seven policy tests pass. The focused Linux tests pass with optimizer
verification enabled. Runtime witnesses cover eight Clang-linked generated
executions and two unmodified references, plus five GCC-linked generated
executions and two references with the union adjustments described above.
Strict Clippy remains blocked by existing dependency warnings. Whole-file
format checks reproduce existing differences in `parse.rs` and
`encode/tables.rs`; changed hunks are formatted.

Twenty C inputs remain rejected: eight shared-register asm contracts, four
cleanup gotos, four public mandatory-inline ownership checks, packed pointer
relocations, special section placement, function alignment and unreachable
width-specific asm. Their first diagnostics are unchanged. The acceptance goal
remains open; this checkpoint does not link or boot a Keel kernel.

## Constant asm paths and public mandatory-inline definitions

The [inline-finalization ledger](../suite/linux-kernel/audit-c-inline-finalization.json)
records 469/484 C inputs and 506/506 assembly inputs under the unchanged named
kernel policy. `traps.i`, `tsc.i`, `kernel/entry/common.i`, `mm/maccess.i` and
`mm/page_alloc.i` now compile. All 970 prior accepted inputs remain accepted.
Of their objects, 664 are byte-identical; 306 C objects change with finalization.
All 506 assembly objects are byte-identical. Reference inputs, config, reference
artifacts and binary dependencies were rehashed unchanged.

Before validating asm, mandatory C finalization runs bounded phi simplification,
constant folding and CFG cleanup on asm-bearing functions. Width-selecting
`sizeof` branches can therefore contain invalid asm in paths that are never
emitted, independently of optional optimization. This uses the IR CFG rather
than discarding source statements. Direct and computed goto targets remain
reachable, and invalid asm in runtime-selected paths still rejects. Existing
constant cleanup for deferred immediate operands retains its stronger passes.

The C frontend explicitly supplies its strong public `always_inline` definitions
to the mandatory inliner. Direct calls can expand those bodies while their
public symbols, stored addresses and binding attributes remain intact. No
noninterposable or no-semantic-interposition flag is added. Inline-only GNU
bodies retain their separate discard rule. Ordinary IR clients retain the
public-definition gate; weak, missing, local and non-mandatory definitions cannot
use the new public-ownership context. Other eligibility, recursion, signature
and growth checks still apply.

Full validation passes 2,915 tests and one doctest, with 13 skipped. The initial
full run exposed one declaration-matrix expectation that public mandatory calls
would remain calls. The updated target passes locally and on Linux; the full
rerun passes. Workspace checking, scoped formatting, documentation builds and
both generator freshness checks pass. Strict Clippy remains blocked by existing
dependency warnings. Sixteen focused Linux Rust tests pass, including optimizer
verification. Native witnesses cover 16 generated executions and four unmodified
GCC/Clang references on Linux, plus four generated public-inline variants on Arm.

The remaining 15 first diagnostics are unchanged: eight asm register-sharing
contracts, four cleanup-aware goto cases, one packed pointer relocation, one
ELF note-section placement and one function alignment. These require checked
operand sharing, declaration-lifetime exits, or storage metadata through IR and
emitters. The next work needs a contract design rather than relaxing guards.
The 484/484 acceptance goal remains open. No Keel kernel was linked or booted.

## Checked operand, lifetime and storage contracts

The [contract ledger](../suite/linux-kernel/audit-c-contracts.json) records
484/484 C inputs and 506/506 assembly inputs under the unchanged named kernel
policy. All 15 former rejections now produce objects. All 975 prior accepted
inputs remain accepted. Of their objects, 798 are byte-identical; 177 C objects
change with register-group initialization. All 506 assembly objects are
byte-identical. All 992 saved inputs, config, reference artifacts and binary
dependencies were rehashed unchanged. The pinned source-to-object goal is complete.

Shared asm inputs must have provably equal common low bits. The widest input
initializes each physical register once. Conflicting initializers, independent
early-clobber overlaps, duplicate output registers and clobber overlaps reject.
The IR keeps its existing serialized fields. AArch64 retains its restriction
against multiple inputs naming the same register.

Cleanup-aware direct jumps unwind declaration lifetimes in reverse order.
Forward entry across cleanup initialization rejects. Dynamic computed and asm
jumps require identical lifetime regions, and VM-type jumps still reject.
Literal computed jumps use direct lifetime handling. GCC 14 skips an exited
cleanup in the literal-computed-jump fixture, while Clang and Keel run it.
Both GCC optimization modes return 10 for that unmodified fixture; these are
recorded compiler differences, not passing reference executions.

Packed pointers use byte-offset relocations, with overlap, bounds, bitfield and
addend checks retained. Minimum function alignment survives IR roundtrips and
native, LLVM and Cranelift emission; wasm rejects native function alignment.
Readonly relocation-free ELF notes emit SHT_NOTE with allocation and optional
retention flags. Keel's executable linker still rejects producer notes such as
.note.Linux. Darwin packed pointer runtime requires classic dyld fixups.

The clean full workspace run passes 2,933 tests and one doctest, with 13 skipped.
The first run found three outdated rejection expectations; their replacements
pass before the full rerun. The Arm diagnostic matrix is now explicitly targeted
and passes locally and on Linux. Workspace checking and both generator freshness
checks pass. Documentation builds complete with eight existing warnings.
Linux witnesses pass 75 generated executions and eight reference executions.
The shared-register
reference collapses redundant matching inputs because Clang rejects those source
constraints. The upper-bit-loss control exits 4 as expected. The focused runs
pass 38 tests at the production snapshot and 27 at the final qualification
snapshot, with repeated tests counted in each run. Strict Clippy remains blocked
by existing diagnostics. Formatting comparisons find no new differences across
39 changed Rust files; pre-existing differences remain. The reference kernel
was not rebuilt or rebooted, and no Keel kernel was linked or booted.

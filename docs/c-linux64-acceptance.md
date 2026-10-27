# Linux64 C acceptance checkpoint

The pinned Linux v6.12 audit measures source-to-object acceptance for its
64-bit inputs. It does not qualify a Keel-linked or Keel-booted kernel.
The checkpoint ledger is [recorded with the corpus](../suite/linux-kernel/audit-c-acceptance.json).

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

The mandatory inliner retains its public and weak binding checks. GNU bodies
arrive through a separate C-owned body list; the ordinary IR entry point does
not receive that ownership. Constructor/destructor roots survive cleanup, and
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

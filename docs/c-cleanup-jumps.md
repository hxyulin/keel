# Cleanup-aware jumps

A cleanup local has a declaration-lifetime region. Its region starts after its
initializer (or its declaration without an initializer) and ends at the end of
its block. Regions nest in declaration order, including declarations in the same
block. A label records the regions containing that source position.

A direct jump may retain a prefix of its source regions. It calls callbacks for
the remaining suffix in reverse declaration order before branching. A backward
jump across a declaration therefore cleans up the old object before executing
its declaration again. A jump forward over a cleanup declaration is rejected,
as is entry from a sibling block into a cleanup lifetime. This follows Clang's
checked entry rules. Entry into uninitialized cleanup objects is outside this
contract. No runtime initialized flags suppress callbacks.

In functions containing cleanup locals, statement expressions form additional
entry barriers. Jumps may leave them or stay within them, but cannot enter them from outside. Their internal labels and
cleanup declarations participate in the same analysis as ordinary statements.

Literal `goto *&&label` follows the direct-jump rules. Dynamic computed gotos
use every address-taken label as a possible destination. All such targets must
have exactly the source cleanup and statement-expression regions. Clang rejects
dynamic indirect edges that bypass initialization or teardown. The existing
shared computed-goto dispatch remains valid under this restriction.

Asm goto writes its outputs before dispatch. Its possible targets must have
exactly the source cleanup and statement-expression regions, following Clang's
checked asm-goto rules. Fallthrough and taken successors retain those lifetimes
until their ordinary scope exits. The existing backend contract needs no change.

All goto forms in functions with variably modified types remain rejected.
Stack-save tokens require a separate dominance and lifetime contract. Nonlocal
exits and returns-twice calls also retain their existing rejection.

The parser validates all cleanup jump edges, including unused inline bodies.
Lowering uses the same destination-region table. Existing return, break,
continue, switch and ordinary block-exit lowering retains its reverse-order
exit-action stack. A jump emits actions without consuming that compile-time
stack, so sibling paths and later labels retain their own actions.

## Compiler witnesses

Apple Clang 21.0.0 rejects forward entry across a cleanup declaration and entry
into a GNU statement expression. A native witness with a backward jump across a
cleanup declaration records one callback per iteration. A literal computed jump
out of an inner cleanup block runs the callback before reaching the outer label. A
dynamic computed jump and asm goto across the same boundary are rejected.
The regression tests retain these cases as executable witnesses.

GCC 14.4 differs for literal `goto *&&label` leaving a cleanup lifetime: the
unmodified fixture returns 10 at both `-O0` and `-O2`, because the inner cleanup
is skipped. Clang 21.1.8 and Keel run that cleanup and pass the fixture.
Keel deliberately follows the checked Clang contract for this form. The audit
records GCC's different result separately from passing reference executions.

The [GCC cleanup attribute contract](https://gcc.gnu.org/onlinedocs/gcc-14.4.0/gcc/Common-Variable-Attributes.html) specifies callbacks on scope exit. [Clang cleanup lowering](https://clang.llvm.org/docs/LLVMExceptionHandlingCodeGen.html) describes cleanup-aware direct edges.

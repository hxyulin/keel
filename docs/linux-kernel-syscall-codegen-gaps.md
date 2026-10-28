# Kernel syscall code generation gaps

This records the pre-optimization audit. The subsequent
[clobber-aware implementation](x86-asm-clobber-optimization.md) removes much of
the dispatcher overhead measured below; its new function sizes are recorded there.

This audit examines the pinned Linux v6.12 `arch/x86/entry/common.i` and
`kernel/sys.i` inputs. Fresh O2 and Os objects from the current working-tree
release compiler match all four object hashes in the previously qualified
`audit-policy-fix.json` source campaign. Concurrent frontend changes are present
in the compiler build, but do not change these four objects. No new whole-kernel
build or runtime campaign was run for this audit.

## Interpreting the runtime comparison

The earlier chart compares Keel with GCC at each matching preset. A lower Os
ratio means a smaller slowdown relative to GCC Os; it does not mean Keel Os
runs faster than Keel O2. The recorded absolute workload medians are higher for
Os. Moreover, both compilers' Os samples shift sharply after pair 3, including
host startup times. The cause was not established. Do not use these separate
cohorts to rank O2 against Os. A new interleaved four-image campaign with host
and guest clock diagnostics is needed for that question.

## Largest observed source of dispatcher growth

| Function | GCC O2 bytes | Keel O2 bytes | GCC Os bytes | Keel Os bytes |
|---|---:|---:|---:|---:|
| `do_syscall_64` | 193 | 711 | 168 | 651 |
| `__x64_sys_getpid` | 27 | 50 | 27 | 53 |

The GCC Os objects are the retained reference build. GCC O2 sizes come from
the qualified measurement ledger. Fresh Keel objects reproduce its sizes.

Keel's Os dispatcher wraps `sti` and the inline bounds-mask template with
14 GPR stores followed by 14 GPR loads each. Each template's preservation
costs 148 encoded bytes, for 296 bytes across both templates. That is 61.3%
of the 483-byte dispatcher excess over GCC Os. This counts existing emitted
instructions; it is not a measured saving from an implemented optimization.

The Os object offsets are:

- `0x24..0x6e`: stores before `sti`; `0x6f..0xb9`: restores afterward.
- `0x106..0x150`: stores before the mask template;
  `0x169..0x1b3`: restores afterward.

The backend's `native_inline_asm` always preserves every available non-frame
GPR. Planning reserves `8 * (16 + input_count + output_count)` bytes, and
the presence of an asm home also forces RBX and R12–R15 prologue preservation.
In this dispatcher, the stack allocation is 152 bytes in addition to those
register pushes. These are conservative implementation choices, rather than
requirements imposed by a memory clobber or by `sti` itself.

The first contained optimization should bypass register staging for templates
with no operands and no GPR clobbers. Memory and condition-code effects must
remain intact. A later step can preserve only homes that are live and may be
overwritten by operand setup, template clobbers, outputs or staging scratch
registers. It must handle fixed/tied operands, early clobbers, rematerialization,
flags outputs, asm calls, reserved stacks and custom allocator pools. Simply
deleting the saves from the general path would lose that contract.

## Other observed overhead

1. **Call argument and result placement.** The Os getpid wrapper has 17
   instructions versus GCC's nine. It copies constants and arguments through
   temporary homes, including an overwritten incoming-argument move, then
   moves the call result through RSI before extending it and moving it to RAX.
   ABI-aware allocation and copy elimination are candidates. Some copies are
   needed for genuine parallel-copy cycles, so eliminate only proven redundant
   ones.
2. **Symbol and address folding.** The current kernel model uses a seven-byte
   signed-32-bit symbol-address materialization followed by a three-byte load.
   GCC uses a seven-byte RIP-relative load directly into the argument register.
   The older `movabs` example is superseded for this current object. The
   dispatcher also builds field addresses with separate moves and adds rather
   than consistently selecting base-plus-displacement memory operands.
   Folding needs the appropriate code-model range and relocation guarantees.
3. **Boolean tests and control flow.** The dispatcher emits move/AND/TEST
   sequences, jump-only blocks and separate boolean return preparations.
   GCC selects compact TEST forms and converges more paths. Keel already
   has mask-test fusion and Os shared epilogues; these examples expose gaps
   in their coverage rather than a complete absence of those optimizations.

Recommended order is the operand-free asm fast path, selective preservation,
argument/result copy elimination, then address and test folding. These changes
target code that executes on common syscall paths. Function size alone does
not attribute the measured runtime gap: syscall helpers, VFS, memory management,
and scheduling also contribute. Rebuild, inspect objtool diagnostics, boot the
correctness fixture and rerun controlled timings after each backend change.

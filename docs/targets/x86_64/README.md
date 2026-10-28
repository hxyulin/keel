# x86-64

Keel targets x86-64 Linux with feature-gated native code generation, assembly
and decoding. The default CPU profile is strict v1. Assembly exposure, typed
compiler operations and runtime qualification are separate boundaries.

| Area | Reference |
| --- | --- |
| Feature levels and runtime requirements | [Target support](../README.md), [CPU profiles](../../../tools/x86-isa/cpu-profiles.md) |
| Assembly | [Assembler](assembler.md), [GNU language rules](gnu-assembly-language.md), [AT&T syntax](att.md), [symbolic immediates](signed-symbolic-immediates.md) |
| Compiler | [Code generation](codegen.md), [intrinsics](intrinsics.md), [BMI operations](bmi.md), [raw division](raw-div.md) |
| C inline assembly | [x86 constraints](c-inline-asm.md), [shared operands](../../frontends/c/shared-registers.md) |
| Extended floating point | [F80 and x87](f80/README.md) |
| Privileged and machine state | [System APIs](system/README.md) |
| Generated instruction inventory | [Structural coverage](coverage/README.md) |
| Linux kernel | [Integration and limits](../../integrations/linux-kernel.md) |

Feature permission does not configure OS state or privilege. Read an operation's
contract before using system, raw flags, x87 or terminal-transfer APIs.

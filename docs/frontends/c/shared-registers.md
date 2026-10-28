# Shared x86 inline assembly registers

One physical register has an input lifetime before the template and an output
lifetime after it. A non-early fixed output may share an input register without
an explicit matching constraint. This does not require a balanced stack operand.
Each output still needs its own register. A fixed independent input cannot share
an early-clobber output. An explicit matching input may initialize that output.
Clobbers cannot overlap operands.

Several inputs may share a fixed register or match one output. Their common low
bits must be provably equal. The IR helper `asm::same_register_input` resolves SSA
aliases and strips scalar conversions that preserve all observed bits. It also
compares numeric constants modulo the common width and equal symbolic addresses.
Separate loads, unrelated parameters and unproved arithmetic equality fail the
check. The same C variable name, runtime coincidence or allocator location does
not prove equality. Distinct volatile loads remain distinct observations.

The widest input initializes the register once. Narrower inputs retain their
individual register views and require only their own low bits. Neither a wider
output nor a template modifier creates an implicit extension guarantee for a
narrow input. Multiple output writes, incompatible initializers and independent
early-clobber overlaps remain errors.

The C parser retains repeated matching constraints, including explicit matches
beside a read/write operand's implicit match. IR verification checks every pair
of matching inputs. The x86 planner checks all inputs assigned to each physical
register, including mixtures of fixed and matching constraints. No serialized IR
fields or metadata schemas change. The AArch64 backend retains its existing
restriction against multiple inputs assigned to one register.

Emission saves the old GPR homes before staging any argument. It stages arguments
before loading the selected widest initializer. It captures output registers,
restores old homes, then installs SSA results. This preserves live original inputs
under register allocation, spilling and constant rematerialization.

## Focused validation

`crates/keel-c/tests/x86_asm_shared_registers.rs` exports objects when
`KEEL_X86_SHARED_REGISTERS_EXPORT` names a directory. The C payload defines
`guest_main` and returns zero only when output values and live inputs match its
arithmetic oracle. It covers RAX multiplication/division, status/mask sharing,
read/write duplicate matches, explicit early ties, pointer views, narrow results,
mixed input widths and constants. It uses ordinary instructions in place of
privileged kernel templates. Raw kernel preprocessed units are not included.

The matrix covers optimization off/on, three selection tiers, fast/full/forced
rematerialization allocation, and normal/empty register pools. Text, typed and
chunked objects must agree. Exported `.o` files can each run as `--payload` through
`suite/integration/x86-cpl0/run.py`. The exported `wrong-wide-initializer.control` is an ELF
object that deliberately discards required upper bits. The harness must reject it.
Clang rejects repeated explicit matching constraints, so an external reference
must first collapse redundant matching and fixed inputs to the widest initializer.
This also avoids Clang reloading a narrow fixed alias over a wider input.

The IR tests in `crates/keel-ir/tests/shared_asm_inputs.rs` check common-bit proofs
and rejection of conflicting high bits, separate loads and unrelated values.
Full workspace validation and kernel corpus acceptance belong to the parent run.

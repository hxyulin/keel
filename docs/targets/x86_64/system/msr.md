# Raw x86-64 MSR access

`x86_rdmsr(i32 index) -> i64` and `x86_wrmsr(i32 index, i64 value)` expose
RDMSR and WRMSR in 64-bit code. The index's full 32-bit pattern is placed in
ECX. Reads concatenate EDX:EAX into one i64. Writes place value bits 31:0 in
EAX and bits 63:32 in EDX, with no result. Both SSA inputs remain available
with their original values after the operation.

Both intrinsics are pinned compiler memory barriers. Unused reads and writes
remain after optimization. The lowering uses the allocator-reserved RAX, RCX
and RDX scratch registers; it does not overwrite allocated input homes or
introduce a helper call. The existing fixed encodings are `0f 32` and `0f 30`.
No assembler enum or encoding metadata changes are needed. The intrinsic IDs
append as 776 and 777.

## Preconditions and effects

The caller must run at CPL0 on a CPU supporting MSR access. The selected MSR
must exist and permit the requested access in the current CPU and virtual
machine environment. Reserved-bit, canonical-address, feature and mode rules
remain the hardware's rules. No MSR whitelist, CPUID probe, software fallback
or permission check is inserted. The API accepts runtime indices; v1/v2/v3
select legal baseline instruction encodings rather than promising that every
MSR index is usable.

The protocol covers MSRs accessed through ECX and EDX:EAX. It does not supply
additional implicit registers for model-specific protocols that require them.
Undefined or unimplemented read bits retain the hardware result; they are not
normalized into a stronger value guarantee.

The caller owns state changes and restoration. Code, stack and data mappings,
64-bit execution and the calling convention must remain usable by subsequent
compiled code. Changing FS/GS bases requires code whose TLS and segment-address
assumptions remain valid. The intrinsic does not rebind existing TLS addresses. The intrinsics do not create an interrupt, exception, TLS or
execution-mode transition ABI. A hardware fault produces no normal result or
compiler-provided recovery path.

A compiler memory barrier does not add LFENCE, MFENCE or CPUID. Architectural
serialization remains a property of the actual instruction, MSR and CPU. The
API does not fix the number or placement of surrounding pure arithmetic
instructions. Packing and splitting may change arithmetic flags; these APIs
do not preserve a caller's RFLAGS image. The AArch64 backend rejects both
intrinsics.

## Disposable CPL0 validation

`cases/msr.keel` exports `msr_observe(out, index, value, live0, ..., live7)`
using the SysV integer ABI. The fixture reads the selected MSR, writes the
supplied value, reads it again and performs one unused read. It then stores
both observations, the original value, the zero-extended original index and
eight additional live i64 inputs into a 128-byte output. Bytes 0–7 and
104–127 remain canaries. Eleven arguments and multiple live results exercise
register pressure and incoming stack arguments. The unused read must survive
optimization.

The assembly bridge seeds and checks two distinct MSRs:

- IA32_KERNEL_GS_BASE (0xc0000102): from 0x00001234deadbeef to
  0xffffabcd01234567.
- IA32_GS_BASE (0xc0000101): from 0xfffffeba89abcdef to 0x0000456789abcdef.

All MSR values are canonical under 48-bit addressing. The fixture uses no
GS-relative accesses or SWAPGS. A separate `msr_constant() -> i64` function
writes and reads index 0xc0000102 using IR constants, exercising
constant-operand lowering independently of argument homes. The bridge checks its
returned value and the actual MSR against 0x000056789abcdef0.

The bridge saves both original MSRs and
restores them on normal success or mismatch. It independently reads the
actual target after the generated write, then compares the entire output
against literal expectations. It installs no exception recovery.

```sh
KEEL_MSR_EXPORT_DIR="$PWD/target/msr-exports" \
  cargo test -p keel-codegen --test x86_msr
python3 suite/x86-cpl0/descriptor_campaign.py --msr \
  --exports target/msr-exports --timeout 5
```

The compile matrix covers all three selection tiers, both allocators and
normal/empty pools at explicit v1, v2 and v3. Text, typed and chunked objects
must match. The twelve standard v1 variants are exported for the guest campaign. One
additional forced-allocation variant replaces both constant operand homes
with immutable rematerialization recipes. Its emitted instructions rebuild
ECX and RAX directly before WRMSR, exercising the scratch-capture path.
It uses the same text/typed/chunked parity and guest checks. This qualifies
that backend path separately from the experimental allocator profile. Tests also check signatures,
pinned/barrier effects, invalid operand types,
foreign-target rejection and the literal opcode bytes.

All thirteen generated variants and an independent assembly reference pass
both dynamic MSR cases and the constant-operand case under QEMU 11.1.2 TCG on macOS ARM64. Five negative controls
return the exact READY/FAIL transcript and exit 35: omitted write, truncated
read, wrong index, clobbered live value and no observations. Faults and
timeouts cannot satisfy a negative control. This is bounded emulated evidence
for these two indices and values, not native bare-metal qualification, every
MSR protocol or fault resumption.

The operand and privilege contracts follow Intel's
[instruction reference](https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf).
See the [guest runner](../../../../suite/x86-cpl0/README.md) for isolation and tool-path
overrides.

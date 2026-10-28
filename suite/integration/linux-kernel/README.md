# Minimal Linux x86-64 compiler target

Keel compiles and natively links the captured 64-bit core of the pinned minimal
Linux v6.12 configuration. See the [code-model contract](../../../docs/integrations/linux-kernel.md),
[native linking guide](../../../docs/integrations/linux-kernel.md) and
[benchmark methodology](../../../docs/benchmarks/linux-kernel.md) for scope and limits.
This is not a complete Kbuild replacement.

This fixture targets the **64-bit parts** of Linux v6.12. Host programs,
16/32-bit code and mixed-mode assembly remain external. The supported x86 C
inline-assembly subset is recorded in the follow-ups below. Do not strip
assembly or attributes from Linux to make the probe pass.

`source.json` pins the release archive and source identity. `minimal.config`
is the input to `allnoconfig`; `resolved.config` records the reference result.
The seed enables serial output, an ELF initramfs and the disposable guest's
I/O permission API. The latter lets the reference init report a normal QEMU
debug-exit result without requiring an ACPI power-off subsystem.

This is a compatibility fixture. It disables SMP, modules, networking, optional
instrumentation and most devices by starting from `allnoconfig`. It is not a
production or security configuration. The booted reference kernel, initramfs program and boot code use an external
compiler and linker. Link experiments use separate object copies.

The fixture selects the frame-pointer unwinder. ORC unwind metadata is a later
configuration target. Objtool still runs for architecture-selected features;
disabling ORC does not remove that build requirement.

## Build the reference

Run on x86-64 Linux with GCC 14.4.0, GNU binutils, make, flex, bison, bc, Perl, Python
libelf development headers/libraries and `qemu-system-x86_64` available. Builds
are bounded by `--jobs`. `--hostcc` selects the host compiler independently;
it must be able to find those host development headers. Use a fresh
output directory. Keep the source tree unmodified and verify the release
archive before extraction:

```sh
kernel_audit_root="$PWD/target/linux-kernel"
mkdir -p "$kernel_audit_root"
curl -fL https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz \
  -o "$kernel_audit_root/linux-6.12.tar.xz"
echo "b1a2562be56e42afb3f8489d4c2a7ac472ac23098f1ef1c1e40da601f54625eb  $kernel_audit_root/linux-6.12.tar.xz" | sha256sum --check
tar -xf "$kernel_audit_root/linux-6.12.tar.xz" -C "$kernel_audit_root"
python3 suite/integration/linux-kernel/reference.py \
  --source "$kernel_audit_root/linux-6.12" \
  --out "$kernel_audit_root/reference-out" --jobs 8
```

The script checks the source Makefile hash, configures the kernel, builds a
reference `bzImage` while saving intermediate inputs, builds a static initramfs
program and boots under single-vCPU QEMU TCG. The boot must print exactly one
`KEEL_LINUX64_REFERENCE_OK` line and return debug-exit status 33. Faults,
timeouts and a marker without the expected exit never pass. The reference
image includes external boot/decompressor code; this does not expand Keel's
64-bit implementation scope.

The initramfs uses deterministic gzip/newc output and includes `/dev/console`
without requiring root or host `mknod`. Kbuild's user, host and timestamp
fields are fixed. Compiler versions and resolved configuration still affect
the result; `reference.json` records versions and artifact hashes. A different
compiler may resolve different capability symbols. The runner rejects a
resolved config that differs from the committed pin. Review those differences
rather than silently replacing it. The initial reference uses GCC 14.4.0,
GNU binutils 2.46 and QEMU 11.1.2; newer GCC defaults break the old release's
external boot/decompressor C. Host compilation used a compiler able to find
the system libelf headers. Compiler-capability fields also record other
installed tool versions even when their optional kernel features are disabled.

## Capture and probe

```sh
cargo build -p keel
python3 suite/integration/linux-kernel/audit.py capture \
  --source "$kernel_audit_root/linux-6.12" \
  --build "$kernel_audit_root/reference-out/build" \
  --out "$kernel_audit_root/reference-out/inputs.json"
python3 suite/integration/linux-kernel/audit.py probe \
  --build "$kernel_audit_root/reference-out/build" \
  --manifest "$kernel_audit_root/reference-out/inputs.json" \
  --keel "$PWD/target/debug/keel" --out "$kernel_audit_root/keel-probes"
python3 suite/integration/linux-kernel/audit.py compare \
  --build "$kernel_audit_root/reference-out/build" \
  --probe-out "$kernel_audit_root/keel-probes" \
  --out "$kernel_audit_root/gnu-comparison"
```

Capture joins GCC's saved `.i`/`.s` files with Kbuild's `.o.cmd` commands. It
requires `__KERNEL__` and excludes host programs. It marks boot, the real-mode
`arch/x86/realmode/rm/` subtree and
compatibility subtrees, `-m16`/`-m32` invocations and assembly containing
`.code16`/`.code32` as external. The entire mixed-mode assembly file stays
external; it does not claim coverage for selected lines within it. Saved
compiler-generated `.s` and preprocessed handwritten `.S` inputs are both
included. Per-input hashes and normalized commands remain in the manifest.
Build helpers under `scripts/` and `tools/` also remain external, even if they
use target flags; `scripts/mod/empty.c` is one such target-layout probe.

The probe checks that the resolved configuration and each input still match
the capture. It sends unchanged `.i` files to the C compiler and unchanged
`.s` files to the AT&T assembler. Assembly uses the explicit v3 profile to
separate syntax failures from the default v1 profile; remaining feature-gate
errors remain failures. It does not modify input or retry after removing a
diagnostic. First errors are grouped in `results.json`; later errors can be
masked. An accepted input is not execution evidence or a complete kernel
support verdict. GCC-preprocessed inputs also retain GCC-selected language
and attribute paths, which need to work in their own right.
Probes run in Kbuild's output directory so relative generated-file references
retain their build context. Handling `.incbin` and other file dependencies
still needs an assembler input-access contract; the audit does not inline them.
Keel's unsupported Kbuild flags are not forwarded or silently accepted. The C
probe uses its ordinary userspace code-generation mode. Accepted objects are
not yet checked by objtool or linked into the reference image.

The `compare` command assembles each accepted assembly input with GNU as and
uses the existing `compiler_asm_compat/objinspect.py` normalizer. GNU relocation
relaxation is disabled to match the current assembler contract. It retains
strict section/symbol differences. Shared-section and public-symbol metrics are
additional diagnostics; they do not excuse missing sections or establish whole
object equivalence. Use `--gnu-as` to select the reference assembler.

Generated build trees, saved inputs, objects and raw logs stay outside Git.

`probe --asm-target-features <list>` and `--c-target-features <list>` select
explicit extra features independently. Omit them to retain the original
assembler v3 and C v1 policies. Results record both policies. Keep reports from
different policies separate when comparing acceptance counts.

The opt-in [kernel audit policy](kernel-audit-policy.json) grants instruction
availability for this corpus without changing Keel's CPU levels. Select it with:

```sh
python3 suite/integration/linux-kernel/audit.py probe \
  --build /path/to/build --manifest inputs.json \
  --keel target/debug/keel --out probe-kernel-policy \
  --feature-policy suite/integration/linux-kernel/kernel-audit-policy.json
```

This keeps C at v1 and assembly at v3, then supplies the JSON's explicit extra
features. The C list grants POPCNT, RDRAND, RDTSCP, XSAVE, CLFLUSH, FSGSBASE,
PREFETCHW, INVPCID, RDSEED, CX16 and WAITPKG. The assembler retains the preceding
eight-feature permission list. Each permission has a rationale in the JSON.
The probe validates canonical names against the generated feature schema and
records the policy name, content hash and resolved flags. Combining this option
with either target-feature override is rejected before probing.

This policy permits assembly of the captured instructions. It does not change
normal compiler defaults, establish CPU/OS state preconditions, disable the
red zone or SIMD, or qualify kernel ABI, linking or execution. Ordinary probes
without feature options retain the original policies. Historical reports keep
their original permissions and remain separate from the named-policy results.

## Results and maintenance

Keep source pins, configurations, policies, fixtures and runners in Git.
Write audit manifests, object comparisons, logs and timing samples under
`target/linux-kernel/` or `suite/reports/`. Do not commit per-run audit snapshots.
Earlier checkpoints and their results remain in Git history.

Acceptance of a translation unit does not establish correct runtime behavior.
After compiler or linker changes, regenerate objects and run the boot and
workload checks. Use the runner's `--help` for link, benchmark and comparison
options. Keep external boot, setup, decompressor and host tools identified in
any result; the fixture does not qualify 16/32-bit code.

# Native kernel linking

Keel now owns archive selection, ELF relocatable aggregation and final linking
for the pinned minimal x86-64 Linux v6.12 core. It evaluates the unchanged
expanded GNU script, keeps Linux notes, applies relocations at high virtual
addresses and emits separate physical load addresses. The physical entry is
`0x1000000`. No reference symbol values, replacement objects, removed notes or
undefined-symbol suppression are used.

The [qualification ledger](../suite/linux-kernel/audit-native-linker-qualification.json)
records input and output hashes, GNU comparisons and runtime results. The
[original probe ledger](../suite/linux-kernel/audit-native-linker.json) preserves
the seven failures that prompted this work.

## Native link paths

Both native routes pass the existing checked VFS, memory/protection,
fork/COW and pipe workloads in QEMU 11.1.2 TCG:

- Keel selects regular archives, emits an ET_REL aggregate, then final-links it.
- Keel selects and final-links the original archives directly.

A third test final-links the existing GNU aggregate with Keel. GNU final links
of the original archives and Keel aggregate provide independent controls. All
five paths boot and complete every runtime stage. Three deliberately broken
userspace fixtures are rejected: a wrong success marker, a wrong exit status
and corrupted memory expectations. Faults and timeouts do not count as passes.

The three archives contain 440, 35 and 25 object members. Rebuilding them as
GNU thin archives preserves all member bytes and order. Thin and regular
archives produce identical Keel aggregate bytes and identical direct final-link
bytes. The final reviewed implementation also regenerates the three qualified
native-final images byte for byte.

The compiler objects come from the previously qualified captured-input build.
This run qualifies the new linker, not a new kernel compilation. External
reference stages still supply boot/setup, decompression and vDSO preparation.
Raw-source Kbuild and arbitrary kernel configurations are not qualified.
The existing objtool warning backlog is unchanged.

## Reproduction

Use the pinned archives, expanded `vmlinux.lds` and separately compiled
`init/version-timestamp.o` from the captured build:

```sh
cargo build -p keel
KEEL=target/debug/keel
$KEEL link --target x86_64-unknown-linux-gnu -r \
  --whole-archive vmlinux.a --no-whole-archive \
  --start-group lib/lib.a arch/x86/lib/lib.a --end-group -o aggregate.o
$KEEL link --target x86_64-unknown-linux-gnu \
  -z max-page-size=0x200000 --script=vmlinux.lds \
  aggregate.o init/version-timestamp.o -o vmlinux
```

For the direct route, replace the final link's two object inputs with
`--whole-archive vmlinux.a init/version-timestamp.o --no-whole-archive
--start-group lib/lib.a arch/x86/lib/lib.a --end-group`.

The 2 MiB LOAD alignment is required by this Linux decompressor. A diagnostic
run without that setting halted in the decompressor before serial output;
the corrected GNU and Keel links pass. The default userspace layout retains
its existing target page size.

Package and test with the reference boot build and external tools on PATH:

```sh
python3 suite/linux-kernel/package.py --reference-build reference/build \
  --vmlinux vmlinux --out packaged
python3 suite/linux-kernel/runtime.py \
  --kernel packaged/arch/x86/boot/bzImage --out runtime
```

The packaging helper checks the pinned config, copies boot artifacts into a
new output directory, sorts exception tables and rebuilds the compressed and
setup images. It never relinks the kernel core. Output directories must not
already exist. Keep their small JSON/log evidence and remove their generated
images and boot-tree copies when the experiment is finished.

## Script and input scope

See [GNU linker scripts](gnu-linker-scripts.md) for the grammar, expression
limits, output-section ordering, symbol rules and remaining unsupported syntax.
`-T path`, `-Tpath`, `--script path` and `--script=path` select it. An explicit
`-e`/`--entry` takes precedence over script ENTRY and participates in archive
selection. Whole-archive controls and nested groups retain left-to-right lazy
selection outside those controls.

Partial links retain contribution identities and relocations rather than
performing GNU ld's default coalescing and unwind compaction. Intermediate
files and some final addresses therefore differ from GNU. Their layouts and
runtime behavior are checked independently. The partial writer preserves
COMDAT groups, common/weak selection and linked section indices. It currently
rejects REL relocations and extended ELF section-index output. Scripted partial
links are unsupported.

Thin archives are materialized by the CLI from member paths relative to their
archive. Library callers supply an explicit reader through
`archive::materialize_thin`; the linker library does not read the filesystem.
Unsupported scripts, unplaced allocated sections and relocation overflow fail
with diagnostics. This is bounded GNU-script support for the captured kernel,
not full GNU ld command-line or script-language compatibility.

## Validation

The full workspace run passed 2,993 tests, with zero failures and 15 ignored,
including doctests. A final linker/CLI run passed 440 tests; the two pinned
kernel opt-in tests also passed separately. Workspace check, scoped formatting,
library documentation and 12 kernel Python tests passed. Ordinary Clippy
completed; warnings-denied Clippy remains blocked by existing diagnostics in
unchanged code. Small logs remain locally under
`.git/native-link-qualification/20261028`; generated kernel files, copied
archives, boot trees and all three delegated checkouts/branches were removed.

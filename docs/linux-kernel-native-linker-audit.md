# Native kernel linker gaps

The compiler/assembler milestone does not establish native kernel linking.
GNU ld 2.46 still links the qualified Linux v6.12 core. A fresh Keel build
rejects the pinned link script, archive-selection options and partial-link mode.
Passing the already aggregated Keel core object directly also fails.

The [probe ledger](../suite/linux-kernel/audit-native-linker.json) records binary
and input hashes, seven observed failures and the relocation inventory. No
linker implementation changes are included in this audit.

## Observed blockers

| Area | Current behavior | Pinned kernel requirement |
| --- | --- | --- |
| Script evaluation | `-T` and `--script=...` reject. | Evaluate the expanded `arch/x86/kernel/vmlinux.lds`, including expressions, symbol assignments, `KEEP`, sorting, assertions and discard rules. |
| Address layout | `elf.rs` uses `BASE = 0x400000`; symbol addresses and both LOAD address fields derive from `BASE + file offset`. | `.text` starts at virtual `0xffffffff81000000`, with physical load address `0x1000000`. Explicit `PHDRS` select RX, RW and RWX LOADs and a NOTE. |
| Section organization | Input sections are laid out by permissions and BSS class, with existing special registration/unwind handling. | Aggregate named input sections in script order, preserve table boundaries, apply 2 MiB alignments and fill, and account for reserved stack/BSS regions. |
| Script symbols and entry | Without the script, the note-stripped diagnostic object fails on `__initramfs_start`. Entry validation requires a symbol inside executable code. | Define boundary symbols, aliases and expressions, including absolute physical entry `phys_startup_64 = startup_64 - 0xffffffff80000000`. |
| Linux notes | The unchanged aggregated object rejects with `unsupported allocated note .note.Linux`. | Preserve bounded Linux note records and place them in the script-requested NOTE segment. GNU property notes are discarded by this script. |
| Build orchestration | `-r`, `--whole-archive` and `--start-group` reject. Archives are currently lazy, left-to-right inputs. | The captured link uses whole-archive inclusion, fixed-point library groups, and an ET_REL aggregation step. |

The script/layout work is a linker architecture change, not an option parser
addition. `Section` currently has one output offset. Address calculation,
relocation application, segment emission and symbol-table emission must share
a layout that separates file offsets, virtual addresses and physical load
addresses. High virtual addresses must not be passed through file-size limits.

All five relocation kinds present in the qualified Keel aggregate already have
native handlers: `R_X86_64_64` (32,601), `PC32` (16,308), `PLT32` (21,911),
`32` (1,535) and `32S` (8). This does not prove those handlers are correct under
a new kernel layout. It does mean this aggregate presents no observed missing
relocation kind. New instruction encoding is not the immediate blocker.

## Implementation order

1. Separate output-section file offsets, virtual addresses and load addresses.
   Keep the existing userspace layout as the default. Add high-address,
   nonidentity-load and relocation-overflow comparisons against GNU ld.
2. Implement the syntax used by the pinned expanded script, with explicit
   rejection of unsupported constructs. Add section aggregation, symbol
   expressions, `KEEP`/sorting/discard, PHDR selection, fill and assertions.
   Permit a validated script-defined physical entry. Compare section contents,
   symbol values and program headers against GNU ld on small fixtures first.
3. Add bounded Linux-note preservation and test the real aggregated core
   without deleting metadata or injecting reference symbol values. Run Linux
   `sorttable`, package with the existing external stages, then repeat boot,
   runtime and negative controls.
4. Add whole-archive/group selection and ET_REL output to remove the remaining
   GNU aggregation stage. Thin-archive support is needed for an unchanged
   Kbuild invocation, but the current captured fixture uses verified normal
   archives and does not require it for the first milestone.

The first kernel milestone can consume a GNU-produced ET_REL aggregate while
Keel owns final layout, relocations and ELF emission. That must be described as
native final linking, not a complete native Kbuild toolchain. Packaging,
real-mode code, setup, decompression and vDSO preparation remain separate work.

## Probe reproduction

Build Keel with `cargo build -p keel`. Given the pinned expanded script and
the qualified Keel aggregate produced by the existing GNU partial link:

```sh
target/debug/keel link --target x86_64-unknown-linux-gnu \
  --script=vmlinux.lds aggregate.o -o candidate
target/debug/keel link --target x86_64-unknown-linux-gnu \
  -e startup_64 aggregate.o -o candidate
```

The first command currently rejects the option. The second rejects `.note.Linux`.
Removing only that note from a disposable GNU-objcopy copy reaches the missing
script-symbol failure. That removal is diagnostic only; the qualified kernels
and benchmark images retain their original bytes.

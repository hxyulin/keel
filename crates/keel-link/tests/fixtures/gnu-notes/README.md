# GNU metadata fixtures

The `*.note.*.bin` files are exact section bytes copied from GNU toolchain
inputs and outputs on x86-64 Linux. `provenance.json` records the original
files and the SHA-256 of each extracted section.

The installed binutils package was **2.47-4**, while the `ld`, `as` and
`readelf` banners reported **2.46**; the installed SFrame header matches
upstream 2.47. The GNU outputs serve as an independent property oracle. They
are not confirmed upstream 2.47 executions and not Keel runtime results.

## Property notes

CRT fixtures cover the observed multiple notes and unsorted USED records.
Tests place these note bytes in small purpose-built relocatables to isolate
metadata behavior. They do not strip CRT objects and do not claim full CRT
linking. ABI-tag bytes test preservation and internal local-symbol relocation
identity; the output symbol table remains global-only.

`split-alone` and `split-all` check per-object duplicate reduction before the
explicit surviving-IBT rejection. `split-missing`, `split-usedzero` and
`isazero-gnu` are compared byte for byte with Keel's canonical property note.
These references come from GNU `as` fixtures assembled with
`-mx86-used-note=no`, and were not regenerated with a different toolchain.

The implementation contract follows these primary sources:

- binutils `binutils-2_47` `bfd/elfxx-x86.c`:
  `_bfd_x86_elf_parse_gnu_properties` ORs duplicate values per BFD;
  `_bfd_x86_elf_merge_gnu_properties` supplies the AND/OR/OR_AND rules.
- Linux ABI `object-files.tex`: note/property formats and note alignment.
- x86-64 psABI `dl.tex` (master `e1ce098331da5dbd66e1ffc74162380bcc213236`):
  masks, presence rules and basic-x86 FEATURE_2_USED.
- glibc `sysdeps/x86/dl-prop.h` and Linux `fs/binfmt_elf.c`: loader-visible
  property discovery, bounds and canonical ordering.

Producer compatibility is distinguished from the stricter canonical output
contract.

## Shared-object fixtures

`libreview.so` is a complete, generated ELF64 DSO with two LOADs, DYNAMIC,
SONAME `libreview.so`, a SysV hash and a writable `R_X86_64_64` relocation to
the strong undefined OBJECT `prop`. Tests inspect this interface and mutate
binding only for weak-lookup controls.

`libdefined.so`, `libweak-defined.so`, `libprotected-defined.so` and
`libundefined.so` are built from the matching C sources in this directory.
`compiler-provenance.json` records whole-file hashes and exact commands. Apple
Clang 21 compiled them with

```sh
clang -target x86_64-unknown-linux-gnu -fPIC -fno-asynchronous-unwind-tables \
  -fno-stack-protector -c NAME.c -o NAME.o
```

and Rust LLD 23.1.1 linked them with
`-flavor gnu -shared --hash-style=sysv -soname libNAME.so NAME.o -o libNAME.so`.
The sources use ordinary pointer initializers. Defined GLOBAL/WEAK inputs have
real named `R_X86_64_64` dynamic lookups; the protected control uses RELATIVE.

`got-defined.c`/`libgot-defined.so` and `plt-defined.c`/`libplt-defined.so` use
the same tools and flags and exercise real GLOB_DAT and JUMP_SLOT lookups. The
PLT fixture also has an independent `R_X86_64_64` relocation against `other`
in `.rela.dyn`, so the test must check both runtime tables.

Tests consume the stored ELF bytes and need neither the cross compiler nor LLD.
These are structural preemption controls, not native Linux loader executions.

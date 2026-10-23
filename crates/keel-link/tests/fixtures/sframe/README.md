# Retained installed SFrame fixture

`crt1.o` is an unchanged copy of
`target/elf-system-runtime-20261020/metadata-contract-r1/remote/crt1.o`.
It was retained on 2026-10-20 alongside `installed-sframe.h` and the metadata
contract evidence described in `../gnu-notes/README.md`. No fixture was
regenerated with a newer assembler or draft format. The installed reference
package was binutils 2.47-4; tool banners reported 2.46.

SHA256:

- Whole object: `10ac96ba393f9812f1fa92b6a393a08c4b6f38e6d178320a4f258b4c2454b1ca`
- `.sframe`: `ec9c3950c2a25fd367d9d957c4d3a06f28cc31f4ddfa34a27d8185d7d5a8eba4`
- `.rela.sframe`: `cda46097fc8ebeeb6cb86e793c34bd73278e458d1a123f5c18e755560f9372b5`

The metadata is 102 bytes: 28-byte header, three 16-byte indices, 26 FRE
bytes, and four rows. Its three PC64 RELA entries target the original local
`.text` section symbol with addends 0, 48 and 64. Unit tests require exact
roundtrip and row ownership. Integration tests link the real object with
minimal symbol stubs and inspect final metadata addresses. These tests do
not execute the CRT or claim libc startup or native libsframe runtime coverage.

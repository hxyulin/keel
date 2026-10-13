# keel

An educational compiler backend: a low-level SSA IR, optimizer, aarch64 code
generator, assembler and Mach-O linker.

See [docs/design.md](docs/design.md) for the design decisions.

## Usage

```sh
cargo run -p keel -- compile crates/keel-codegen/tests/programs/return42.keel -o return42.s
clang -o return42 return42.s && ./return42; echo $?   # 42
```

## Development

- `cargo test` runs everything. End-to-end tests need `clang`.
- IR golden tests live in `crates/keel-ir/tests/golden/`; `BLESS=1 cargo test`
  rewrites the expected `.out` files.
- Codegen tests live in `crates/keel-codegen/tests/programs/`: each `.keel`
  file has an `; expect: exit N` or `; expect: trap` line and may have a
  same-named `.c` driver that it is linked with. Each program is compiled
  twice: with the full register set and with a tiny one that forces spills.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/),
  scoped by area: `feat(ir): ...`, `fix(codegen): ...`, `docs: ...`.

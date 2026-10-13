# keel

An educational compiler backend: a low-level SSA IR, optimizer, aarch64 code
generator, assembler and Mach-O linker.

See [docs/design.md](docs/design.md) for the design decisions.

## Usage

```sh
cargo run -p keel -- compile crates/keel-codegen/tests/programs/return42.keel -o return42.s
clang -o return42 return42.s && ./return42; echo $?   # 42

cargo run -p keel -- compile -O input.keel -o out.s   # optimize first
cargo run -p keel -- opt -p gvn,dce input.keel        # run passes, print the IR
cargo run -p keel -- passes                           # list the passes
```

## Development

- `cargo test` runs everything. End-to-end tests need `clang`.
- IR golden tests live in `crates/keel-ir/tests/golden/`; `BLESS=1 cargo test`
  rewrites the expected `.out` files.
- Codegen tests live in `crates/keel-codegen/tests/programs/`: each `.keel`
  file has an `; expect: exit N` or `; expect: trap` line and may have a
  same-named `.c` driver that it is linked with. Each program is compiled
  three times: with the full register set, with a tiny one that forces
  spills, and after the default optimization pipeline. A program that runs
  for more than 10 seconds fails.
- Pass tests live in `crates/keel-opt/tests/golden/`: each starts with a
  `; passes: a, b` line.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/),
  scoped by area: `feat(ir): ...`, `fix(codegen): ...`, `docs: ...`.

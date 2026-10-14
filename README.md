# keel

An educational compiler backend: a low-level SSA IR, optimizer, code
generators for aarch64 macOS and x86-64 Linux, an assembler and Mach-O
linker, and a C compiler on top of it.

See [docs/design.md](docs/design.md) for the design decisions.

## Usage

```sh
cargo run -p keel -- compile crates/keel-codegen/tests/programs/return42.keel -o return42.s
clang -o return42 return42.s && ./return42; echo $?   # 42

cargo run -p keel -- compile -O input.keel -o out.s   # optimize first
cargo run -p keel -- opt -p gvn,dce input.keel        # run passes, print the IR
cargo run -p keel -- passes                           # list the passes
```

C goes through clang's preprocessor first; keel compiles one preprocessed
file to one assembly file:

```sh
clang -E -std=c11 hello.c -o hello.i
cargo run -p keel -- cc -O hello.i -o hello.s         # -g for debug info
clang -o hello hello.s && ./hello                     # clang -g with -g, then lldb
cargo run -p keel -- cc --emit-ir hello.i             # print the keel IR
```

Both commands take `--target aarch64-apple-darwin` or
`--target x86_64-unknown-linux-gnu`; the default is the host.

## Development

- `cargo test` runs everything, on macOS arm64 or x86-64 Linux. End-to-end
  tests need `clang`. A `; target: <triple>` line limits a codegen test
  program to one target.
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
- C tests live in `crates/keel-c/tests/programs/`: each `.c` program is
  built by clang and by keel (as is and with `-O`), and both must print the
  same and exit the same.
- `cargo suite test` builds Lua, SQLite and QuickJS with keel and runs
  their test suites; `cargo suite run` builds them with clang and keel in
  every configuration and times them. Each program is a directory in
  `suite/`; see [suite/README.md](suite/README.md).
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/),
  scoped by area: `feat(ir): ...`, `fix(codegen): ...`, `docs: ...`.

# The suite

Real programs that keel must build correctly and that measure how fast its
code is. Each directory here with a `suite.toml` is one program; the
`keel-suite` tool (`crates/keel-suite`) finds them by scanning.

```sh
cargo suite list                    # suites, benchmarks, configurations
cargo suite test                    # build with keel and keel -O, run the tests
cargo suite test sqlite -c keel-g   # one suite, another configuration
cargo suite run                     # build every configuration, time everything
cargo suite run lua/fib -n 10 -c clang-O0,keel
cargo suite show                    # the latest run on this host again
cargo suite compare aarch64-apple-darwin x86_64-unknown-linux-gnu
```

Sources are downloaded (and checked against their SHA-256) into
`target/suite/<name>/`, and built there in `build/<config>/`. `clang -E`
runs once per suite: every configuration compiles the same preprocessed
files, so the only difference between them is the compiler. clang builds
are cached; keel builds are redone each time.

## Configurations

`configs.toml` lists the ways a program is built, in the order tables show
them: a name, a `compiler` (`clang`: `clang -c` on the preprocessed file;
`keel`: `keel cc`, then clang assembles), and its `flags`. `baseline` is
what benchmark ratios are against; `test` is what `cargo suite test` builds
unless given `-c`.

## A suite

```toml
description = "Lua 5.4.7"

# Archives to download and unpack into the work directory.
[[fetch]]
url = "https://www.lua.org/ftp/lua-5.4.7.tar.gz"
sha256 = "9fbf5e28..."

[build]
dir = "lua-5.4.7/src"       # where `prepare` and the preprocessor run
std = "gnu99"               # clang -E -std=
sources = ["*.c"]           # files, or `*` in a file name
exclude = ["luac.c"]
defines = []                # -D, without the -D
includes = []               # -I, relative to `dir`
libs = ["m"]                # -l, without the -l
prepare = []                # commands run first, e.g. to generate sources
binary = "lua"

# Added on matching hosts: `os` is macos or linux, `arch` aarch64 or x86_64.
[[build.when]]
os = "linux"
defines = ["LUA_USE_LINUX"]
libs = ["dl"]

# Each command must succeed. With `compare`, it must also print exactly
# what the named configuration's build prints.
[test]
dir = "lua-5.4.7-tests"     # relative to the work directory
run = ["{bin} -e _U=true all.lua"]
stdin = "{suite}/input.txt" # optional
compare = "clang-O2"        # optional

# A timed workload. Before timing, every configuration's output must
# equal the baseline's.
[[bench]]
name = "fib"
run = "{bin} {suite}/bench.lua fib"
stdin = "{suite}/input.txt" # optional
```

Commands are split at whitespace and run without a shell. Their
variables:

| variable    | value                                                  |
|-------------|--------------------------------------------------------|
| `{bin}`     | the executable built in the configuration under test   |
| `{suite}`   | this directory, `suite/<name>`                         |
| `{work}`    | the work directory, `target/suite/<name>`              |
| `{gen}`     | `{work}/gen`, for what `prepare` generates             |
| `{defines}` | the `-D` flags, one argument each                      |
| `{libs}`    | the `-l` flags, one argument each                      |

## Results

`cargo suite run` prints a table per suite, the mean time per benchmark
and configuration and its ratio to the baseline, and saves the run as JSON
in `target/keel-suite/results/<target>/`. `show` and `compare` take a
results file or a target triple, meaning that target's latest run. Runs on
one target are compared by time. Runs on two targets are compared by each
configuration's ratio to its own baseline, since the machines differ. To
compare with another machine, copy its `results/<target>` directory over.

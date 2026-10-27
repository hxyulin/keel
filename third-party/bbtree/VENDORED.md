# Vendored bbtree

Source: the crates.io `bbtree` 0.1.5 package, upstream commit
`ff4f37ca199220452d65005742596635ee8fb621` according to its
`.cargo_vcs_info.json`. Repository: <https://github.com/XiangpengHao/bbtree>.
The published crate archive has SHA-256
`5fcbde4ae7859bc7cbeabf81fd0fb834fd135a6167e90828341724e9884f2f25`.
The upstream MIT and Apache 2.0 license texts are preserved in `LICENSE`.
All published Rust sources and tests are included. `README.md` is upstream's.

## Local changes

The optional `cache-line-alignment` feature keeps node sizes, capacities,
slot offsets, and the 12-key minimum unchanged. It lowers allocation alignment
from the full node page size to 64 bytes, unless stored keys, values, child
pointers, or a leaf annex require stricter alignment. Allocation size still
rounds up to a multiple of both page size and alignment.

The linker enables this feature to reduce allocator padding. Node pointers
use only the low bit as a tag, which remains clear at 64-byte alignment.
Compile-time layout checks continue to enforce slot and header alignment.
The upstream layout expectations now account for the feature; over-aligned
key/value tests remain intact, and an added test covers leaf annex alignment.
No unsafe code was added, and the search, split, merge, and removal algorithms
are unchanged. Feature-off layouts retain upstream expectations.

`Cargo.toml` comes from upstream's `Cargo.toml.orig`, with the local feature
and a corrected local README path. The package is excluded from Keel's
workspace so its own test dependencies and checks stay separate.

## Validation

Run the vendored suite in both configurations when updating this copy:

```sh
cargo test --manifest-path third-party/bbtree/Cargo.toml
cargo test --manifest-path third-party/bbtree/Cargo.toml --all-features
cargo test -p keel-link -p keel
```

Compare future updates against the published package first. Preserve this
patch and its tests, and rerun the benchmark in
`suite/native-linker/collections_bench.py`. File maps are feature-dependent
binary layouts; do not exchange persisted maps between configurations.

On 2026-10-25, the default suite passed 387 unit and documentation tests.
The all-features suite passed 411. Keel's linker and CLI packages passed
393 tests, with two ignored.

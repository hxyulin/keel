# LZ4 block-codec workload

Pins unmodified [LZ4 1.10.0](https://github.com/lz4/lz4/releases/tag/v1.10.0)
`lib/lz4.c` and its public header, with the archive SHA-256 in `suite.toml`.
The deterministic driver covers 174 roundtrips and 1,892 assertions over empty,
literal/match thresholds, small and 64 KiB window boundaries, three corpora,
default/fast compression, malformed/truncated input and short output buffers.
Every decoded payload must match its original bytes. Guard bytes check output
bounds. Compression byte streams may differ when both are valid.

Separate compression and decompression workloads each process 786,432,000
uncompressed bytes from text-like, repeated and deterministic pseudorandom
blocks. Inputs are prepared once per process. Startup and preparation remain
inside platform-runner runtime measurements; no LTO or CPU tuning is used.
This is public block-API coverage, not the upstream CLI, frame format,
streaming dictionary, high-compression or multithreading suite.

Run through the standard registry or the platform runner:

```sh
python3 suite/fast_opt_locked.py target/lz4-lock.jsonl \
  python3 suite/benchmark_platforms.py --root . --work target/lz4-run \
  --keel /absolute/path/to/keel --apps lz4 \
  --configs keel-none keel-fast keel-full clang-O2
```

`validate.py` independently checks a pinned existing archive, retains private
source/object/command evidence, and runs Clang O2 followed by Keel None/Fast/Full
correctness. It accepts `--archive`, `--keel`, `--output` and at most two jobs.
The Clang reference must pass before any candidate is validated. Launch errors
and timeouts retain structured command records. Use a private output directory;
the helper can reuse its own hash-validated compile cache. It performs no
performance measurements.

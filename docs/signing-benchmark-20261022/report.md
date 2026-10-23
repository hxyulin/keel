# Keel in-process ad-hoc signing, 2026-10-22

Measured current source at `b13510c25ac280561de189b89721fa823456a365` in the isolated T3 worktree `~/.t3/worktrees/keel/t3-94542143`. No library changes or optimizations. No historical implementation was benchmarked: these results do **not** establish a historical speedup.

## Answer

Across two independent runs, median isolated memory signing is 19 microseconds for a 32,992-byte tiny executable, 0.519–0.648 ms for approximately 1 MiB, 8.294–9.779 ms for approximately 16 MiB, and 37.545–42.772 ms for approximately 64 MiB. Public `write_signed` medians are 0.244–0.276, 1.133–1.256, 14.707–16.004, and 58.210–83.984 ms, respectively. On this host the analogous Apple write/sign/rename path takes 21.216–23.660, 23.561–27.496, 44.243–57.549, and 105.754–121.571 ms. These spans are between the two run medians; the tables below give each run's full sample range.

## Machine and build

Native Darwin ARM64 `machost`, Apple M5 Max, 18 physical/logical CPUs (6 Super + 12 Performance), 128 GiB RAM, macOS 27.0.1 (26A434), Darwin 27.0.0. Local APFS Data volume, Apple Fabric storage. Rust 1.99.0 (`b940084d7`, LLVM 23.1.1), Cargo 1.99.0, Apple clang 21.0.0. Cargo `release` defaults (optimized, no dist/fat LTO, no explicit target-cpu=native); no RUSTFLAGS/CARGO_TARGET_DIR/CARGO_BUILD_TARGET/CARGO_ENCODED_RUSTFLAGS environment overrides. sha2 0.11.1. ARM SHA256 feature is present; executable disassembly contains `sha256h.4s`/`sha256h2.4s` instructions.

This is a busy shared host: observed load averages 43.43 / 36.44 / 40.62 on 18 CPUs between runs. No thermal/performance warnings were reported by pmset. No CPU affinity, frequency locking, exclusive reservation, or cold-cache control. Large outliers and between-run variation are retained. These are warm-cache wall-clock latencies under concurrent work, not clean-machine CPU-time or disk durability measurements. Full captured metadata: [machine.txt](machine.txt).

## Reproduction

From this checkout:

```sh
cargo build --release --locked -p keel-link --example sign_bench
target/release/examples/sign_bench /tmp/keel-sign-bench-20261022
target/release/examples/sign_bench /tmp/keel-sign-bench-20261022-repeat
python3 suite/scripts/summarize_signing.py /tmp/keel-sign-bench-20261022/raw.csv /tmp/keel-sign-bench-20261022-repeat/raw.csv
```

Requires native macOS ARM64 with Command Line Tools (`xcrun clang`), `/usr/bin/codesign`, and Cargo. Choose new output directories for clean reproduction. The 20.17-second initial build is excluded. Harness: [sign_bench.rs](../../crates/keel-link/examples/sign_bench.rs); summary script: [summarize_signing.py](../../suite/scripts/summarize_signing.py). Raw nanosecond samples are [run1.csv](run1.csv) and [run2.csv](run2.csv). `signed_bytes` in CSV means predicted Keel signed size for the image, regardless of the row's mode; Apple output size differs.

Each run has 3 complete warmup cycles then 21 timed repetitions per size/mode; mode order rotates each cycle. No samples discarded. Timers are Rust Instant wall time. CSV writing and verification are outside timed regions. Sizes run tiny to large. Object generation, clang invocation, Keel linking/layout/UUID computation, input reading, and buffer allocation/reset are outside signing timings. This is one long-running release process, with external processes only for the explicitly named Apple/true modes.

## Images

A clang-assembled ARM64 object contains `_main: mov w0,#0; ret`, plus initialized `__TEXT,__const` payload. Payload byte i is `(i*131 + (i>>8)*17 + 29) & 255`, deterministic and fully materialized (not zero-fill/BSS). Keel `link` produces each valid unsigned Mach-O once, with libSystem and no debug objects. Exact unsigned sizes: 32,992; 1,081,568; 16,810,208; 67,141,856 bytes. Corresponding Keel signed sizes: 33,408; 1,090,176; 16,941,696; 67,666,560 bytes. Payload labels are not exact file sizes.

## What each mode measures

- `core_memory`: exact private `sign.rs` included by source path, using preallocated Cursor<Vec<u8>>. Timer covers header validation/patch, signature metadata, per-page SHA256, and signature writes. Fresh unsigned bytes copied/reset before the timer; no image-sized allocations, file I/O, linking, or process startup in timer. Generic Cursor specialization may optimize differently from File specialization.
- `core_file`: same signer on an already-open read/write File containing fresh unsigned bytes. Creation/write/chmod/open and final close excluded. Includes signing reads, seeks, header patch and signature writes through the filesystem/page cache.
- `unsigned_atomic`: harness copy of install_output's create_new sibling-temp/write/chmod/close/rename path, omitting signing, with empty debug objects. This provides a no-signing baseline comparable to `write_signed`; it is not a public unsigned API. Timed chmod matches the signed path. Both replace an existing destination after warmups.
- `write_signed`: actual public API on the prelinked Executable. Includes empty debug-object handling, sibling temporary file creation, full unsigned image write, File signing, chmod 0755, close and atomic rename. No process startup or link layout.
- `apple_sign`: `/usr/bin/codesign --force --sign - PATH`, launched and waited with Command::output. Unsigned image file creation/chmod excluded. Includes process launch, pipes/output collection, Apple's file open/read/hash/metadata/write/close and any OS service work. Input is a new unsigned inode each time, not re-signing an already-signed image.
- `apple_atomic`: fresh temporary unsigned write/chmod, same codesign child command and wait, then atomic rename. Includes create/write/process/sign/install like `write_signed`, but the baseline uses a fixed harness-only temp name rather than Keel's create_new collision loop. All setup is in one native harness, without a shell or cp subprocess.
- `spawn_true`: `/usr/bin/true` through the same Command::output mechanism. Observed median 2.0–2.7 ms. This is context for process launch/wait, not an exact subtraction of codesign startup; codesign has its own loading/initialization and services.

All modes start from equivalent exact unsigned Keel bytes for that size. Core File and Apple-only unsigned file preparation unlinks the previous file first; atomic modes write a new temporary inode and replace the old destination. Preparation heats caches. No fsync/F_FULLFSYNC, cache purge, or sustained disk-throughput test. Subtracting independent medians is not an exact decomposition: unsigned/core/signed timings are separately scheduled, and range overlap can be large.

`link_signed` was inspected: it uses the same install_output and signer, but emits via link_to directly into the temp file. It avoids the complete Executable buffer. It is not separately timed here because doing so would include object parsing/layout/emission, which this question asks to exclude from isolated signing. `write_signed` is the measured public file-output API.

## Results

Each cell is median [minimum–maximum], in milliseconds, n=21. Ranges include scheduling outliers.

docs/signing-benchmark-20261022/run1.csv: median [min–max], milliseconds

| Payload | core_memory | core_file | unsigned_atomic | write_signed | apple_sign | apple_atomic | spawn_true |
|---|---:|---:|---:|---:|---:|---:|---:|
| tiny | 0.019 [0.019–0.034] | 0.047 [0.039–0.167] | 0.260 [0.218–0.530] | 0.276 [0.226–0.756] | 23.654 [21.095–46.857] | 23.660 [21.402–45.726] | 2.400 [1.656–5.063] |
| 1MiB | 0.648 [0.472–1.044] | 0.886 [0.706–12.545] | 0.520 [0.395–4.445] | 1.256 [1.042–8.578] | 26.720 [22.946–46.717] | 27.496 [24.409–38.794] | 2.688 [1.784–5.200] |
| 16MiB | 9.779 [7.942–25.274] | 12.758 [11.591–29.855] | 2.702 [2.255–3.529] | 16.004 [14.673–58.540] | 52.468 [41.121–170.532] | 57.549 [44.770–163.775] | 2.099 [1.726–10.231] |
| 64MiB | 42.772 [31.937–95.386] | 60.039 [47.185–151.487] | 8.852 [7.685–10.579] | 83.984 [53.206–208.699] | 109.944 [91.264–145.817] | 121.571 [106.381–258.616] | 2.092 [1.651–10.342] |

docs/signing-benchmark-20261022/run2.csv: median [min–max], milliseconds

| Payload | core_memory | core_file | unsigned_atomic | write_signed | apple_sign | apple_atomic | spawn_true |
|---|---:|---:|---:|---:|---:|---:|---:|
| tiny | 0.019 [0.019–0.021] | 0.041 [0.039–0.057] | 0.232 [0.208–0.354] | 0.244 [0.218–0.295] | 21.128 [19.891–49.633] | 21.216 [19.785–30.880] | 2.636 [1.624–5.006] |
| 1MiB | 0.519 [0.469–1.367] | 0.886 [0.685–0.924] | 0.428 [0.387–7.305] | 1.133 [1.009–2.486] | 23.116 [20.207–34.883] | 23.561 [21.261–56.372] | 2.139 [1.516–18.357] |
| 16MiB | 8.294 [7.526–17.649] | 12.264 [11.125–16.852] | 2.515 [2.243–3.344] | 14.707 [13.706–25.633] | 41.606 [37.836–66.783] | 44.243 [40.386–50.155] | 2.043 [1.518–3.583] |
| 64MiB | 37.545 [31.794–47.957] | 49.435 [44.894–58.330] | 8.318 [7.699–15.322] | 58.210 [52.133–116.585] | 96.665 [88.716–108.609] | 105.754 [100.489–116.474] | 2.054 [1.550–4.363] |

## Throughput and format differences

Using actual unsigned bytes divided by median time, memory signing at 16/64 MiB delivers approximately 1.46–1.89 GiB/s across runs; File core is approximately 1.04–1.28 GiB/s; full write_signed approximately 0.745–1.074 GiB/s at 64 MiB. Tiny images are dominated by fixed API/filesystem/process costs, so throughput there is not informative.

The implementation reads the 16 KiB header, validates and patches load commands, then hashes all signed bytes including that patched header. It batches at most 16 x 4096-byte pages into a 65,536-byte read buffer and emits up to 512 bytes of hashes per batch; each 4 KiB page is an independent SHA256 digest (including proper short-page hashing). The hash table costs approximately 0.78125% of image size plus a 128-byte fixed signature header. Temporary signing buffers are bounded (16 KiB header, 64 KiB pages, 512-byte hashes, 128-byte signature header), not image-sized. At 64 MiB there are 16,393 page digests and 1,025 read/hash-write batches, with seeks between input and signature output. Memory timing removes syscall overhead; File timing retains those seeks/reads/writes. This work is sequential in one thread with ARM SHA instructions available.

Apple's default codesign on this host uses SHA256 with **16 KiB** pages, while Keel uses **4 KiB**. At 64 MiB, inspection reports Apple 4,099+2 hashes versus Keel 16,393+0 hashes. Apple produces a 67,273,296-byte output versus Keel 67,666,560, and a filename/UUID-derived identifier plus requirements; Keel emits identifier `keel.native` and adhoc|linker-signed flags. Apple is a general signing tool with different metadata and startup work. Thus these are practical default-path comparisons on equivalent starting images, not identical-format hashing microbenchmarks. No historical implementation was measured and no old-versus-new speedup is asserted.

Inspection commands:

```sh
/usr/bin/codesign -d --verbose=4 /tmp/keel-sign-bench-20261022/64MiB/core_memory
/usr/bin/codesign -d --verbose=4 /tmp/keel-sign-bench-20261022/64MiB/apple_sign
sysctl -n hw.optional.arm.FEAT_SHA256
otool -tv target/release/examples/sign_bench | rg -m 4 'sha256[h|s]'
```

## Correctness checks

Both runs completed successfully. `cargo test --release --locked -p keel-link --example sign_bench` also passed the three existing signer unit tests included by the harness. Signature metadata is retained in [signature-inspection.txt](signature-inspection.txt). At each size, the final sample from core_memory, core_file, write_signed, apple_sign and apple_atomic passed `/usr/bin/codesign --verify --strict --verbose=2 PATH` (40 strict checks across two runs). All five signed tiny variants executed natively and returned zero in both runs (10 executions). Memory-core, File-core and public write_signed output bytes matched exactly at every size in both runs. Verification is outside the timer and covers retained samples, not every individual timed iteration. Generated images, objects, payloads, and executable samples remain in the two `/tmp` output directories; raw measurements and harness are retained in the repository. No PR, push, shared-root changes, or optimization edits.

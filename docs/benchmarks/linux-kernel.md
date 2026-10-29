# Pinned kernel size and runtime comparison

The qualified Keel core image is 73.4% larger as a compressed `bzImage` and takes
1.52–2.38 times as long on five checked kernel-heavy microbenchmarks as the
pinned GCC image. Guest init-entry timing is similar. These measurements are
for the minimal Linux v6.12 captured core, not a general Linux performance claim.

The [integration scope](../integrations/linux-kernel.md#scope-and-limits) applies.

## Method

Both images use the pinned source, resolved configuration and external boot
stages. GCC 14.4.0 built the reference with the configuration's `-Os`. Keel
checkpoint `8cd387c8` used `-O --code-model kernel --link-intent executable` and
the explicit kernel instruction policy. This compares the current configurations;
it is not a comparison of equivalent optimizer implementations or tuning flags.

The host is an AMD Ryzen 9 9950X3D Linux workstation. QEMU 11.1.2 uses KVM,
one `qemu64` vCPU and 128 MiB RAM. The whole VM is pinned to one logical CPU.
KVM executes native x86 instructions; this is not TCG and not bare-metal boot.
The host governor remains `powersave`, boost remains unchanged, and the CPU is
not isolated. The feature mask and one-vCPU configuration bound the result.
No host reboot, package installation or persistent host tuning was performed.

One identical freestanding userspace binary, built with GCC 16.2.1, runs in
both kernels. It uses direct syscalls, no libc, no SSE and no vDSO timing. One
excluded whole-guest warmup per image precedes nine measured pairs. Pair order
alternates. All guests run sequentially. Workloads check data and syscall
results; a missing result, wrong operation count, FAIL marker or wrong exit
rejects the entire run.

## Size

| Metric | GCC | Keel | Keel / GCC |
| --- | ---: | ---: | ---: |
| Compressed `bzImage`, bytes | 926,720 | 1,606,656 | 1.734 |
| `vmlinux` file, bytes | 10,242,096 | 14,940,344 | 1.459 |
| Sum of allocated sections, bytes | 6,614,682 | 11,199,029 | 1.693 |
| Sum of executable sections, bytes | 4,326,770 | 8,777,355 | 2.029 |

Allocated sizes include BSS and section padding, exclude gaps between sections,
and are not measured resident memory. Executable-section sizes include padding.
In particular, the script rounds `.text` to 4 MiB for GCC and 8 MiB for Keel.
Keel also retains a 418,380-byte `.eh_frame` orphan section absent from GCC.
## Runtime

Times below are medians across nine boots per image. Ratios are medians of the
nine paired Keel/GCC ratios, so they need not equal the quotient of medians.
Each timed interval uses a direct `CLOCK_MONOTONIC` syscall in the guest.
Console output happens outside the timed interval.

| Checked workload | Work per boot | GCC ms | Keel ms | Paired time ratio |
| --- | ---: | ---: | ---: | ---: |
| `getpid` | 5,000,000 syscalls | 165.053 | 251.275 | 1.522 |
| Open/read ELF header/close | 300,000 iterations | 92.560 | 220.573 | 2.383 |
| Map, fault, protect, verify and unmap | 5,120 one-MiB mappings, 1,310,720 pages | 410.876 | 889.076 | 2.166 |
| Fork, stack COW and wait | 5,000 children | 29.756 | 61.411 | 2.063 |
| Two-process pipe ping-pong | 200,000 round trips, eight-byte payload | 251.196 | 437.590 | 1.744 |

Every child status, pipe payload, ELF header and touched page is checked. The
fork test also verifies the parent's stack value survives child writes. File
reads use the initramfs, not physical storage. The fixture has no network or
block-device workloads. Two earlier batches showed the same direction and
similar ratios; they are not pooled into these final nine pairs.

Guest monotonic time at the first clock read in init is 259.168 ms for GCC and
256.766 ms for Keel. It excludes firmware, decompression and startup before
kernel timekeeping. Host QEMU launch-to-ready medians are 448.751 and 442.164 ms,
with wide ranges of 401.553–579.989 and 398.564–590.815 ms. These measurements
do not establish a startup speedup. Boot is close enough here that startup
delays and VM overhead can dominate compiler differences.

A deliberately incorrect PID expectation returned the normal FAIL marker and
exit 35 on both kernels, and the checker rejected both.

## Reproduction

On an x86-64 Linux host with accessible `/dev/kvm`, GCC and QEMU:

```sh
python3 suite/integration/linux-kernel/bench.py \
  --gcc-kernel /path/to/reference/bzImage \
  --keel-kernel /path/to/qualified/bzImage \
  --cpu 2 --repeats 9 --out /path/to/new-results
python3 -m unittest discover -s suite/integration/linux-kernel -p 'test_*.py'
size reference-vmlinux keel-vmlinux
size -A reference-vmlinux keel-vmlinux
```

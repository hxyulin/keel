#!/usr/bin/env python3
"""Build and boot the pinned minimal kernel with an external reference toolchain."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import time

HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def initramfs(init):
    """Deterministic newc archive, including /dev/console without host mknod."""
    stream = io.BytesIO()
    entries = [('dev', stat.S_IFDIR | 0o755, b'', 0, 0),
               ('dev/console', stat.S_IFCHR | 0o600, b'', 5, 1),
               ('init', stat.S_IFREG | 0o755, init, 0, 0),
               ('TRAILER!!!', 0, b'', 0, 0)]
    for inode, (name, mode, data, major, minor) in enumerate(entries, 1):
        name = name.encode() + b'\0'
        values = [inode, mode, 0, 0, 1, 0, len(data), 0, 0,
                  major, minor, len(name), 0]
        stream.write(b'070701' + ''.join(f'{x:08x}' for x in values).encode())
        stream.write(name)
        stream.write(b'\0' * (-stream.tell() % 4))
        stream.write(data)
        stream.write(b'\0' * (-stream.tell() % 4))
    return gzip.compress(stream.getvalue(), mtime=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cc', default='gcc')
    parser.add_argument('--hostcc', default='gcc')
    parser.add_argument('--make', default='make')
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--jobs', type=int, default=8)
    args = parser.parse_args()
    source = args.source.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    build = out / 'build'
    pin = json.loads((HERE / 'source.json').read_text())
    if sha(source / 'Makefile') != pin['makefile_sha256']:
        raise SystemExit('Source Makefile differs from the pinned Linux release.')
    env = dict(os.environ, SOURCE_DATE_EPOCH='1731801600', KBUILD_BUILD_USER='keel',
               KBUILD_BUILD_HOST='reference', KBUILD_BUILD_VERSION='1',
               KBUILD_BUILD_TIMESTAMP='Sun Nov 17 00:00:00 UTC 2024')
    report = dict(source=pin, seed_sha256=sha(HERE / 'minimal.config'),
                  compiler=subprocess.check_output([args.cc, '--version'], text=True).splitlines()[0],
                  host_compiler=subprocess.check_output([args.hostcc, '--version'], text=True).splitlines()[0],
                  assembler=subprocess.check_output(['as', '--version'], text=True).splitlines()[0],
                  linker=subprocess.check_output(['ld', '--version'], text=True).splitlines()[0],
                  qemu=subprocess.check_output([args.qemu, '--version'], text=True).splitlines()[0])

    def run(name, command):
        start = time.monotonic()
        with (out / (name + '.log')).open('wb') as log:
            result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, env=env)
        report[name] = dict(returncode=result.returncode,
                            seconds=round(time.monotonic() - start, 3))
        (out / 'reference.json').write_text(json.dumps(report, indent=2) + '\n')
        if result.returncode:
            raise SystemExit(f'{name} failed; see {out / (name + ".log")}')

    make = [args.make, '-C', str(source), 'O=' + str(build), 'ARCH=x86_64',
            'CC=' + args.cc, 'HOSTCC=' + args.hostcc]
    run('configure', make + ['KCONFIG_ALLCONFIG=' + str(HERE / 'minimal.config'), 'allnoconfig'])
    if sha(build / '.config') != sha(HERE / 'resolved.config'):
        raise SystemExit('Resolved config differs from resolved.config; inspect compiler/capability differences before changing the pin.')
    run('build', make + ['-j' + str(args.jobs), 'KCFLAGS=-save-temps=obj',
                         'KAFLAGS=-save-temps=obj', 'bzImage'])
    report['config_sha256'] = sha(build / '.config')
    report['vmlinux_sha256'] = sha(build / 'vmlinux')
    report['bzimage_sha256'] = sha(build / 'arch/x86/boot/bzImage')
    run('init', [args.cc, '-O2', '-static', '-nostdlib', '-ffreestanding', '-fno-pie',
                 '-no-pie', '-fno-stack-protector', '-Wl,--build-id=none', '-Wl,-e,_start',
                 str(HERE / 'init.c'), '-o', str(out / 'init')])
    (out / 'initramfs.cpio.gz').write_bytes(initramfs((out / 'init').read_bytes()))
    command = [args.qemu, '-accel', 'tcg', '-machine', 'pc', '-cpu', 'qemu64',
               '-m', '128M', '-smp', '1', '-display', 'none', '-serial', 'stdio',
               '-monitor', 'none', '-no-reboot', '-device',
               'isa-debug-exit,iobase=0xf4,iosize=4', '-kernel',
               str(build / 'arch/x86/boot/bzImage'), '-initrd',
               str(out / 'initramfs.cpio.gz'), '-append',
               'console=ttyS0,115200 rdinit=/init panic=-1']
    try:
        result = subprocess.run(command, capture_output=True, timeout=60)
        transcript = result.stdout + result.stderr
        passed = (result.returncode == 33 and
                  transcript.replace(b'\r\n', b'\n').splitlines().count(
                      b'KEEL_LINUX64_REFERENCE_OK') == 1)
        report['boot'] = dict(returncode=result.returncode, passed=passed)
    except subprocess.TimeoutExpired as error:
        transcript = (error.stdout or b'') + (error.stderr or b'')
        report['boot'] = dict(returncode=None, passed=False, timeout=True)
    (out / 'boot.log').write_bytes(transcript)
    (out / 'reference.json').write_text(json.dumps(report, indent=2) + '\n')
    if not report['boot']['passed']:
        raise SystemExit('Reference boot failed; see boot.log')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()

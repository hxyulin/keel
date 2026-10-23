#!/usr/bin/env python3
"""Finite multiboot CPL0 guest; all generated files live under /tmp."""
import argparse
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
READY = b'KEEL CPL0 LONG64\n'
PASS = b'KEEL PASS\n'
FAIL = b'KEEL FAIL\n'


class HarnessError(Exception):
    pass


def tool(name):
    found = shutil.which(name)
    if not found:
        raise HarnessError(f'missing prerequisite: {name}')
    return found


def validate_payload(path):
    raw = path.read_bytes()
    if (len(raw) < 64 or raw[:7] != b'\x7fELF\x02\x01\x01'
            or struct.unpack_from('<HH', raw, 16) != (1, 62)):
        raise HarnessError(f'payload must be relocatable little-endian ELF64 x86-64: {path}')


def validate_flat(raw):
    # objcopy must preserve the address-bearing multiboot header at offset zero.
    if len(raw) < 32:
        raise HarnessError('truncated multiboot image')
    magic, flags, checksum, header, start, end, bss, entry = struct.unpack_from('<8I', raw)
    if (magic != 0x1badb002 or flags != 0x10003
            or (magic + flags + checksum) & 0xffffffff
            or header != start or start != 0x100000
            or end - start != len(raw) or not end <= bss < 0x40000000
            or not start + 32 <= entry < end):
        raise HarnessError('invalid flat multiboot placement/bounds')


def build(work, payloads, assembler, linker, objcopy):
    objects = []
    for name in ['boot', 'harness'] + ([] if payloads else ['smoke']):
        obj = work / (name + '.o')
        checked([assembler, '--64', '-o', str(obj), str(HERE / (name + '.S'))])
        objects.append(obj)
    for payload in payloads:
        validate_payload(payload)
    elf, image = work / 'guest.elf', work / 'guest.bin'
    checked([linker, '-m', 'elf_x86_64', '-T', str(HERE / 'link.ld'),
             '-o', str(elf), *map(str, objects + payloads)])
    checked([objcopy, '-O', 'binary', str(elf), str(image)])
    validate_flat(image.read_bytes())
    return image


def checked(argv):
    result = subprocess.run(argv, capture_output=True, timeout=20)
    if result.returncode:
        raise HarnessError(f'build command failed ({result.returncode}): {argv!r}: '
                           + result.stderr.decode(errors='replace'))


def qemu_command(qemu, image, transcript):
    return [qemu, '-no-user-config', '-nodefaults', '-machine', 'pc',
            '-accel', 'tcg,thread=single', '-cpu', 'qemu64', '-m', '32M', '-smp', '1',
            '-display', 'none', '-monitor', 'none', '-serial', 'none', '-nic', 'none',
            '-no-reboot', '-kernel', str(image),
            '-chardev', f'file,id=status,path={transcript}',
            '-device', 'isa-debugcon,iobase=0xf1,chardev=status',
            '-device', 'isa-debug-exit,iobase=0xf4,iosize=4']


def verdict(returncode, transcript, timed_out=False):
    if timed_out:
        return 'timeout'
    if (returncode == 33 and transcript.startswith(READY)
            and transcript.endswith(PASS) and transcript.count(READY) == 1
            and transcript.count(PASS) == 1 and FAIL not in transcript):
        return 'pass'
    return 'guest-error'


def execute(qemu, image, work, timeout):
    transcript = work / 'debugcon.txt'
    argv = qemu_command(qemu, image, transcript)
    timed_out, returncode = False, None
    with (work / 'qemu.stdout').open('wb') as out, (work / 'qemu.stderr').open('wb') as err:
        try:
            returncode = subprocess.run(argv, stdout=out, stderr=err, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            # subprocess.run kills and waits for the child before raising.
            timed_out = True
    data = transcript.read_bytes() if transcript.exists() else b''
    return dict(status=verdict(returncode, data, timed_out), returncode=returncode,
                command=argv, transcript=data.decode(errors='replace'), evidence='emulated-tcg')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--payload', type=Path, action='append', default=[],
                        help='ELF64 relocatable object(s) defining guest_main; default: smoke.S')
    parser.add_argument('--as', dest='assembler', default='as')
    parser.add_argument('--ld', dest='linker', default='ld')
    parser.add_argument('--objcopy', default='objcopy')
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--timeout', type=float, default=5)
    parser.add_argument('--build-only', action='store_true')
    args = parser.parse_args()
    result = {}
    try:
        if not 0 < args.timeout <= 30:
            raise HarnessError('timeout must be positive and at most 30 seconds')
        for payload in args.payload:
            validate_payload(payload)
        assembler, linker, objcopy = map(tool, [args.assembler, args.linker, args.objcopy])
        qemu = None if args.build_only else tool(args.qemu)
        work = Path(tempfile.mkdtemp(prefix='keel-cpl0-', dir='/tmp'))
        result['artifacts'] = str(work)
        image = build(work, args.payload, assembler, linker, objcopy)
        if args.build_only:
            result.update(status='built', evidence='assembly-link-only')
        else:
            result.update(execute(qemu, image, work, args.timeout))
    except (HarnessError, OSError, subprocess.TimeoutExpired) as exc:
        result.update(status='error', error=str(exc))
    print(json.dumps(result, sort_keys=True))
    return 0 if result['status'] in ('pass', 'built') else 1


if __name__ == '__main__':
    raise SystemExit(main())

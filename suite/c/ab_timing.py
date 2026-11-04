#!/usr/bin/env python3
"""Timing loop for `cargo suite ab`.

keel-suite builds, checks and pads the images, then writes a plan. This script
runs every benchmark in palindromic blocks (each arm twice, in mirrored order)
and writes the raw runs. keel-suite parses the counters and computes the
statistics; this script computes none.

On Linux each placement slot is one executable path and inode. The controller
keeps a read-only shared mapping of every file-backed PT_LOAD page of the slot
locked, and overwrites the file in place between runs, so every arm in a slot
runs from the same page-cache pages. Children run with ASLR disabled
(`setarch -R`) and, with a CPU in the plan, pinned (`taskset`). On macOS a
signed executable cannot be rewritten in place, so each arm gets its own copy
with an equal-length path, and ASLR and affinity stay as the system sets them.

Usage: ab_timing.py PLAN OUTPUT
"""
import ctypes
import hashlib
import json
import mmap
import os
from pathlib import Path
import platform
import random
import struct
import subprocess
import sys
import time

PAGE = 4096
BIN = '@BIN@'
COUNTERS = '@COUNTERS@'


def round_up(value, unit=PAGE):
    return (value + unit - 1) // unit * unit


def load_extent(data):
    """The end of the last file-backed PT_LOAD segment of an ELF64 image."""
    if data[:6] != b'\x7fELF\x02\x01':
        raise ValueError('not a little-endian ELF64 image')
    offset = struct.unpack_from('<Q', data, 32)[0]
    width, count = struct.unpack_from('<HH', data, 54)
    ends = []
    for i in range(count):
        header = offset + i * width
        if struct.unpack_from('<I', data, header)[0] == 1:
            file_offset = struct.unpack_from('<Q', data, header + 8)[0]
            file_size = struct.unpack_from('<Q', data, header + 32)[0]
            ends.append(file_offset + file_size)
    if not ends:
        raise ValueError('ELF image has no PT_LOAD segment')
    return max(ends)


class _Buffer(ctypes.Structure):
    _fields_ = [('buf', ctypes.c_void_p), ('obj', ctypes.c_void_p), ('len', ctypes.c_ssize_t),
                ('itemsize', ctypes.c_ssize_t), ('readonly', ctypes.c_int), ('ndim', ctypes.c_int),
                ('format', ctypes.c_void_p), ('shape', ctypes.c_void_p), ('strides', ctypes.c_void_p),
                ('suboffsets', ctypes.c_void_p), ('internal', ctypes.c_void_p)]


def _mlock(mapping, length):
    view = _Buffer()
    if ctypes.pythonapi.PyObject_GetBuffer(ctypes.py_object(mapping), ctypes.byref(view), 0):
        raise OSError('cannot address the slot mapping')
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.mlock(ctypes.c_void_p(view.buf), ctypes.c_size_t(length)):
            error = ctypes.get_errno()
            raise OSError(error, 'mlock of %d bytes failed (%s); raise RLIMIT_MEMLOCK (ulimit -l)'
                          % (length, os.strerror(error)))
    finally:
        ctypes.pythonapi.PyBuffer_Release(ctypes.byref(view))


def slot_extent(images):
    """The file size a slot needs for `images`, and how much of it to map."""
    size = round_up(max(len(image) for image in images))
    return size, min(size, round_up(max(load_extent(image) for image in images)))


def check_lock_limit(plan):
    """Fails before any timing if one application's slots cannot all be locked."""
    if plan['slots'] != 'fixed-inode' or not plan['lock']:
        return
    import resource
    limit = resource.getrlimit(resource.RLIMIT_MEMLOCK)[0]
    if limit == resource.RLIM_INFINITY:
        return
    for app in plan['apps']:
        need = sum(slot_extent([Path(p).read_bytes() for p in placement.values()])[1]
                   for placement in app['placements'])
        if need > limit:
            sys.exit('%s needs %d KiB of locked memory for %d placements, but ulimit -l is %d KiB;'
                     ' raise the limit or pass --no-lock'
                     % (app['suite'], need // 1024, len(app['placements']), limit // 1024))


class FixedSlot:
    """One executable path and inode that every image is written into in place."""

    def __init__(self, path, images, lock):
        self.path = Path(path)
        self.size, self.extent = slot_extent(images)
        self.install(images[0], create=True)
        self.inode = self.path.stat().st_ino
        fd = os.open(self.path, os.O_RDONLY)
        try:
            self.mapping = mmap.mmap(fd, self.extent, flags=mmap.MAP_SHARED, prot=mmap.PROT_READ)
        finally:
            os.close(fd)
        for offset in range(0, self.extent, PAGE):
            self.mapping[offset]
        self.locked = False
        if lock:
            _mlock(self.mapping, self.extent)
            self.locked = True

    def install(self, image, create=False):
        """Overwrites the slot without renaming or truncating it."""
        padded = image + bytes(self.size - len(image))
        if create:
            self.path.write_bytes(padded)
            self.path.chmod(0o755)
            return
        # The descriptor is closed before any child runs, so execve cannot see ETXTBSY.
        with self.path.open('r+b', buffering=0) as f:
            if f.write(padded) != self.size:
                raise OSError('short write to ' + str(self.path))
        if self.path.stat().st_ino != self.inode:
            raise OSError(str(self.path) + ' changed inode')

    def close(self):
        self.mapping.close()


def block_order(rng, arms):
    """A shuffled order followed by its mirror: every arm runs twice, and the
    mean position of every arm is the same, which cancels linear drift."""
    order = list(arms)
    rng.shuffle(order)
    return order + order[::-1]


def child_argv(plan, argv, executable, counter_file):
    """The benchmark command with its counter wrapper, affinity and ASLR control."""
    command = [counter_file if word == COUNTERS else word for word in plan['counter']]
    command += [word.replace(BIN, executable) for word in argv]
    if plan['platform'] == 'linux':
        command = ['setarch', platform.machine(), '-R', *command]
        if plan.get('cpu') is not None:
            command = ['taskset', '-c', str(plan['cpu']), *command]
    return command


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run_once(plan, bench, executable, counter_file):
    argv = child_argv(plan, bench['argv'], executable, counter_file)
    stdin = Path(bench['stdin']).read_bytes() if bench.get('stdin') else None
    start = time.perf_counter_ns()
    proc = subprocess.run(argv, cwd=bench['cwd'], input=stdin, capture_output=True, timeout=600)
    wall = time.perf_counter_ns() - start
    expected = bench['expected']
    got = {'status': proc.returncode, 'stdout_sha256': digest(proc.stdout),
           'stderr_sha256': digest(proc.stderr)}
    if got != expected:
        raise RuntimeError('%s printed differently from its checked output: %s instead of %s\n%s'
                           % (bench['name'], got, expected, proc.stderr.decode(errors='replace')[-2000:]))
    counters = Path(counter_file).read_text() if plan['counter'] else ''
    return wall, counters


def load_average():
    try:
        return list(os.getloadavg())
    except OSError:
        return None


def run_app(plan, app, rng, out):
    """Times every benchmark of one application over all placements."""
    work = Path(plan['work'])
    arms = plan['arms']
    images = [{arm: Path(p[arm]).read_bytes() for arm in arms} for p in app['placements']]
    slots = []
    paths = []
    try:
        for index, placement in enumerate(images):
            if plan['slots'] == 'fixed-inode':
                path = work / ('%s-slot-%d' % (app['suite'], index))
                slot = FixedSlot(path, list(placement.values()), plan['lock'])
                slots.append(slot)
                paths.append({arm: str(path) for arm in arms})
                out['slots'].append({'suite': app['suite'], 'placement': index, 'path': str(path),
                                     'inode': slot.inode, 'locked_bytes': slot.extent if slot.locked else 0})
            else:
                copies = {}
                for number, arm in enumerate(arms):
                    path = work / ('%s-%d-arm%d' % (app['suite'], index, number))
                    path.write_bytes(placement[arm])
                    path.chmod(0o755)
                    copies[arm] = str(path)
                paths.append(copies)
        counter_file = str(work / 'counters.txt')
        for bench in app['benches']:
            for round_index in range(-plan['warmup'], plan['rounds']):
                placements = list(range(len(images)))
                rng.shuffle(placements)
                for placement in placements:
                    order = block_order(rng, arms)
                    load = load_average()
                    for position, arm in enumerate(order):
                        if slots:
                            slots[placement].install(images[placement][arm])
                        wall, counters = run_once(plan, bench, paths[placement][arm], counter_file)
                        if round_index >= 0:
                            out['runs'].append({
                                'suite': app['suite'], 'bench': bench['name'], 'round': round_index,
                                'placement': placement, 'arm': arm, 'position': position,
                                'wall_ns': wall, 'counters': counters, 'load': load})
            print('timed %s/%s' % (app['suite'], bench['name']), flush=True)
        # Leave each slot holding its baseline image.
        for slot, placement in zip(slots, images):
            slot.install(placement[arms[0]])
        for placement, original in zip(app['placements'], images):
            for arm in arms:
                if Path(placement[arm]).read_bytes() != original[arm]:
                    raise RuntimeError(placement[arm] + ' changed during timing')
    finally:
        for slot in slots:
            slot.close()


def main(argv):
    if len(argv) != 3:
        sys.exit(__doc__)
    plan = json.loads(Path(argv[1]).read_text())
    if plan.get('schema') != 1:
        sys.exit('unsupported plan schema')
    if plan['platform'] == 'linux' and plan.get('controller_cpu') is not None:
        os.sched_setaffinity(0, {plan['controller_cpu']})
    check_lock_limit(plan)
    Path(plan['work']).mkdir(parents=True, exist_ok=True)
    rng = random.Random(plan['seed'])
    out = {'schema': 1, 'started_ns': time.time_ns(), 'load_before': load_average(),
           'slots': [], 'runs': []}
    for app in plan['apps']:
        run_app(plan, app, rng, out)
    out['load_after'] = load_average()
    out['finished_ns'] = time.time_ns()
    temp = Path(argv[2] + '.tmp')
    temp.write_text(json.dumps(out) + '\n')
    temp.replace(argv[2])


if __name__ == '__main__':
    main(sys.argv)

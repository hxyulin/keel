"""Tests for the `cargo suite ab` timing loop, without real benchmarks."""
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile
import unittest

import ab_timing


def elf(segments, size=8192):
    """A minimal ELF64 image whose program headers are (type, offset, filesz)."""
    data = bytearray(size)
    data[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<Q', data, 32, 64)
    struct.pack_into('<HH', data, 54, 56, len(segments))
    for i, (kind, offset, filesz) in enumerate(segments):
        header = 64 + i * 56
        struct.pack_into('<I', data, header, kind)
        struct.pack_into('<Q', data, header + 8, offset)
        struct.pack_into('<Q', data, header + 32, filesz)
    return bytes(data)


def script(text):
    return ('#!/bin/sh\n' + text + '\n').encode()


class Extent(unittest.TestCase):
    def test_largest_file_backed_load_segment(self):
        image = elf([(1, 0, 0x800), (6, 0x40, 0x5000), (1, 0x1000, 0x234)])
        self.assertEqual(ab_timing.load_extent(image), 0x1234)

    def test_rejects_other_formats_and_images_without_loads(self):
        with self.assertRaises(ValueError):
            ab_timing.load_extent(b'\xcf\xfa\xed\xfe' + bytes(60))
        with self.assertRaises(ValueError):
            ab_timing.load_extent(elf([(6, 0, 16)]))


class LockLimit(unittest.TestCase):
    def test_reports_an_application_that_cannot_be_locked(self):
        import resource
        limit = resource.getrlimit(resource.RLIMIT_MEMLOCK)[0]
        if limit == resource.RLIM_INFINITY or limit > 1 << 26:
            self.skipTest('the locked-memory limit is too large to exceed')
        with tempfile.TemporaryDirectory() as temporary:
            image = Path(temporary) / 'image'
            image.write_bytes(elf([(1, 0, limit + 1)], limit + 8192))
            plan = {'slots': 'fixed-inode', 'lock': True,
                    'apps': [{'suite': 'big', 'placements': [{'baseline': str(image)}]}]}
            with self.assertRaises(SystemExit) as raised:
                ab_timing.check_lock_limit(plan)
            self.assertIn('big needs', str(raised.exception.code))
            plan['lock'] = False
            ab_timing.check_lock_limit(plan)


class Blocks(unittest.TestCase):
    def test_every_arm_runs_twice_at_the_same_mean_position(self):
        rng = random.Random(3)
        arms = ['baseline', 'candidate', 'control']
        for _ in range(20):
            order = ab_timing.block_order(rng, arms)
            self.assertEqual(order, order[::-1])
            for arm in arms:
                positions = [i for i, a in enumerate(order) if a == arm]
                self.assertEqual(len(positions), 2)
                self.assertEqual(sum(positions), len(order) - 1)

    def test_linux_children_disable_aslr_and_pin(self):
        plan = {'platform': 'linux', 'cpu': 8,
                'counter': ['perf', 'stat', '-o', ab_timing.COUNTERS, '--']}
        argv = ab_timing.child_argv(plan, ['@BIN@', '@BIN@.lua'], '/slot', '/c.txt')
        self.assertEqual(argv[:4], ['taskset', '-c', '8', 'setarch'])
        self.assertEqual(argv[5:], ['-R', 'perf', 'stat', '-o', '/c.txt', '--', '/slot', '/slot.lua'])
        plan = {'platform': 'macos', 'cpu': None, 'counter': []}
        self.assertEqual(ab_timing.child_argv(plan, ['@BIN@'], '/copy', '/c.txt'), ['/copy'])


class Slot(unittest.TestCase):
    def test_install_keeps_path_inode_and_padding(self):
        with tempfile.TemporaryDirectory() as temporary:
            first = elf([(1, 0, 0x1100)], 0x1200)
            second = elf([(1, 0, 0x1800)], 0x2100)
            slot = ab_timing.FixedSlot(Path(temporary) / 'slot', [first, second], lock=False)
            try:
                self.assertEqual(slot.size, 0x3000)
                self.assertEqual(slot.extent, 0x2000)
                inode = slot.path.stat().st_ino
                slot.install(second)
                self.assertEqual(slot.path.stat().st_ino, inode)
                data = slot.path.read_bytes()
                self.assertEqual(data[:len(second)], second)
                self.assertEqual(data[len(second):], bytes(0x3000 - len(second)))
                slot.install(first)
                self.assertEqual(slot.path.read_bytes()[:len(first)], first)
            finally:
                slot.close()


class Loop(unittest.TestCase):
    """Runs the whole loop with small programs in place of benchmark images."""

    def run_plan(self, temporary, slots, stdout=b'same\n'):
        root = Path(temporary)
        if slots == 'fixed-inode':
            # A slot reads ELF program headers, so use a real executable; the
            # candidate differs by a trailing byte outside every segment.
            echo = Path('/bin/echo').read_bytes()
            sources = {'baseline': echo, 'candidate': echo + b'\0'}
            argv = ['@BIN@', 'same']
        else:
            sources = {'baseline': script('echo same'), 'candidate': script('echo same  # other')}
            argv = ['@BIN@']
        images = {}
        for arm, data in sources.items():
            images[arm] = root / arm
            images[arm].write_bytes(data)
            images[arm].chmod(0o755)
        images['control'] = images['baseline']
        expected = {'status': 0, 'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
                    'stderr_sha256': hashlib.sha256(b'').hexdigest()}
        plan = {'schema': 1, 'platform': sys.platform, 'slots': slots, 'lock': False, 'cpu': None,
                'controller_cpu': None, 'counter': [], 'rounds': 2, 'warmup': 1, 'seed': 5,
                'work': str(root / 'work'), 'arms': ['baseline', 'candidate', 'control'],
                'apps': [{'suite': 'app',
                          'placements': [{a: str(p) for a, p in images.items()}] * 2,
                          'benches': [{'name': 'b', 'argv': argv, 'cwd': str(root),
                                       'stdin': None, 'expected': expected}]}]}
        (root / 'plan.json').write_text(json.dumps(plan))
        return subprocess.run([sys.executable, ab_timing.__file__, str(root / 'plan.json'),
                               str(root / 'runs.json')], capture_output=True, text=True, timeout=60)

    def check_runs(self, slots):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_plan(temporary, slots)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((Path(temporary) / 'runs.json').read_text())
            runs = report['runs']
            self.assertEqual(len(runs), 2 * 2 * 6)
            for arm in ['baseline', 'candidate', 'control']:
                self.assertEqual(len([r for r in runs if r['arm'] == arm]), 8)
            self.assertEqual({r['round'] for r in runs}, {0, 1})
            return report

    def test_copies_run_every_arm_twice_per_block(self):
        self.check_runs('copies')

    @unittest.skipUnless(sys.platform == 'linux', 'rewriting an executable in place is Linux-only')
    def test_fixed_slots_keep_one_inode_per_placement(self):
        report = self.check_runs('fixed-inode')
        self.assertEqual(len({s['inode'] for s in report['slots']}), 2)

    def test_changed_output_stops_the_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_plan(temporary, 'copies', stdout=b'other\n')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('printed differently', result.stderr)
            self.assertFalse((Path(temporary) / 'runs.json').exists())


if __name__ == '__main__':
    unittest.main()

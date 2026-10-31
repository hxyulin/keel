#!/usr/bin/env python3
"""Paired native sequence measurements, using Keel's actual x86 encodings."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import selectors
import statistics
import struct
import subprocess

HERE = Path(__file__).resolve().parent
MASK = (1 << 64) - 1
CONSTANT = 0xffffffff00000000
REGS = [('rsi', 'esi', 'sil'), ('rcx', 'ecx', 'cl'),
        *[(f'r{i}', f'r{i}d', f'r{i}b') for i in range(8, 14)]]
CASES = ('compare_width', 'load_extension', 'lea_arithmetic', 'constant_materialization')
TARGET = 'x86_64-unknown-linux-gnu'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def operation(case, candidate, reg):
    r, d, b = reg
    if case == 'compare_width':
        return [f'movzx {d}, {b}', f'cmp {d if candidate else b}, 3', f'sete {b}']
    if case == 'load_extension':
        return [f'movzx {d}, byte ptr [rdx + {r}]'] + ([] if candidate else [f'movzx {d}, {b}'])
    if case == 'lea_arithmetic':
        return ([f'imul {r}, {r}, 3', f'add {r}, 7'] if candidate
                else [f'lea {r}, [{r} + {r} * 2 + 7]'])
    if case == 'constant_materialization':
        return ([f'mov {d}, 4294967295', f'not {r}'] if candidate
                else [f'movabs {r}, 0xffffffff00000000'])
    raise ValueError(case)


def source(case, candidate, streams, unroll):
    """One PIC SysV function; no stack or memory traffic in the arithmetic loop."""
    if case not in CASES or streams not in (1, 8) or not 1 <= unroll <= 64:
        raise ValueError('invalid case, stream count or unroll')
    lines = ['.intel_syntax noprefix', '.text', 'push r12', 'push r13', 'mov rax, rsi']
    for i, (r, _, _) in enumerate(REGS[:streams]):
        lines += [f'lea {r}, [rax + {17 * i}]']
        if case == 'load_extension':
            lines += [f'and {r}, 255']
    # Also validate the zero-trip boundary. The driver times only positive counts.
    lines += ['test rdi, rdi', 'jz sequence_done', '.p2align 6', 'sequence_loop:']
    body_line = len(lines) + 1
    for _ in range(unroll):
        for reg in REGS[:streams]:
            lines.extend(operation(case, candidate, reg))
    body_end_line = len(lines) + 1
    lines += ['dec rdi', 'jnz sequence_loop', 'sequence_done:', 'xor eax, eax']
    for r, _, _ in REGS[:streams]:
        lines += ['rol rax, 7', f'xor rax, {r}']
    lines += ['pop r13', 'pop r12', 'ret']
    return '\n'.join(lines) + '\n', body_line, body_end_line


def affine(value, count):
    """Independent modular oracle for repeatedly applying 3*x+7."""
    a, b = 3, 7
    while count:
        if count & 1:
            value = (a * value + b) & MASK
        a, b = a * a & MASK, b * (a + 1) & MASK
        count >>= 1
    return value


def oracle(case, streams, unroll, iterations, seed):
    count = unroll * iterations
    result = 0
    for i in range(streams):
        value = (seed + 17 * i) & MASK
        if case == 'load_extension':
            value &= 255
            # This permutation has a period dividing 256. Do not assume a
            # cycle length: discover the orbit independently of the asm loop.
            orbit = [value]
            nxt = (value * 73 + 19) & 255
            while nxt != value:
                orbit.append(nxt)
                nxt = (nxt * 73 + 19) & 255
            value = orbit[count % len(orbit)]
        elif count and case == 'compare_width':
            value = int((value & 255) == 3) if count == 1 else 0
        elif case == 'lea_arithmetic':
            value = affine(value, count)
        elif count and case == 'constant_materialization':
            value = CONSTANT
        result = (((result << 7) | (result >> 57)) & MASK) ^ value
    return result


def text_section(data):
    """Read one relocation-free ELF64 .text; reject unsupported object formats."""
    if len(data) < 64 or data[:6] != b'\x7fELF\x02\x01':
        raise ValueError('expected little-endian ELF64')
    if struct.unpack_from('<HH', data, 16) != (1, 62):
        raise ValueError('expected an x86-64 relocatable object')
    offset = struct.unpack_from('<Q', data, 40)[0]
    size, count, names = struct.unpack_from('<HHH', data, 58)
    if size != 64 or not 0 < names < count or offset + size * count > len(data):
        raise ValueError('invalid ELF section table')
    headers = [struct.unpack_from('<IIQQQQIIQQ', data, offset + i * size) for i in range(count)]
    def contents(h):
        start, length = h[4:6]
        if start + length > len(data):
            raise ValueError('truncated ELF section')
        return data[start:start + length]
    strings = contents(headers[names])
    text = [i for i, h in enumerate(headers)
            if strings[h[0]:].split(b'\0', 1)[0] == b'.text']
    if len(text) != 1:
        raise ValueError('expected one .text section')
    index = text[0]
    if any(h[1] in (4, 9) and h[7] == index and h[5] for h in headers):
        raise ValueError('unresolved text relocations')
    if headers[index][1] != 1 or not headers[index][2] & 4:
        raise ValueError('expected executable PROGBITS text')
    return contents(headers[index])


def verify_report(report, code, first, end):
    sections = report['sections']
    text = next(s['index'] for s in sections if s['name'] == '.text')
    body = []
    for region in report['regions']:
        if region['section'] != text:
            continue
        if region['relocations']:
            raise ValueError('unresolved metric relocations')
        if region['kind'] == 'instruction':
            start = region['offset']
            if bytes(region['bytes']) != code[start:start + region['size']]:
                raise ValueError('metrics and ordinary assembler bytes differ')
            if first <= region['line'] < end:
                body.append(region)
    if not body:
        raise ValueError('missing measured loop body')
    return {'loop_offset': body[0]['offset'],
            'body_bytes': sum(r['size'] for r in body), 'body_instructions': len(body)}


def interval(pairs, seed=1):
    """Paired geometric mean change and percentile bootstrap over whole rounds."""
    logs = [math.log(b / a) for a, b in pairs]
    if len(logs) < 2:
        raise ValueError('need at least two paired rounds')
    rng = random.Random(seed)
    boot = sorted(statistics.mean(rng.choices(logs, k=len(logs))) for _ in range(4000))
    percent = lambda x: 100 * math.expm1(x)
    return {'change_percent': percent(statistics.mean(logs)),
            'ci95_percent': [percent(boot[100]), percent(boot[3899])]}


class Driver:
    def __init__(self, executable, cpu):
        self.process = subprocess.Popen([str(executable), str(cpu)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)
        self.ready = selectors.DefaultSelector()
        self.ready.register(self.process.stdout, selectors.EVENT_READ)

    def sample(self, blob, placement, iterations, seed, expected):
        if any(c.isspace() for c in str(blob)):
            raise ValueError('blob paths cannot contain whitespace')
        self.process.stdin.write(f'{blob} {placement} {iterations} {seed} {expected}\n')
        self.process.stdin.flush()
        if not self.ready.select(30):
            raise RuntimeError('native sample timed out')
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f'native driver exited: {self.process.poll()}')
        sample = json.loads(line)
        if sample['checksum'] != expected:
            raise ValueError('native checksum mismatch')
        return sample

    def close(self):
        self.ready.close()
        try:
            self.process.stdin.close()
        except BrokenPipeError:
            pass
        try:
            code = self.process.wait(timeout=5)
            if code:
                raise RuntimeError(f'native driver exited with {code}')
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
            raise


def prepare(keel, out, case, streams, unroll):
    variants = []
    for candidate in (False, True):
        name = 'candidate' if candidate else 'baseline'
        base = out / name
        asm, first, end = source(case, candidate, streams, unroll)
        base.with_suffix('.s').write_text(asm)
        for command, suffix in [('asm', '.o'), ('asm-metrics', '.json')]:
            subprocess.run([str(keel), command, '--target', TARGET, str(base.with_suffix('.s')),
                            '-o', str(base.with_suffix(suffix))], check=True)
        code = text_section(base.with_suffix('.o').read_bytes())
        layout = verify_report(json.loads(base.with_suffix('.json').read_text()), code, first, end)
        blob = base.with_suffix('.bin')
        blob.write_bytes(code)
        variants.append({'name': name, 'blob': str(blob), 'sha256': digest(code),
                         'source_sha256': digest(asm.encode()), 'bytes': len(code), **layout})
    control = out / 'control.bin'
    control.write_bytes(Path(variants[0]['blob']).read_bytes())
    variants.append({**variants[0], 'name': 'control', 'blob': str(control)})
    return variants


def measure(driver, variants, case, streams, args, rng):
    def sample(variant, placement, iterations, seed):
        expected = oracle(case, streams, args.unroll, iterations, seed)
        reply = driver.sample(variant['blob'], placement, iterations, seed, expected)
        if reply['cpu'] != args.cpu:
            raise ValueError('measurement migrated away from selected CPU')
        return reply
    # Boundary values, nontrivial iteration counts and high-bit seeds, before timing.
    for variant in variants:
        for count in (0, 1, 2, 7):
            for seed in (0, 3, 255, 256, MASK):
                sample(variant, 0, count, seed)
    iterations = 1024
    while sample(variants[0], 0, iterations, 73)['ns'] < args.sample_ms * 1e6:
        iterations *= 2
        if iterations > 1 << 34:
            raise RuntimeError('calibration failed')
    # Fixed trip count across every variant and placement. Calibration is untimed data.
    rows, summaries = [], []
    placements = list(range(3))
    rng.shuffle(placements)
    for placement in placements:
        for round_index in range(-2, args.rounds):
            order = list(range(3))
            rng.shuffle(order)
            seed = rng.getrandbits(64)
            for index in order:
                reply = sample(variants[index], placement, iterations, seed)
                if round_index >= 0:
                    rows.append({'placement': placement, 'round': round_index,
                                 'variant': variants[index]['name'], 'seed': seed, **reply})
        measured = [r for r in rows if r['placement'] == placement]
        by_round = {i: {r['variant']: r['ns'] for r in measured if r['round'] == i}
                    for i in range(args.rounds)}
        summary = {'placement': placement, 'entry_mod64': placement * 16,
                   'loop_mod64': (placement * 16 + variants[0]['loop_offset']) % 64}
        for name in ('candidate', 'control'):
            summary[name] = interval([(r['baseline'], r[name]) for r in by_round.values()])
        summary['ns_per_sequence'] = {
            name: statistics.median(r['ns'] for r in measured if r['variant'] == name)
                  / (iterations * args.unroll * streams)
            for name in ('baseline', 'candidate', 'control')}
        summaries.append(summary)
    return {'case': case, 'streams': streams,
            'mode': 'throughput' if case == 'constant_materialization' else ('chain' if streams == 1 else 'parallel'),
            'iterations': iterations, 'unroll': args.unroll,
            'sequences_per_sample': iterations * args.unroll * streams,
            'variants': variants, 'samples': rows, 'summaries': summaries}


def environment(cpu):
    paths = [f'/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list',
             f'/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor',
             f'/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq',
             '/sys/devices/system/cpu/cpufreq/boost']
    return {'sysfs': {p: Path(p).read_text().strip() for p in paths if Path(p).exists()},
            'activity': subprocess.check_output(['ps', '-eo', 'comm,pcpu', '--sort=-pcpu'],
                                                text=True).splitlines()[:20]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path, required=True)
    parser.add_argument('--cpu', type=int, required=True)
    parser.add_argument('--cc', default='cc')
    parser.add_argument('--case', choices=CASES, action='append')
    parser.add_argument('--rounds', type=int, default=20)
    parser.add_argument('--sample-ms', type=float, default=10)
    parser.add_argument('--unroll', type=int, default=16)
    parser.add_argument('--seed', type=int, default=20261031)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('native execution requires Linux x86-64')
    if args.rounds < 2 or not 0 < args.sample_ms <= 500 or not 1 <= args.unroll <= 64:
        parser.error('need rounds >= 2, sample-ms in (0,500], unroll in [1,64]')
    if args.cpu not in os.sched_getaffinity(0):
        parser.error('CPU is outside the allowed affinity mask')
    args.keel = args.keel.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    executable = args.output / 'driver'
    build = [args.cc, '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', str(HERE / 'driver.c'), '-o', str(executable)]
    subprocess.run(build, check=True)
    result = {'schema_version': 1, 'complete': False, 'machine': platform.uname()._asdict(),
              'cpuinfo': Path('/proc/cpuinfo').read_text(), 'cpu': args.cpu,
              'clock': 'CLOCK_MONOTONIC_RAW nanoseconds; no hardware cycle estimate',
              'boundary': 'one native function call, including loop and checksum; warmup excluded',
              'backing': 'three retained mlocked anonymous arenas; physical frame migration not checked',
              'driver_build': build,
              'driver_compiler': subprocess.check_output([args.cc, '--version'], text=True),
              'driver_sha256': digest(executable.read_bytes()),
              'harness_sha256': {p.name: digest(p.read_bytes()) for p in (HERE / 'driver.c', Path(__file__))},
              'environment_before': environment(args.cpu),
              'assembler_sha256': digest(args.keel.read_bytes()), 'options': vars(args).copy(), 'results': []}
    result['options'] = {k: str(v) if isinstance(v, Path) else v for k, v in result['options'].items()}
    rng = random.Random(args.seed)
    driver = Driver(executable, args.cpu)
    try:
        for case in args.case or CASES:
            for streams in (1, 8):
                out = args.output / f'{case}-{streams}'
                out.mkdir()
                variants = prepare(args.keel, out, case, streams, args.unroll)
                # An unrolled comparison quickly reaches a fixed point. Check
                # one-operation functions too, so a wrong predicate cannot hide.
                validation = out / 'validation-one-step'
                validation.mkdir()
                checks = prepare(args.keel, validation, case, streams, 1)
                for variant in checks:
                    for count in (0, 1, 2, 7):
                        for seed in (0, 3, 255, 256, MASK,
                                     *((3 - 17 * i) & MASK for i in range(1, streams))):
                            driver.sample(variant['blob'], 0, count, seed,
                                          oracle(case, streams, 1, count, seed))
                record = measure(driver, variants, case, streams, args, rng)
                record['one_step_validation'] = checks
                result['results'].append(record)
                (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
                print(f'{case}, {streams} stream(s): complete', flush=True)
                for summary in sorted(record['summaries'], key=lambda row: row['placement']):
                    change = summary['candidate']
                    low, high = change['ci95_percent']
                    print(f"  placement {summary['placement']}: {change['change_percent']:+.3f}% "
                          f"[{low:+.3f}, {high:+.3f}], control "
                          f"{summary['control']['change_percent']:+.3f}%", flush=True)
    finally:
        driver.close()
    if digest(args.keel.read_bytes()) != result['assembler_sha256']:
        raise RuntimeError('assembler binary changed during the run')
    result['complete'] = True
    result['environment_after'] = environment(args.cpu)
    (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

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
MASK_WIDTHS = {'mask_byte': 8, 'mask_word': 16}
SIGN_WIDTHS = {'sign_byte': 8, 'sign_word': 16, 'sign_dword': 32}
MULTIPLIERS = {'multiply_5': 5, 'multiply_10': 10}
MULTIPLIERS.update({f'multiply_{n}': n for n in (3, 7, 9, 12, 15, 17, 63)})
ZERO_SHIFT_WIDTHS = {'zero_shift_byte': 8, 'zero_shift_word': 16, 'zero_shift_dword': 32}
WIDE_LOADS = {'fold_load_word': (16, False), 'fold_signed_load_word': (16, True),
              'fold_signed_load_dword': (32, True), 'fold_zero_then_signed_byte': (8, True),
              'remove_load_dword_extension': (32, False)}
BOOLEAN_INPUTS = ('zero_branch', 'invert_boolean')
ZERO_PREDICATES = ('zero_setcc', 'zero_setcc32', 'setcc_clean_test', 'setcc_clean_cmp')
EXTRA_CASES = ('zero_setcc32', 'setcc_clean_test', 'setcc_clean_cmp', 'bool_nonzero',
               'mask_nonzero', 'mask_zero', 'invert_boolean', 'rotate_left', 'rotate_right',
               'bitwise_not', 'negate', 'add_lea', 'double', 'increment', 'loop_dec_sub',
               'absolute_select')
LOAD_CASES = ('load_extension', 'fold_load_byte', 'fold_signed_load_byte')
CASES = ('compare_width', 'load_extension', 'lea_arithmetic', 'constant_materialization',
         *MASK_WIDTHS, *SIGN_WIDTHS, 'fold_load_byte', 'fold_signed_load_byte',
         'zero_setcc', 'zero_branch', *MULTIPLIERS, *ZERO_SHIFT_WIDTHS, *WIDE_LOADS,
         *EXTRA_CASES)
TARGET = 'x86_64-unknown-linux-gnu'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def subregister(reg, bits):
    r, d, b = reg
    if bits == 8:
        return b
    if bits == 16:
        return {'rsi': 'si', 'rcx': 'cx'}.get(r, r + 'w')
    if bits == 32:
        return d
    raise ValueError(bits)


def operation(case, candidate, reg, label='single'):
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
    if case in MASK_WIDTHS:
        bits = MASK_WIDTHS[case]
        # The add keeps the mask/extension in a changing dependency chain.
        return [f'add {r}, 17'] + ([f'movzx {d}, {subregister(reg, bits)}'] if candidate
                                 else [f'and {r}, {(1 << bits) - 1}'])
    if case in SIGN_WIDTHS:
        bits = SIGN_WIDTHS[case]
        extension = 'movsxd' if bits == 32 else 'movsx'
        return [f'add {r}, 17'] + (
            [f'{extension} {r}, {subregister(reg, bits)}'] if candidate
            else [f'shl {r}, {64 - bits}', f'sar {r}, {64 - bits}'])
    if case == 'fold_load_byte':
        return ([f'movzx {d}, byte ptr [rdx + {r}]'] if candidate
                else [f'mov {b}, byte ptr [rdx + {r}]', f'movzx {d}, {b}'])
    if case == 'fold_signed_load_byte':
        # Signed values are masked before supplying the next table index.
        return [f'and {r}, 255'] + (
            [f'movsx {r}, byte ptr [rdx + {r}]'] if candidate
            else [f'mov {b}, byte ptr [rdx + {r}]', f'movsx {r}, {b}'])
    if case in ('zero_setcc', 'zero_branch'):
        compare = f'test {r}, {r}' if candidate else f'cmp {r}, 0'
        if case == 'zero_setcc':
            return [compare, f'sete {b}', f'movzx {d}, {b}']
        # On the Boolean input, 0 -> 1 and 1 -> 0. Each unrolled site
        # has its own branch; this is not an unpredictable-branch workload.
        return [compare, f'jnz .Lnonzero_{label}', f'add {r}, 2',
                f'.Lnonzero_{label}:', f'sub {r}, 1']
    if case in MULTIPLIERS:
        factor = MULTIPLIERS[case]
        if not candidate:
            return [f'imul {r}, {r}, {factor}']
        if factor in (3, 5, 9):
            return [f'lea {r}, [{r} + {r} * {factor - 1}]']
        if factor in (10, 12):
            return [f'lea {r}, [{r} + {r} * {4 if factor == 10 else 2}]',
                    f'add {r}, {r}' if factor == 10 else f'shl {r}, 2']
        shift = {7: 3, 15: 4, 17: 4, 63: 6}[factor]
        return [f'mov rax, {r}', f'shl {r}, {shift}',
                f'{"add" if factor == 17 else "sub"} {r}, rax']
    if case in ZERO_SHIFT_WIDTHS:
        bits = ZERO_SHIFT_WIDTHS[case]
        extend = f'mov {d}, {d}' if bits == 32 else f'movzx {d}, {subregister(reg, bits)}'
        return [f'add {r}, 17'] + ([extend] if candidate else
                                                  [f'shl {r}, {64 - bits}', f'shr {r}, {64 - bits}'])
    if case in WIDE_LOADS:
        bits, signed = WIDE_LOADS[case]
        operand = f'{ {8: "byte", 16: "word", 32: "dword"}[bits]} ptr [rdx + {r} * 4]'
        load = 'mov' if bits == 32 else 'movzx'
        extension = 'movsxd' if bits == 32 else 'movsx'
        if candidate:
            ops = [f'{extension} {r}, {operand}'] if signed else [f'{load} {d}, {operand}']
        else:
            # The unsigned word case measures a partial-register load; the
            # signed cases match the compiler's zero-load-then-sign-extend form.
            ops = ([f'mov {subregister(reg, bits)}, {operand}', f'movzx {d}, {subregister(reg, bits)}']
                   if not signed and bits == 16 else [f'{load} {d}, {operand}'])
            if signed:
                ops += [f'{extension} {r}, {subregister(reg, bits)}']
            elif bits == 32:
                ops += [f'mov {d}, {d}']
        return [f'and {r}, 255', *ops]
    if case == 'zero_setcc32':
        return [f'test {d}, {d}' if candidate else f'cmp {d}, 0', f'sete {b}', f'movzx {d}, {b}']
    if case in ('setcc_clean_test', 'setcc_clean_cmp'):
        compare = f'test {r}, {r}' if case.endswith('test') else f'cmp {r}, 0'
        return (['xor eax, eax', compare, 'sete al', f'mov {r}, rax'] if candidate
                else [compare, f'sete {b}', f'movzx {d}, {b}'])
    if case in ('bool_nonzero', 'mask_nonzero'):
        if candidate:
            return [f'neg {r}', f'sbb {r}, {r}'] + ([f'neg {r}'] if case == 'bool_nonzero' else [])
        return [f'test {r}, {r}', f'setne {b}', f'movzx {d}, {b}'] + (
            [f'neg {r}'] if case == 'mask_nonzero' else [])
    if case == 'mask_zero':
        return ([f'cmp {r}, 1', f'sbb {r}, {r}'] if candidate else
                [f'cmp {r}, 0', f'sete {b}', f'movzx {d}, {b}', f'neg {r}'])
    if case == 'invert_boolean':
        return [f'xor {r}, 1'] if candidate else [f'cmp {r}, 0', f'sete {b}', f'movzx {d}, {b}']
    if case in ('rotate_left', 'rotate_right'):
        left = case == 'rotate_left'
        return ([f'{"rol" if left else "ror"} {r}, 13'] if candidate else
                [f'mov rax, {r}', f'{"shl" if left else "shr"} {r}, 13',
                 f'{"shr" if left else "shl"} rax, 51', f'or {r}, rax'])
    if case == 'bitwise_not':
        return [f'not {r}'] if candidate else [f'xor {r}, -1']
    if case == 'negate':
        return [f'neg {r}'] if candidate else [f'not {r}', f'add {r}, 1']
    if case == 'add_lea':
        return [f'add {r}, 17'] if candidate else [f'lea {r}, [{r} + 17]']
    if case == 'double':
        return [f'add {r}, {r}'] if candidate else [f'lea {r}, [{r} + {r}]']
    if case == 'increment':
        return [f'inc {r}'] if candidate else [f'add {r}, 1']
    if case == 'loop_dec_sub':
        return [f'add {r}, 17']
    if case == 'absolute_select':
        return [f'add {r}, 17'] + (
            [f'mov rax, {r}', 'neg rax', f'test {r}, {r}', f'cmovs {r}, rax'] if candidate else
            [f'test {r}, {r}', f'jns .Lpositive_{label}', f'neg {r}', f'.Lpositive_{label}:'])
    raise ValueError(case)


def source(case, candidate, streams, unroll):
    """One PIC SysV function; no stack or memory traffic in the arithmetic loop."""
    if case not in CASES or streams not in (1, 2, 4, 8) or not 1 <= unroll <= 64:
        raise ValueError('invalid case, stream count or unroll')
    lines = ['.intel_syntax noprefix', '.text', 'push r12', 'push r13', 'mov rax, rsi']
    for i, (r, _, _) in enumerate(REGS[:streams]):
        lines += [f'lea {r}, [rax + {17 * i}]']
        if case in LOAD_CASES or case in WIDE_LOADS:
            lines += [f'and {r}, 255']
        elif case in BOOLEAN_INPUTS:
            lines += [f'and {r}, 1']
        elif case == 'zero_setcc32':
            lines += [f'mov {REGS[i][1]}, {REGS[i][1]}']
    # Also validate the zero-trip boundary. The driver times only positive counts.
    lines += ['test rdi, rdi', 'jz sequence_done', '.p2align 6', 'sequence_loop:']
    body_line = len(lines) + 1
    for step in range(unroll):
        for index, reg in enumerate(REGS[:streams]):
            lines.extend(operation(case, candidate, reg, f'{step}_{index}'))
    body_end_line = len(lines) + 1
    lines += ['sub rdi, 1' if case == 'loop_dec_sub' and candidate else 'dec rdi',
              'jnz sequence_loop', 'sequence_done:', 'xor eax, eax']
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


def byte_index(value, count):
    value &= 255
    orbit = [value]
    nxt = (value * 73 + 19) & 255
    while nxt != value:
        orbit.append(nxt)
        nxt = (nxt * 73 + 19) & 255
    return orbit[count % len(orbit)]


def wide_value(index):
    return (((index * 73 + 19) & 255) | (((index * 11 + 3) & 255) << 8)
            | (((index * 13 + 7) & 255) << 24))


def absolute_recurrence(value, count):
    if not count:
        return value
    value = (value + 17) & MASK
    value = min(value, (1 << 64) - value)
    count -= 1
    boundary = 1 << 63
    # Once positive additions cross INT64_MIN, absolute value reflects
    # around that boundary. Subsequent steps alternate between two values.
    until_crossing = (boundary - value) // 17 + 1
    if count < until_crossing:
        return value + 17 * count
    value = 2 * boundary - value - 17 * until_crossing
    if (count - until_crossing) & 1:
        value = 2 * boundary - value - 17
    return value


def oracle(case, streams, unroll, iterations, seed):
    if case not in CASES:
        raise ValueError(case)
    count = unroll * iterations
    result = 0
    for i in range(streams):
        value = (seed + 17 * i) & MASK
        if case == 'zero_setcc32':
            value &= (1 << 32) - 1
        if case in LOAD_CASES:
            # This permutation has a period dividing 256. Do not assume a
            # cycle length: discover the orbit independently of the asm loop.
            value = byte_index(value, count)
            if case == 'fold_signed_load_byte' and count and value >= 128:
                value = (value - 256) & MASK
        elif case in WIDE_LOADS:
            value &= 255
            if count:
                bits, signed = WIDE_LOADS[case]
                value = wide_value(byte_index(value, count - 1)) & ((1 << bits) - 1)
                if signed and value & (1 << (bits - 1)):
                    value = (value - (1 << bits)) & MASK
        elif count and case == 'compare_width':
            value = int((value & 255) == 3) if count == 1 else 0
        elif case == 'lea_arithmetic':
            value = affine(value, count)
        elif count and case == 'constant_materialization':
            value = CONSTANT
        elif count and case in (MASK_WIDTHS | ZERO_SHIFT_WIDTHS):
            bits = (MASK_WIDTHS | ZERO_SHIFT_WIDTHS)[case]
            value = (value + 17 * count) & ((1 << bits) - 1)
        elif count and case in SIGN_WIDTHS:
            bits = SIGN_WIDTHS[case]
            value = (value + 17 * count) & ((1 << bits) - 1)
            if value & (1 << (bits - 1)):
                value = (value - (1 << bits)) & MASK
        elif count and case in ZERO_PREDICATES:
            value = int(value == 0) if count & 1 else int(value != 0)
        elif case in BOOLEAN_INPUTS:
            value = (value & 1) ^ (count & 1)
        elif case in MULTIPLIERS:
            value = value * pow(MULTIPLIERS[case], count, 1 << 64) & MASK
        elif count and case == 'bool_nonzero':
            value = int(value != 0)
        elif count and case == 'mask_nonzero':
            value = MASK if value else 0
        elif count and case == 'mask_zero':
            value = MASK if (value == 0 if count & 1 else value != 0) else 0
        elif case in ('rotate_left', 'rotate_right'):
            shift = (count * 13 * (1 if case == 'rotate_left' else -1)) % 64
            value = ((value << shift) | (value >> (64 - shift))) & MASK
        elif count & 1 and case == 'bitwise_not':
            value ^= MASK
        elif count & 1 and case == 'negate':
            value = -value & MASK
        elif case in ('add_lea', 'loop_dec_sub', 'increment'):
            value = (value + count * (1 if case == 'increment' else 17)) & MASK
        elif case == 'double':
            value = value * pow(2, count, 1 << 64) & MASK
        elif case == 'absolute_select':
            value = absolute_recurrence(value, count)
        result = (((result << 7) | (result >> 57)) & MASK) ^ value
    return result


def validation_seeds(case, streams):
    seeds = [0, 1, 3, 127, 128, 255, 256, MASK]
    widths = SIGN_WIDTHS | MASK_WIDTHS | ZERO_SHIFT_WIDTHS
    if case in widths:
        bits = widths[case]
        # Cover the signed boundary and truncation wrap after the common add.
        seeds += [(value - 17) & MASK for value in
                  ((1 << (bits - 1)) - 1, 1 << (bits - 1), (1 << bits) - 1, 1 << bits)]
    if case == 'absolute_select':
        seeds += [(1 << 63) - 18, (1 << 63) - 17, (1 << 63) - 1, 1 << 63]
    # Make each comparison stream take its equality path at least once.
    seeds += [(3 - 17 * i) & MASK for i in range(1, streams)]
    return tuple(dict.fromkeys(seeds))


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
    def __init__(self, executable, cpu, timer='wall'):
        self.cpu, self.timer, self.entries = cpu, timer, {}
        self.process = subprocess.Popen([str(executable), str(cpu), timer], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)
        self.ready = selectors.DefaultSelector()
        self.ready.register(self.process.stdout, selectors.EVENT_READ)
        try:
            self.metadata = self.read()
        except Exception:
            self.close()
            raise

    def read(self):
        if not self.ready.select(30):
            raise RuntimeError('native sample timed out')
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f'native driver exited: {self.process.poll()}')
        return json.loads(line)

    def sample(self, blob, placement, iterations, seed, expected, input_kind=0):
        if any(c.isspace() for c in str(blob)):
            raise ValueError('blob paths cannot contain whitespace')
        self.process.stdin.write(f'{blob} {placement} {iterations} {seed} {expected} {input_kind}\n')
        self.process.stdin.flush()
        sample = self.read()
        validate_sample(sample, expected, self.cpu, self.timer)
        if (sample['entry'] % 64 != placement * 16
                or self.entries.setdefault(placement, sample['entry']) != sample['entry']):
            raise ValueError('code placement changed')
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


def validate_sample(sample, expected, cpu, timer):
    if sample['checksum'] != expected:
        raise ValueError('native checksum mismatch')
    if sample['cpu'] != cpu or not sample['locked'] or sample['ns'] <= 0:
        raise ValueError('invalid CPU, locking or wall-time sample')
    if timer == 'dual' and (sample['tsc_ticks'] <= 0
                           or sample['tsc_aux_start'] != sample['tsc_aux_end']):
        raise ValueError('invalid TSC duration or endpoint CPU markers')


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
                         'source_sha256': digest(asm.encode()), 'bytes': len(code),
                         'input_kind': int(case in WIDE_LOADS), **layout})
    control = out / 'control.bin'
    control.write_bytes(Path(variants[0]['blob']).read_bytes())
    variants.append({**variants[0], 'name': 'control', 'blob': str(control)})
    return variants


def measure(driver, variants, case, streams, args, rng):
    def sample(variant, placement, iterations, seed):
        expected = oracle(case, streams, args.unroll, iterations, seed)
        return driver.sample(variant['blob'], placement, iterations, seed, expected, variant['input_kind'])
    # Boundary values, nontrivial iteration counts and high-bit seeds, before timing.
    for variant in variants:
        for count in (0, 1, 2, 7):
            for seed in validation_seeds(case, streams):
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
        if args.timer == 'dual':
            tsc_rounds = {i: {r['variant']: r['tsc_ticks'] for r in measured if r['round'] == i}
                          for i in range(args.rounds)}
            for name in ('candidate', 'control'):
                summary[name + '_tsc'] = interval([(r['baseline'], r[name]) for r in tsc_rounds.values()])
            summary['tsc_ticks_per_sequence'] = {
                name: statistics.median(r['tsc_ticks'] for r in measured if r['variant'] == name)
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
             f'/sys/devices/system/cpu/cpu{cpu}/topology/core_id',
             f'/sys/devices/system/cpu/cpu{cpu}/topology/core_type',
             f'/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq',
             '/sys/devices/system/cpu/cpufreq/boost',
             '/sys/devices/system/cpu/intel_pstate/no_turbo',
             '/sys/bus/event_source/devices/cpu_core/cpus',
             '/sys/bus/event_source/devices/cpu_atom/cpus',
             '/sys/devices/system/clocksource/clocksource0/current_clocksource']
    return {'sysfs': {p: Path(p).read_text().strip() for p in paths if Path(p).exists()},
            'activity': subprocess.check_output(['ps', '-eo', 'comm,pcpu', '--sort=-pcpu'],
                                                text=True).splitlines()[:20]}


def listing():
    """Each case's one-stream loop iteration: baseline, then candidate.

    Loop control appears only for a case whose variants differ there."""
    lines = []
    for case in CASES:
        variants = []
        for candidate in (False, True):
            asm, first, end = source(case, candidate, 1, 1)
            body = asm.splitlines()
            variants.append((body[first - 1:end - 1], body[end - 1]))
        same_control = variants[0][1] == variants[1][1]
        lines.append(case)
        for label, (ops, control) in zip(('baseline:', 'candidate:'), variants):
            lines.append(f'  {label:<11}' + '; '.join(ops if same_control else [*ops, control]))
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true',
                        help="print each case's baseline and candidate instructions, then exit")
    parser.add_argument('--keel', type=Path)
    parser.add_argument('--cpu', type=int)
    parser.add_argument('--controller-cpu', type=int,
                        help='pin the Python controller to a different allowed CPU')
    parser.add_argument('--cc', default='cc')
    parser.add_argument('--timer', choices=('wall', 'dual'), default='wall',
                        help='dual adds ordered RDTSCP ticks and endpoint CPU markers')
    parser.add_argument('--case', choices=CASES, action='append')
    parser.add_argument('--streams', type=int, choices=(1, 2, 4, 8), action='append',
                        help='repeat to select stream counts; defaults to 1 and 8')
    parser.add_argument('--rounds', type=int, default=20)
    parser.add_argument('--sample-ms', type=float, default=10)
    parser.add_argument('--unroll', type=int, default=16)
    parser.add_argument('--seed', type=int, default=20261031)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.list:
        print(listing(), end='')
        return 0
    missing = [f'--{name}' for name in ('keel', 'cpu', 'output') if getattr(args, name) is None]
    if missing:
        parser.error('the following arguments are required: ' + ', '.join(missing))
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('native execution requires Linux x86-64')
    if args.rounds < 2 or not 0 < args.sample_ms <= 500 or not 1 <= args.unroll <= 64:
        parser.error('need rounds >= 2, sample-ms in (0,500], unroll in [1,64]')
    allowed_cpus = os.sched_getaffinity(0)
    if args.cpu not in allowed_cpus:
        parser.error('CPU is outside the allowed affinity mask')
    if args.controller_cpu is not None:
        if args.controller_cpu not in allowed_cpus or args.controller_cpu == args.cpu:
            parser.error('controller CPU must be allowed and different from the measurement CPU')
        os.sched_setaffinity(0, {args.controller_cpu})
    args.keel = args.keel.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    executable = args.output / 'driver'
    build = [args.cc, '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', str(HERE / 'driver.c'), '-o', str(executable)]
    subprocess.run(build, check=True)
    result = {'schema_version': 2, 'complete': False, 'machine': platform.uname()._asdict(),
              'cpuinfo': Path('/proc/cpuinfo').read_text(), 'cpu': args.cpu,
              'controller_affinity': sorted(os.sched_getaffinity(0)),
              'clock': 'CLOCK_MONOTONIC_RAW nanoseconds; optional invariant TSC ticks, not core cycles',
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
    driver = Driver(executable, args.cpu, args.timer)
    result['timer'] = driver.metadata
    try:
        for case in args.case or CASES:
            for streams in dict.fromkeys(args.streams or (1, 8)):
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
                        for seed in validation_seeds(case, streams):
                            driver.sample(variant['blob'], 0, count, seed,
                                          oracle(case, streams, 1, count, seed), variant['input_kind'])
                record = measure(driver, variants, case, streams, args, rng)
                record['one_step_validation'] = checks
                record['validation_seeds'] = validation_seeds(case, streams)
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

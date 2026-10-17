"""Source-only proofs. No C compiler, native execution, or performance claims.

The deliberately bounded C evaluator reads the actual independent oracle source;
unknown syntax fails. It is not a C implementation or assembled-code validator.
"""
import ast
import importlib.util
import json
from pathlib import Path
import random
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
U64 = (1 << 64) - 1
SALT = 0x9e3779b97f4a7c15
SEEDS = [0, 1, 9, 10, 11, 127, 128, 255, 256,
         9223372036854775807, 9223372036854775808, 18446744073709551615]
COUNTS = [0, 1, 2, 17, 257]
FAMILIES = ('madd', 'rotate', 'udiv10', 'branch-select', 'sext8', 'narrow-load')
SPEC = importlib.util.spec_from_file_location('more_manifest', ROOT / 'lab/manifest.py')
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)


def loaded():
    return [MANIFEST.load(p, ROOT) for p in sorted((ROOT / 'patterns').glob('*.json'))]


def reference(family, count, seed, data, mask):
    state = seed
    for _ in range(count):
        if family == 'madd':
            state = 3 * state + 7
        elif family == 'rotate':
            # Independent bit-index definition of rotation, no shift/or recipe.
            state = sum(((state >> ((bit + 13) % 64)) & 1) << bit for bit in range(64)) ^ SALT
        elif family == 'udiv10':
            state = divmod(state, 10)[0] + SALT
        elif family == 'branch-select':
            state += 5 - 2 * (data[(state >> 8) & mask] % 2)
        elif family == 'sext8':
            state = ((state + 128) % 256) - 128 + SALT
        elif family == 'narrow-load':
            state = data[(state >> 8) & mask] % 256
        else:
            raise AssertionError(family)
        state &= U64
    return state


def c_expression(text, env):
    text = re.sub(r'UINT64_C\((0x[0-9a-fA-F]+|[0-9]+)\)', r'\1', text)
    # This exact small oracle grammar has one top-level ternary, optionally
    # parenthesized. Splitting before AST parsing keeps C and Python precedence.
    ternary = re.fullmatch(r'(.*?)\(([^()?]+)\?([^:]+):([^()?]+)\)(.*)', text)
    if ternary:
        prefix, condition, yes, no, suffix = ternary.groups()
        value = c_expression(yes if c_expression(condition, env) else no, env)
        text = prefix + str(value) + suffix
    elif '?' in text:
        condition, choices = text.split('?', 1)
        yes, no = choices.split(':', 1)
        return c_expression(yes if c_expression(condition, env) else no, env)
    text = text.replace('/', '//')
    tree = ast.parse(text.strip(), mode='eval')
    allowed = (ast.Expression, ast.BinOp, ast.Name, ast.Load, ast.Constant,
               ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.LShift, ast.RShift,
               ast.BitOr, ast.BitAnd, ast.BitXor, ast.Compare, ast.Lt,
               ast.Subscript)
    if any(not isinstance(node, allowed) for node in ast.walk(tree)):
        raise AssertionError('unsupported C expression: ' + text)
    return eval(compile(tree, '<bounded oracle expression>', 'eval'), {'__builtins__': {}}, env)


def source_oracle(family, count, seed, data, mask):
    src = (ROOT / f'oracles/more-{family}.c').read_text()
    src = re.sub(r'/\*.*?\*/', '', src, flags=re.S)
    signature = ('uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,\n'
                 '                        const uint64_t *data, uint64_t mask) {\n')
    if signature not in src:
        raise AssertionError('unexpected oracle signature')
    content = src.split(signature, 1)[1].replace('    (void)data;\n', '').replace('    (void)mask;\n', '')
    prefix = '    uint64_t state = seed;\n    while (iterations != 0) {\n'
    suffix = '        iterations--;\n    }\n    return state;\n}\n'
    if not content.startswith(prefix) or not content.endswith(suffix):
        raise AssertionError('unexpected oracle control flow')
    statements = content[len(prefix):-len(suffix)].splitlines()
    assignments = []
    for line in statements:
        match = re.fullmatch(r'        (?:uint64_t )?(\w+) = (.+);', line)
        if not match:
            raise AssertionError('unexpected oracle statement: ' + line)
        assignments.append(match.groups())
    env = dict(state=seed, data=data, mask=mask)
    for _ in range(count):
        for name, expression in assignments:
            env[name] = c_expression(expression, env) & U64
    return env['state']


class MoreMath(unittest.TestCase):
    def test_actual_c_oracles_against_independent_definitions(self):
        rng = random.Random(20261017)
        arrays = [[0] * 256, [U64] * 256,
                  [((i << 40) | (255 - i)) for i in range(256)],
                  [rng.getrandbits(64) for _ in range(256)]]
        for family in FAMILIES:
            for seed in SEEDS:
                # Zero iterations must never access even an invalid data pointer.
                self.assertEqual(source_oracle(family, 0, seed, None, 255), seed)
                for data in arrays:
                    for count in COUNTS[1:]:
                        self.assertEqual(source_oracle(family, count, seed, data, 255),
                                         reference(family, count, seed, data, 255),
                                         (family, seed, count))
        examples = [('madd', U64, 4), ('rotate', 0, SALT),
                    ('udiv10', U64, 13245389226694153646),
                    ('sext8', 128, SALT - 128), ('sext8', 255, SALT - 1)]
        for family, seed, expected in examples:
            self.assertEqual(source_oracle(family, 1, seed, arrays[0], 255), expected)

    def test_divide10_exact_algebra_entire_uint64_domain(self):
        # B=2^67, M=(B+2)/10. n=10q+r, 0<=r<=9.
        # n*M=q*B + (2q+r*M). Thus floor(n*M/B)=q if remainder
        # is in [0,B). q<=floor((2^64-1)/10); independent maxima
        # give 2*q+9*M<B, proving the identity for EVERY uint64 n.
        b = 1 << 67
        m = 0xcccccccccccccccd
        qmax = U64 // 10
        self.assertEqual(10 * m, b + 2)
        self.assertGreaterEqual(2 * 0 + 0 * m, 0)
        self.assertLess(2 * qmax + 9 * m, b)
        rng = random.Random(10)
        quotients = {0, 1, 2, qmax, qmax-1, qmax-2}
        quotients.update((1 << k) // 10 for k in range(65))
        quotients.update(rng.randrange(qmax + 1) for _ in range(1000))
        for q in quotients:
            for r in range(10):
                n = 10 * q + r
                if n <= U64:
                    self.assertEqual(((n * m) >> 64) >> 3, n // 10)
        for n in range(U64 - 1024, U64 + 1):
            self.assertEqual(((n * m) >> 64) >> 3, n // 10)

    def test_sign_extension_all_bytes_and_narrow_load_all_addresses(self):
        for byte in range(256):
            shifted = (byte << 56) & U64
            signed = shifted if shifted < 1 << 63 else shifted - (1 << 64)
            self.assertEqual((signed >> 56) & U64, (byte if byte < 128 else byte - 256) & U64)
        for index in range(256):
            self.assertLess(index * 8 + 7, 256 * 8)
            self.assertLess(index * 8, 256 * 8)
            for word in [0, U64, index, (U64 ^ 255) | index]:
                self.assertEqual(word.to_bytes(8, 'little')[0], word & 255)
        self.assertNotEqual((1 << 56).to_bytes(8, 'big')[0], (1 << 56) & 255)

    def test_strict_runtime_parser_all_available_manifests(self):
        cases = loaded()
        self.assertGreaterEqual(len(cases), 7)
        self.assertEqual(len({m['id'] for m in cases}), len(cases))
        for m in cases:
            self.assertTrue(all(type(n) is int for n in m['seeds']))
            self.assertTrue(all(0 <= n <= U64 for n in m['seeds']))

    def test_complete_nineteen_manifest_contract(self):
        cases = loaded()
        expected = {f'{arch}-more-{family}' for arch in ('arm', 'x86') for family in FAMILIES}
        current = {m['id'] for m in cases if '-more-' in m['id']}
        self.assertTrue(current <= expected, 'unexpected new pattern IDs')
        if current != expected:
            self.skipTest('PENDING committed architecture checkpoints: ' + ', '.join(sorted(expected-current)))
        self.assertEqual(len(cases), 19)
        for m in cases:
            if m['id'] not in expected:
                continue
            family = m['id'].split('-more-', 1)[1]
            self.assertEqual(m['seeds'], SEEDS)
            self.assertEqual(m['correctness_iterations'], COUNTS)
            self.assertEqual([v['id'] for v in m['variants']], ['baseline', 'candidate'])
            self.assertEqual(m['oracle'], {'source': f'oracles/more-{family}.c', 'symbol': 'pattern_oracle'})
            self.assertEqual(m['work_per_iteration'], 1)
            self.assertEqual((m['data_words'], m['mask']), (256, 255) if family in ('branch-select', 'narrow-load') else (1, 0))
            self.assertEqual(m['category'], {'branch-select': 'control', 'narrow-load': 'memory'}.get(family, 'latency'))
            self.assertEqual(m['work_unit'], {'branch-select': 'conditional_update', 'narrow-load': 'low_byte_load'}.get(family, 'state_update'))
            self.assertEqual(set(m.get('mca', {})), set() if family == 'branch-select' else {'baseline', 'candidate'})


class MoreSources(unittest.TestCase):
    def cases(self):
        cases = [m for m in loaded() if '-more-' in m['id']]
        if not cases:
            self.skipTest('PENDING committed architecture sources')
        return cases

    def test_actual_source_guards_leaf_registers_and_mca_correspondence(self):
        for m in self.cases():
            for variant in m['variants']:
                with self.subTest(pattern=m['id'], variant=variant['id']):
                    src = (ROOT / variant['source']).read_text()
                    self.assertIn('#ifdef __APPLE__', src)
                    self.assertIn('#define KERNEL _pattern_kernel', src)
                    self.assertIn('#define KERNEL pattern_kernel', src)
                    self.assertIn('.type KERNEL,', src)
                    self.assertIn('.size KERNEL, .-KERNEL', src)
                    self.assertIn('.section .note.GNU-stack,"",', src)
                    self.assertEqual(src.count('.cfi_startproc'), 1)
                    self.assertEqual(src.count('.cfi_endproc'), 1)
                    self.assertNotRegex(src, r'\b(call|bl|push|pop|rsp|esp|sp|x18|w18)\b')
                    before, after = src.split('L_loop:\n', 1)
                    arm = m['arch'] == 'aarch64'
                    counter = '    subs x0, x0, #1' if arm else '    subq $1, %rdi'
                    instructions, tail = after.split(counter, 1)
                    if arm:
                        self.assertIn('KERNEL:\n.cfi_startproc\n    cbz x0, L_done', before)
                        self.assertIn('\n    b.ne L_loop', tail)
                        self.assertIn('L_done:\n    mov x0, x1\n    ret', tail)
                        regs = set(map(int, re.findall(r'\b[wx](\d+)\b', src)))
                        self.assertTrue(regs <= set(range(18)))
                        self.assertNotRegex(instructions, r'\b(movz|movk|movn)\b')
                    else:
                        self.assertIn('testq %rdi, %rdi\n    jz L_done', before)
                        self.assertIn('\n    jnz L_loop', tail)
                        self.assertIn('L_done:\n    movq %rsi, %rax\n    ret', tail)
                        regs = set(re.findall(r'%([a-z][a-z0-9]*)', src))
                        allowed = {'rax', 'eax', 'ax', 'al', 'ah', 'rcx', 'ecx', 'cx', 'cl',
                                   'rdx', 'edx', 'dx', 'dl', 'rsi', 'esi', 'si', 'sil',
                                   'rdi', 'edi', 'di', 'dil', 'progbits'}
                        allowed.update('r' + str(i) + suffix for i in range(8, 12)
                                       for suffix in ('', 'd', 'w', 'b'))
                        self.assertTrue(regs <= allowed, regs - allowed)
                        self.assertNotIn('movabs', instructions)
                    # No added vector ISA; baseline integer vocabulary is explicit.
                    self.assertNotRegex(src, r'\b([xyz]mm\d+|v[a-z]+|pdep|pext|rorx|mulx|shrx|shlx)\b')
                    if 'branch-select' in m['id']:
                        self.assertFalse(m.get('mca'))
                        continue
                    self.assertEqual(set(m['mca']), {'baseline', 'candidate'})
                    extract = (ROOT / m['mca'][variant['id']]['source']).read_text()
                    extracted = '\n'.join(line for line in extract.splitlines()
                                          if not line.startswith('#')) + '\n'
                    self.assertEqual(extracted, instructions)
                    self.assertNotRegex(extracted, r'\b(ret|jnz|b\.ne|subs)\b')


if __name__ == '__main__':
    unittest.main()

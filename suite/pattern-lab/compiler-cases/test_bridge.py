"""Source-only tests; no compiler, fake compiler, native code or timing work."""
import importlib.util
from pathlib import Path
import re
import unittest

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bridge = load('bridge_capture', HERE / 'capture.py')
# Reuse the existing restricted expression grammar and independent mathematical
# definitions, not an implementation model copied into this bridge.
math = load('bridge_math', HERE.parent / 'tests/test_more_patterns.py')


def evaluate(family, count, seed, data, mask):
    source = (HERE / 'recurrences.c').read_text()
    symbol = 'bridge_' + family.replace('-', '_')
    match = re.search(r'uint64_t ' + symbol + r'\(uint64_t iterations, uint64_t seed,\s*'
                      r'const uint64_t \*data, uint64_t mask\) \{(.*?)\n\}', source, re.S)
    if not match:
        raise AssertionError('unknown signature')
    body = match[1].replace('    (void)data;\n', '').replace('    (void)mask;\n', '')
    match = re.fullmatch(r'\s*uint64_t state = seed;\n    while \(iterations != 0\) \{\n'
                        r'(.*?)        iterations--;\n    }\n    return state;', body, re.S)
    if not match:
        raise AssertionError('unknown loop control')
    assignments = []
    for line in match[1].splitlines():
        assignment = re.fullmatch(r'        (?:uint64_t )?(\w+) = (.+);', line)
        if not assignment:
            raise AssertionError('unknown statement: ' + line)
        name, expression = assignment.groups()
        # Integer suffix removal only for this exact unsigned literal grammar.
        expression = re.sub(r'\b(0x[0-9a-f]+|[0-9]+)ULL\b', r'\1', expression)
        assignments.append((name, expression))
    env = dict(state=seed, data=data, mask=mask)
    for _ in range(count):
        for name, expression in assignments:
            env[name] = math.c_expression(expression, env) & math.U64
    return env['state']


class Semantics(unittest.TestCase):
    def test_source_against_independent_recurrences(self):
        arrays = [[math.U64] * 256, [(i << 48) | (255 - i) for i in range(256)]]
        for family in bridge.FAMILIES:
            for seed in math.SEEDS:
                self.assertEqual(evaluate(family, 0, seed, None, 255), seed)
                for data in arrays:
                    for n in (1, 2, 17):
                        self.assertEqual(evaluate(family, n, seed, data, 255),
                                         math.reference(family, n, seed, data, 255))

    def test_all_sign_bytes_and_memory_addresses(self):
        for byte in range(256):
            self.assertEqual(evaluate('sext8', 1, byte, None, 0),
                             (math.SALT + (byte if byte < 128 else byte - 256)) & math.U64)
        data = [(i << 40) | (255-i) for i in range(256)]
        for index in range(256):
            seed = index << 8
            self.assertEqual(evaluate('narrow-load', 1, seed, data, 255), 255-index)
            self.assertEqual(evaluate('branch-select', 1, seed, data, 255),
                             seed + (3 if (255-index) % 2 else 5))


class Parser(unittest.TestCase):
    def observed(self, family, body, arm=True):
        symbol = 'bridge_' + family.replace('-', '_')
        assembly = '.globl _' + symbol + '\n_' + symbol + ':\n' + body + '\n.cfi_endproc\n'
        return bridge.shape(assembly, family, bridge.TARGETS[0 if arm else 1])

    def test_candidate_baseline_alternative_and_missing(self):
        cases = [('madd', 'madd x1, x1, x5, x6', 'mul x4, x1, x5'),
                 ('rotate', 'ror x1, x1, #13', 'orr x1, x4, x5'),
                 ('udiv10', 'umulh x1, x1, x7', 'udiv x1, x1, x5'),
                 ('sext8', 'sxtb x1, w1', 'asr x1, x1, #56'),
                 ('branch-select', 'csel x1, x5, x6, ne', 'tbz x4, #0, L_zero'),
                 ('narrow-load', 'ldrb w1, [x4]', 'and x1, x1, #255')]
        for family, candidate, baseline in cases:
            for instruction, expected in [(candidate, 'supported'), (baseline, 'missed'), ('ret', 'alternative')]:
                result = self.observed(family, instruction)
                self.assertEqual(result['status'], expected)
                self.assertIsNone(result['code_size_bytes'])
        self.assertEqual(bridge.shape('', 'rotate', bridge.TARGETS[0])['status'], 'unsupported')

    def test_function_boundaries_directives_and_operand_evidence(self):
        source = '''.globl bridge_rotate
bridge_rotate:
.cfi_startproc
.L_loop:
  ror rax, 13
  jne .L_loop
  ret
.cfi_endproc
.globl bridge_madd
bridge_madd:
  imul rax, 3
.cfi_endproc
'''
        for quoted in [source, source.replace('bridge_rotate:', '"bridge_rotate":')]:
            self.assertEqual(bridge.shape(quoted, 'rotate', bridge.TARGETS[1])['status'], 'supported')
        result = bridge.shape(source, 'rotate', bridge.TARGETS[1])
        self.assertEqual(result['evidence'], ['ror rax, 13'])
        self.assertEqual(result['instruction_count'], 3)
        self.assertNotIn('imul rax, 3', result['instructions'])
        self.assertEqual(self.observed('rotate', 'ror x1, x1, #7')['status'], 'alternative')
        self.assertEqual(self.observed('branch-select', 'cbnz x0, L_loop')['status'], 'alternative')
        self.assertEqual(self.observed('branch-select', 'jne .L_loop', False)['status'], 'alternative')
        with self.assertRaises(ValueError):
            bridge.instructions('bridge_madd:\nret', 'bridge_madd')
        with self.assertRaises(ValueError):
            bridge.instructions(source + source, 'bridge_rotate')

    def test_real_arm_comments_and_local_labels(self):
        assembly = """_bridge_rotate:
Lbridge_rotate_begin:
    ; regalloc: v0=x9 v1=x10
    .cfi_startproc
Lbridge_rotate_block0:
    ; v0 = iconst.i64 13
    ror x1, x1, #13
    ret
Lbridge_rotate_end:
    .cfi_endproc
"""
        result = bridge.shape(assembly, 'rotate', bridge.TARGETS[0])
        self.assertEqual(result['status'], 'supported')
        self.assertEqual(result['instructions'], ['ror x1, x1, #13', 'ret'])

    def test_unknown_labels_cannot_supply_shape_evidence(self):
        for label in ('other_function', '"other_function"', '.other_function',
                      'Lother_function_block0', 'Lbridge_rotateimposter_block0'):
            assembly = 'bridge_rotate:\nret\n' + label + ':\nror rax, 13\n.cfi_endproc\n'
            result = bridge.shape(assembly, 'rotate', bridge.TARGETS[1])
            self.assertEqual(result['status'], 'unsupported', label)
            self.assertEqual(result['evidence'], [])
            self.assertIn('unknown nonlocal label', result['reason'])

    def test_rotate_exact_immediates_intel_att_and_arm(self):
        for insn in ('ror rax, 13', 'ror rax, 0xd', 'rorq $13, %rax',
                     'rorq $0xd, %r13', 'ror r13, 13'):
            self.assertEqual(self.observed('rotate', insn, False)['status'], 'supported', insn)
        for insn in ('ror r13, 7', 'ror rax, 113', 'ror rax, 0x1d',
                     'ror r13, cl', 'rorq $7, %r13', 'rorq $113, %rax',
                     'rorq $0x1d, %rax', 'ror rax, -13', 'rorq $-13, %rax'):
            self.assertEqual(self.observed('rotate', insn, False)['status'], 'alternative', insn)
        for insn in ('ror x1, x1, #113', 'ror x13, x13, #7'):
            self.assertEqual(self.observed('rotate', insn)['status'], 'alternative')
        self.assertEqual(self.observed('rotate', 'ror x1, x1, #0xd')['status'], 'supported')

    def test_intel_and_att_candidate_forms(self):
        for family, insn in [('madd', 'lea rax, [rsi + rsi*2 + 7]'),
                             ('madd', 'leaq 7(%rsi,%rsi,2), %rax'),
                             ('sext8', 'movsx rax, sil'),
                             ('sext8', 'movsbq %sil, %rax'),
                             ('narrow-load', 'movzx eax, byte ptr [rdx]'),
                             ('narrow-load', 'movzbl (%rdx), %eax'),
                             ('udiv10', 'mul r9'), ('branch-select', 'cmovne rsi, r8')]:
            self.assertEqual(self.observed(family, insn, False)['status'], 'supported', insn)


if __name__ == '__main__':
    unittest.main()

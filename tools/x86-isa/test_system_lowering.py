import copy
import json
import unittest
from collections import Counter
from pathlib import Path

import system_intrinsics
import system_lowering


class SystemLoweringTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads(Path(__file__).with_name('system-intrinsics.json').read_text())
        self.rows = self.schema['intrinsics']

    def test_existing_scope_has_one_recipe_per_declaration(self):
        system_intrinsics.validate(self.schema)
        self.assertEqual(Counter(r['lowering']['kind'] for r in self.rows), {
            'fixed': 7, 'system_register': 26, 'unary': 9,
            'adjacent_fixed': 1, 'handwritten': 2,
        })
        source = system_lowering.generate(self.rows)
        for row in self.rows:
            self.assertEqual(source.count('Intrinsic::' + row['variant'] + ' =>'), 1)
        self.assertIn('Opcode::Core(CoreOpcode::Lgdt)', source)
        self.assertIn('Opcode::Fixed(FixedOpcode::Swapgs)', source)
        self.assertIn('HandwrittenMsr(Intrinsic::X86Rdmsr)', source)
        self.assertNotIn('Opcode::parse', source)

    def test_new_binding_uses_an_existing_recipe_without_a_backend_allowlist(self):
        schema = copy.deepcopy(self.schema)
        row = dict(schema['intrinsics'][-1], id=819, variant='X86Example', name='x86_example')
        schema['intrinsics'].append(row)
        system_intrinsics.validate(schema)
        source = system_lowering.generate(schema['intrinsics'])
        self.assertIn('Intrinsic::X86Example => SystemLowering::Fixed(Opcode::Fixed(FixedOpcode::Swapgs))', source)

    def rejects(self, variant, change, message):
        rows = copy.deepcopy(self.rows)
        row = next(r for r in rows if r['variant'] == variant)
        change(row)
        with self.assertRaisesRegex(ValueError, message):
            system_lowering.validate(rows)

    def recipe_change(self, variant, field, value, message):
        self.rejects(variant, lambda r: r['lowering'].__setitem__(field, value), message)

    def test_signature_and_argument_validation(self):
        for name in ['X86Clts', 'X86ReadCr0', 'X86LldtMem', 'X86Rdmsr', 'X86StiHlt']:
            self.rejects(name, lambda r: r.__setitem__('params', ['I64', 'I64']), 'signature')
        for argument in [-1, 1, True, '0']:
            self.recipe_change('X86Lgdt', 'argument', argument, 'sole input')
        self.recipe_change('X86ReadCr0', 'direction', 'write', 'signature')

    def test_register_domain_comes_from_the_register_schema(self):
        for register in ['cr1', 'cr00', 'dr8', 'rax', 'cr9', None]:
            self.recipe_change('X86ReadCr0', 'register', register, 'register')
        self.recipe_change('X86ReadCr0', 'direction', 'exchange', 'direction')

    def test_unary_width_role_and_existing_adapter_validation(self):
        for bits in [0, 7, 128, True, '80']:
            self.recipe_change('X86Lgdt', 'bits', bits, 'width')
        self.recipe_change('X86Lgdt', 'bits', 16, 'imported operand form')
        self.recipe_change('X86Lgdt', 'operand', 'register', 'imported operand form')
        self.recipe_change('X86Lgdt', 'operand', 'write', 'operand role')
        self.recipe_change('X86Lgdt', 'mnemonic', 'not_an_opcode', 'Core opcode')
        invlpg = next(r for r in self.rows if r['variant'] == 'X86Invlpg')
        self.assertEqual(invlpg['lowering']['operand'], 'address_only')
        self.assertIn('operand: SystemOperand::AddressOnly', system_lowering.generate(self.rows))

    def test_fixed_rejects_operands_register_clobbers_and_terminal_transfers(self):
        for mnemonic in ['lgdt', 'cpuid', 'rdmsr', 'iretq', 'sysret',
                         'sysexit', 'sysexitq', 'rsm', 'not_an_opcode', None]:
            self.recipe_change('X86Clts', 'mnemonic', mnemonic, 'zero-operand')

    def test_adjacent_pair_is_not_a_general_sequence_language(self):
        for instructions in [[], ['hlt', 'sti'], ['sti'], ['sti', 'pause', 'hlt']]:
            self.recipe_change('X86StiHlt', 'mnemonics', instructions, 'reviewed adjacent')
        self.recipe_change('X86StiHlt', 'argument', 0, 'reviewed adjacent')

    def test_missing_unknown_recipes_and_hooks_fail_generation(self):
        self.rejects('X86Clts', lambda r: r.pop('lowering'), 'requires a lowering recipe')
        self.recipe_change('X86Clts', 'kind', 'arbitrary_rust', 'unknown system lowering')
        self.recipe_change('X86Clts', 'rust', 'unsafe {}', 'recipe fields')
        self.recipe_change('X86Rdmsr', 'handler', 'unknown', 'unknown handwritten')
        self.recipe_change('X86Rdmsr', 'handler', 'wrmsr', 'signature')


if __name__ == '__main__':
    unittest.main()

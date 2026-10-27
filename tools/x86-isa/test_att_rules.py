"""Pins and schema errors for the AT&T mappings moved from 14bed00d."""
import copy
import hashlib
import json
import unittest

import att_rules
import generate


class AttRuleTests(unittest.TestCase):
    def setUp(self):
        load = lambda name: json.loads((generate.ROOT / name).read_text())
        self.schema = load('att-rules.json')
        self.args = [load('api-identities.json'), load('core-identities.json'),
                     load('keel-adapters.json')]

    def assert_invalid(self, mutate, message):
        schema = copy.deepcopy(self.schema)
        mutate(schema)
        with self.assertRaisesRegex(ValueError, message):
            att_rules.generate(schema, *self.args)

    def test_rule_membership_and_widths(self):
        digest = hashlib.sha256(json.dumps(self.schema, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, '24868352fc3aa42134c6c9197f69b4cb7a8fea9b8ec94cda5ae81d6ef87847af')
        att_rules.generate(self.schema, *self.args)

    def test_unknown_groups_and_opcodes_are_rejected(self):
        self.assert_invalid(lambda d: d.update(unknown=[]), 'rule groups')
        self.assert_invalid(lambda d: d['scalar']['baseline'].append('NotAnOpcode'), 'unknown AT&T opcode')
        self.assert_invalid(lambda d: d['scalar'].update(unknown=[]), 'scalar opcode groups')

    def test_duplicate_scalar_members_and_aliases_are_rejected(self):
        self.assert_invalid(lambda d: d['scalar']['baseline'].append('Mov'), 'duplicate scalar')
        self.assert_invalid(lambda d: d['aliases'].append(d['aliases'][0]), 'duplicate AT&T spelling')

    def test_invalid_width_and_suffix_rules_are_rejected(self):
        self.assert_invalid(lambda d: d['scalar_suffixes'].update(z=99), 'suffix rule')
        self.assert_invalid(lambda d: d['legacy_x87_memory'][0].update(width=17), 'width')
        self.assert_invalid(lambda d: d['packed_suffixes'].update(z=128), 'duplicate')

    def test_extensions_must_widen_and_have_known_shape(self):
        self.assert_invalid(lambda d: d['extensions'][0].update(source=64), 'must widen')
        self.assert_invalid(lambda d: d['extensions'][0].update(encoding=1), 'row fields')

    def test_conversion_operand_indices_are_bounded_and_unique(self):
        self.assert_invalid(lambda d: d['integer_conversions'][0].update(operand=3), 'conversion operand')
        self.assert_invalid(lambda d: d['integer_conversions'].append(d['integer_conversions'][0]), 'conversion operand')

    def test_x87_domains_and_suffixes_are_checked(self):
        self.assert_invalid(lambda d: d['x87_memory'][0].update(opcode='Mov'), 'unknown x87')
        self.assert_invalid(lambda d: d['x87_memory'][0]['suffixes'].update(q=64), 'invalid x87 suffix')
        self.assert_invalid(lambda d: d['x87_memory'].append(d['x87_memory'][0]), 'duplicate x87')


if __name__ == '__main__':
    unittest.main()

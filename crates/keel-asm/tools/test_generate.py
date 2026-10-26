"""Decoder predicate simplification preserves validation and field constraints."""
import copy
import json
import re
import unittest

import generate


class DecoderPredicateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        rows = json.loads((generate.ROOT / 'isa/a64.json').read_text())['instructions']
        cls.row = next(row for row in rows if any(f.get('name') == 'size' for f in row['encoding']['values']))

    def output(self, condition):
        row = copy.deepcopy(self.row)
        row['condition'] = condition
        return generate.generate_decoder({'instructions': [row]})

    def test_boolean_identities_and_duplicate_terms_preserve_field_masks(self):
        field = {'_type': 'AST.BinaryOp', 'op': '==',
                 'left': {'_type': 'AST.Identifier', 'value': 'size'},
                 'right': {'_type': 'Values.Value', 'value': "'00'", 'meaning': None}}
        true = {'_type': 'AST.Bool', 'value': True}
        false = {'_type': 'AST.UnaryOp', 'op': '!', 'expr': true}
        expected = self.output(field)
        for op, left, right in [('&&', field, true), ('&&', true, field),
                                ('||', field, false), ('||', false, field),
                                ('&&', field, field), ('||', field, field)]:
            with self.subTest(op=op, left=left, right=right):
                self.assertEqual(self.output({'_type': 'AST.BinaryOp', 'op': op,
                                              'left': left, 'right': right}), expected)

    def test_constant_feature_predicates_are_simplified(self):
        true = {'_type': 'AST.Bool', 'value': True}
        for op in ['&&', '||']:
            self.assertEqual(self.output({'_type': 'AST.BinaryOp', 'op': op,
                                          'left': true, 'right': true}), self.output(true))
        self.assertNotIn('!(true)', self.output({'_type': 'AST.UnaryOp', 'op': '!', 'expr': true}))

    def test_discarded_branches_still_validate(self):
        unknown = {'_type': 'AST.Function', 'name': 'IsFeatureImplemented',
                   'parameters': [], 'arguments': [{'_type': 'AST.Identifier', 'value': 'FEAT_UNKNOWN'}]}
        for op in ['&&', '||']:
            with self.subTest(op=op), self.assertRaises(AssertionError):
                self.output({'_type': 'AST.BinaryOp', 'op': op,
                             'left': {'_type': 'AST.Bool', 'value': True}, 'right': unknown})

    def test_constant_results_keep_the_discarded_field_mask(self):
        field = {'_type': 'AST.BinaryOp', 'op': '==',
                 'left': {'_type': 'AST.Identifier', 'value': 'size'},
                 'right': {'_type': 'Values.Value', 'value': "'00'", 'meaning': None}}
        true = {'_type': 'AST.Bool', 'value': True}
        false = {'_type': 'AST.UnaryOp', 'op': '!', 'expr': true}
        mask = re.search(r'predicate_mask: \d+', self.output(field)).group()
        for op, constant, result in [('&&', false, 'false'), ('||', true, 'true')]:
            for left, right in [(field, constant), (constant, field)]:
                output = self.output({'_type': 'AST.BinaryOp', 'op': op,
                                      'left': left, 'right': right})
                self.assertIn(mask, output)
                self.assertRegex(output, rf'let _ = word;\s*{result}\s*\}}')


if __name__ == '__main__':
    unittest.main()

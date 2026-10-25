import copy
import json
import unittest
from pathlib import Path

import registers


class RegisterSchemaTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads(Path(__file__).with_name('registers.json').read_text())

    def test_ids_and_symbolic_tags_have_independent_unique_domains(self):
        registers.validate(self.schema)
        for key in ['id', 'symbolic_tag']:
            bad = copy.deepcopy(self.schema)
            bad['classes'][1][key] = bad['classes'][0][key]
            with self.assertRaisesRegex(ValueError, key):
                registers.validate(bad)

    def test_named_numeric_and_alias_spellings_cannot_overlap(self):
        for edit in ['named', 'numeric', 'alias']:
            bad = copy.deepcopy(self.schema)
            if edit == 'named':
                bad['classes'][0]['views'][0]['names'][1] = 'al'
            elif edit == 'numeric':
                bad['classes'][0]['views'][0]['numbered']['numbers'].insert(0, 0)
            else:
                bad['classes'][4]['aliases'][0]['name'] = 'al'
            with self.assertRaisesRegex(ValueError, 'duplicate|overlapping'):
                registers.validate(bad)

    def test_views_and_high_byte_names_must_have_valid_number_domains(self):
        self.schema['classes'][0]['views'][0]['numbered']['numbers'].pop()
        with self.assertRaisesRegex(ValueError, 'number domain'):
            registers.validate(self.schema)
        self.setUp()
        self.schema['classes'][0]['high_byte']['first'] = 15
        with self.assertRaisesRegex(ValueError, 'high-byte'):
            registers.validate(self.schema)

    def test_isa_kinds_and_numeric_patterns_cannot_be_ambiguous(self):
        self.schema['classes'][1]['isa_kinds'] = ['gpr8']
        with self.assertRaisesRegex(ValueError, 'ISA register kind'):
            registers.validate(self.schema)
        self.setUp()
        self.schema['classes'][3]['views'][0]['numbered']['prefix'] = 'xmm'
        with self.assertRaisesRegex(ValueError, 'numeric register pattern'):
            registers.validate(self.schema)

    def test_unknown_formatting_checks_and_decode_views_fail_closed(self):
        self.schema['classes'][0]['format']['check'].append('new_constraint')
        with self.assertRaisesRegex(ValueError, 'formatting check'):
            registers.validate(self.schema)
        self.setUp()
        self.schema['classes'][1]['decode']['width'] = 512
        with self.assertRaisesRegex(ValueError, 'decoder register view'):
            registers.validate(self.schema)

    def test_unbounded_and_single_digit_numeric_spelling_policies_are_explicit(self):
        self.schema['classes'][1]['views'][0]['numbered']['parse_digits'] = 0
        with self.assertRaisesRegex(ValueError, 'numeric register length'):
            registers.validate(self.schema)

    def test_unknown_schema_constraints_are_not_silently_ignored(self):
        self.schema['classes'][0]['format']['future_constraint'] = True
        with self.assertRaisesRegex(ValueError, 'unknown.*schema fields'):
            registers.validate(self.schema)

    def test_full_byte_number_domain_does_not_emit_an_overflowing_bound(self):
        self.assertEqual(registers.domain('number', list(range(256))), 'true')


if __name__ == '__main__':
    unittest.main()

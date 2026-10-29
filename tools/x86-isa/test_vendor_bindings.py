import copy
import json
import tempfile
import unittest
from pathlib import Path

import generate
import vendor_bindings


class VendorBindingTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads((generate.ROOT / 'vendor-bindings.json').read_text())

    def rejects(self, change, message):
        schema = copy.deepcopy(self.schema)
        change(schema['bindings'][0])
        with self.assertRaisesRegex(ValueError, message):
            vendor_bindings.generate(schema, generate.ROOT)

    def test_existing_bindings_and_new_semantic_bridge_are_admitted_once(self):
        rows = vendor_bindings.validate(self.schema, generate.ROOT)
        names = [row['builtin'] for row in rows]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual({family: sum(row['family'] == family for row in rows)
                          for family in vendor_bindings.FAMILIES},
                         {'aes': 6, 'packed_shift': 16, 'saturating': 8,
                          'packed_conversion': 3, 'value': 36})
        source = vendor_bindings.generate(self.schema, generate.ROOT)
        self.assertEqual(source.count(' => Binding::'), len(names))
        self.assertIn('pub(crate) fn recognized(', source)
        self.assertIn('pub(crate) fn semantic_builtin(', source)
        self.assertIn('fn reviewed_signatures_match_semantic_intrinsics()', source)

    def test_identity_registry_and_recipes_fail_closed(self):
        for fields, message in [
            ({'intrinsic_id': -1}, 'ID mismatch'),
            ({'intrinsic': 'Imaginary'}, 'ID mismatch'),
            ({'family': 'opcode'}, 'recipe'),
            ({'control': 'dynamic'}, 'control'),
            ({'features': ['not_a_feature']}, 'features'),
            ({'ownership': 'keel'}, 'ownership'),
            ({'reference': 'unreviewed'}, 'reference'),
            ({'params': ['i512']}, 'unknown type'),
            ({'result': 'f32x4'}, 'AES signature'),
        ]:
            self.rejects(lambda row: row.update(fields), message)
        self.schema['bindings'].append(copy.deepcopy(self.schema['bindings'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            vendor_bindings.generate(self.schema, generate.ROOT)

    def test_type_projection_preserves_ir_shape_without_c_signedness(self):
        self.assertEqual(vendor_bindings.ir_type('u16x8'), 'Type::Vector(VectorType::I16x8)')
        self.assertEqual(vendor_bindings.ir_type('int_i64x2'), 'Type::Vector(VectorType::I64x2)')
        self.assertEqual(vendor_bindings.ir_type('same_as_input', 'int_i32x4'),
                         'Type::Vector(VectorType::I32x4)')
        self.assertEqual(vendor_bindings.ir_type('u64'), 'Type::I64')

    def test_reference_hash_check_is_explicit_and_rejects_wrong_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            for reference in vendor_bindings.references(generate.ROOT).values():
                for header in reference.get('headers', {}):
                    (path / header).write_text('not the pinned source')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                vendor_bindings.check_reference_headers(generate.ROOT, path)

    def test_private_bridges_do_not_claim_vendor_builtin_identity(self):
        for row in self.schema['bindings']:
            if row['builtin'].startswith('keel_'):
                self.assertEqual(row['ownership'], 'keel')
        self.assertEqual(self.schema['reference_file'], 'vendor-references.json')

    def test_value_controls_match_the_parser_recipe(self):
        for original in self.schema['bindings']:
            if original['family'] != 'value':
                continue
            for control in ['none', 'imm8', 'u64_count', 'rounding']:
                if control == original['control']:
                    continue
                schema = copy.deepcopy(self.schema)
                row = next(row for row in schema['bindings'] if row['builtin'] == original['builtin'])
                row['control'] = control
                with self.assertRaisesRegex(ValueError, 'value control'):
                    vendor_bindings.validate(schema, generate.ROOT)

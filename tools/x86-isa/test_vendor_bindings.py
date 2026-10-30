import copy
import json
import tempfile
import unittest
from unittest.mock import patch
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
                         {'aes': 6, 'packed_shift': 32, 'saturating': 8,
                          'packed_conversion': 3, 'value': 1520})
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
        self.assertEqual(vendor_bindings.ir_type('ptr'), 'Type::Ptr')

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
        # Whole-manifest validation above admits every reviewed mapping. Each
        # distinct parser recipe needs one negative matrix, not one per alias.
        representatives = {}
        for row in self.schema['bindings']:
            if row['family'] == 'value':
                representatives.setdefault(row['control'], row)
        self.assertEqual(set(representatives), {'none'} | vendor_bindings.VALUE_CONTROLS.keys())
        for expected, original in representatives.items():
            for control in vendor_bindings.CONTROLS - {expected}:
                schema = copy.deepcopy(self.schema)
                row = next(row for row in schema['bindings'] if row['builtin'] == original['builtin'])
                row['control'] = control
                with self.subTest(expected=expected, rejected=control):
                    with self.assertRaisesRegex(ValueError, 'value control'):
                        vendor_bindings.validate(schema, generate.ROOT)

    def test_control_recipe_requires_the_parser_control_position_and_type(self):
        for params in [['f32x4'], ['f32x4', 'u64'], ['i32', 'f32x4']]:
            schema = copy.deepcopy(self.schema)
            row = next(row for row in schema['bindings'] if row['control'] == 'round_imm4')
            row['params'] = params
            with self.assertRaisesRegex(ValueError, 'trailing i32'):
                vendor_bindings.validate(schema, generate.ROOT)

    def test_gather_scales_generate_a_closed_set_at_the_correct_position(self):
        schema = copy.deepcopy(self.schema)
        row = next(row for row in schema['bindings'] if row['control'] == 'gather_scale')
        self.assertEqual(row['params'], ['ptr', 'i32x4', 'f32x4', 'i32x4', 'i32'])
        source = vendor_bindings.generate(schema, generate.ROOT)
        self.assertIn('allowed_values: &[1, 2, 4, 8], index: 4, maximum: 8, mask: 15', source)
        row['params'] = row['params'][:-1]
        with self.assertRaisesRegex(ValueError, 'trailing i32'):
            vendor_bindings.validate(schema, generate.ROOT)

    def test_value_void_pointer_and_zero_arguments_generate_output_slices(self):
        schema = copy.deepcopy(self.schema)
        row = next(row for row in schema['bindings'] if row['family'] == 'value' and row['control'] == 'none')
        row.update(params=['ptr'], result='void')
        source = vendor_bindings.generate(schema, generate.ROOT)
        self.assertIn('&[Type::Ptr], &[])', source)
        row.update(params=[], result='i32')
        source = vendor_bindings.generate(schema, generate.ROOT)
        self.assertIn('&[], &[Type::I32])', source)

    def test_independent_data_immediates_keep_their_type_checks(self):
        for control in ['avx512_mant_controls', 'avx512_roundscale_controls', 'avx512_fixup_controls']:
            schema = copy.deepcopy(self.schema)
            row = next(row for row in schema['bindings'] if row['control'] == control)
            index = vendor_bindings.VALUE_EXTRA_CONTROLS[control][0][0]
            row['params'][index] = 'f32'
            with self.subTest(control=control):
                with self.assertRaisesRegex(ValueError, f'i32 parameter at index {index}'):
                    vendor_bindings.validate(schema, generate.ROOT)

    def test_reviewed_c_alias_controls_cannot_replace_raw_controls(self):
        alias = next(row for row in self.schema['bindings']
                     if row['builtin'] == 'keel_avx512vl_c_vpermilpdimm128')
        raw = next(row for row in self.schema['bindings']
                   if row['builtin'] == 'keel_avx512vl_vpermilpdimm128')
        for name, control in [(alias['builtin'], raw['control']),
                              (raw['builtin'], alias['control'])]:
            schema = copy.deepcopy(self.schema)
            next(row for row in schema['bindings'] if row['builtin'] == name)['control'] = control
            with self.subTest(builtin=name):
                with self.assertRaisesRegex(ValueError, 'value control differs'):
                    vendor_bindings.validate(schema, generate.ROOT)
        schema = copy.deepcopy(self.schema)
        other = next(row for row in schema['bindings']
                     if row['builtin'] == 'keel_avx512vl_vpermilpdimm256')
        changed = next(row for row in schema['bindings'] if row['builtin'] == alias['builtin'])
        changed['intrinsic'] = other['intrinsic']
        changed['intrinsic_id'] = other['intrinsic_id']
        with self.assertRaisesRegex(ValueError, 'value control alias differs'):
            vendor_bindings.validate(schema, generate.ROOT)

    def test_control_declaration_order_does_not_rebind_reviewed_aliases(self):
        reversed_controls = dict(reversed(list(vendor_bindings.VALUE_CONTROLS.items())))
        with patch.object(vendor_bindings, 'VALUE_CONTROLS', reversed_controls):
            vendor_bindings.validate(self.schema, generate.ROOT)

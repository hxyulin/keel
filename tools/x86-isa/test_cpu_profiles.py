import copy
import json
import unittest
from pathlib import Path

import cpu_profiles
import feature_fields


class CpuProfileTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parent
        self.fields = json.loads((root / 'feature-fields.json').read_text())
        self.profiles = json.loads((root / 'cpu-profiles.json').read_text())

    def test_default_and_current_memberships(self):
        self.assertEqual(self.profiles['default'], 'v1')
        self.assertEqual([r['name'] for r in self.profiles['profiles']], ['v1', 'v2', 'v3'])
        self.assertEqual([len(r['features']) for r in self.profiles['profiles']], [0, 7, 10])
        cpu_profiles.generate(self.profiles, self.fields)

    def test_requirement_masks_include_prerequisite_closure(self):
        policy = feature_fields.closed_policy(self.fields,
            {'AVX2': ['avx2'], 'AES': ['aes'], 'BASE': [], 'EXCLUDED': None})
        self.assertEqual(policy['AVX2'], ['avx', 'avx2', 'xsave', 'osxsave'])
        self.assertEqual(policy['AES'], ['aes'])
        self.assertEqual(policy['BASE'], [])
        self.assertIsNone(policy['EXCLUDED'])

    def test_unknown_profile_features_and_parents(self):
        for key, value in [('features', ['unknown']), ('extends', 'unknown')]:
            schema = copy.deepcopy(self.profiles)
            schema['profiles'][1][key] = value
            with self.assertRaisesRegex(ValueError, 'invalid CPU profile feature|unknown parent'):
                cpu_profiles.generate(schema, self.fields)

    def test_profile_cycles_and_duplicate_names(self):
        schema = copy.deepcopy(self.profiles)
        schema['profiles'][0]['extends'] = 'v3'
        with self.assertRaisesRegex(ValueError, 'cycle'):
            cpu_profiles.generate(schema, self.fields)
        schema = copy.deepcopy(self.profiles)
        schema['profiles'].append(schema['profiles'][0])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            cpu_profiles.generate(schema, self.fields)

    def test_profile_dependency_and_cumulative_checks(self):
        schema = copy.deepcopy(self.profiles)
        schema['profiles'][2]['features'].remove('xsave')
        with self.assertRaisesRegex(ValueError, 'lacks prerequisites'):
            cpu_profiles.generate(schema, self.fields)
        schema = copy.deepcopy(self.profiles)
        del schema['profiles'][2]['extends']
        with self.assertRaisesRegex(ValueError, 'cumulative'):
            cpu_profiles.generate(schema, self.fields)

    def test_unknown_default_and_schema_fields(self):
        schema = copy.deepcopy(self.profiles)
        schema['default'] = 'legacy'
        with self.assertRaisesRegex(ValueError, 'default'):
            cpu_profiles.generate(schema, self.fields)
        schema = copy.deepcopy(self.profiles)
        schema['unknown'] = 1
        with self.assertRaisesRegex(ValueError, 'fields'):
            cpu_profiles.generate(schema, self.fields)

    def test_duplicate_bits_and_out_of_range_bits(self):
        for bit in [self.fields[1]['bit'], -1, 64, True]:
            fields = copy.deepcopy(self.fields)
            fields[0]['bit'] = bit
            with self.assertRaisesRegex(ValueError, 'feature bit'):
                feature_fields.generate(fields, {})

    def test_dependency_cycles_and_unknown_prerequisites(self):
        fields = copy.deepcopy(self.fields)
        fields[0]['requires'] = ['unknown']
        with self.assertRaisesRegex(ValueError, 'prerequisites'):
            feature_fields.generate(fields, {})
        fields[0]['requires'] = ['aes']
        with self.assertRaisesRegex(ValueError, 'cycle'):
            feature_fields.generate(fields, {})

    def test_reserved_cli_spellings(self):
        for name in ['none', 'explicit', '+aes', 'a,b', ' aes']:
            fields = copy.deepcopy(self.fields)
            fields[0]['aliases'] = [name]
            with self.assertRaisesRegex(ValueError, 'spelling'):
                feature_fields.generate(fields, {})

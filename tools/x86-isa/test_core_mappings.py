"""API pins captured from 3109c651, plus invalid schema checks."""
import copy
import hashlib
import json
import unittest

import core_mappings
import generate


class CoreMappingTests(unittest.TestCase):
    def setUp(self):
        self.core = json.loads((generate.ROOT / 'core-identities.json').read_text())
        self.conditions = json.loads((generate.ROOT / 'condition-codes.json').read_text())

    def test_core_order_shapes_and_names_match_original(self):
        digest = hashlib.sha256(json.dumps(self.core, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, '168365a40fc8bde0b74fbda19f49b307fc8b59b57246d07fb75cf7e3c81aad8a')
        core_mappings.core_sources(self.core)

    def test_condition_ids_aliases_and_both_name_sets_match_original(self):
        digest = hashlib.sha256(json.dumps(self.conditions, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, 'd5e712204d84497faaec5293e7603e65c6f20755bb096a20cea1e87d0ab9533c')
        core_mappings.condition_source(self.conditions)

    def test_duplicate_core_identity_and_spelling_are_rejected(self):
        for field in ['variant', 'spellings']:
            rows = copy.deepcopy(self.core)
            rows[3][field] = rows[2][field]
            with self.assertRaisesRegex(ValueError, 'duplicate'):
                core_mappings.core_sources(rows)

    def test_payload_api_and_unknown_fields_are_rejected(self):
        rows = copy.deepcopy(self.core)
        rows[0]['fields'][1][1] = 'u8'
        with self.assertRaisesRegex(ValueError, 'payload API changed'):
            core_mappings.core_sources(rows)
        rows = copy.deepcopy(self.core)
        rows[2]['encoding'] = 0x90
        with self.assertRaisesRegex(ValueError, 'unknown'):
            core_mappings.core_sources(rows)

    def test_condition_missing_or_reordered_ids_are_rejected(self):
        for rows in [self.conditions[:-1], list(reversed(self.conditions))]:
            with self.assertRaisesRegex(ValueError, '0 through 15'):
                core_mappings.condition_source(rows)

    def test_duplicate_condition_alias_and_nonalias_name_are_rejected(self):
        rows = copy.deepcopy(self.conditions)
        rows[1]['aliases'].append('o')
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            core_mappings.condition_source(rows)
        rows = copy.deepcopy(self.conditions)
        rows[0]['isa_name'] = 'bad'
        with self.assertRaisesRegex(ValueError, 'must be aliases'):
            core_mappings.condition_source(rows)


if __name__ == '__main__':
    unittest.main()

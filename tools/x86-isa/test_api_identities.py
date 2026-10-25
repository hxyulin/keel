"""Compatibility pins captured from db6129d3 before moving the handwritten API."""
import copy
import hashlib
import json
import re
import unittest

import api_identities
import generate


class ApiIdentityTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads((generate.ROOT / 'api-identities.json').read_text())

    def test_public_order_payloads_names_and_spellings_match_original(self):
        # Independent pin: changing a public API requires an intentional review
        # of the old contract, not simply regenerating expected output.
        digest = hashlib.sha256(json.dumps(self.schema, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, '03d95d6201f3d0d3ff10418533a715f25eca52568be2bf1b31ef9ad7e2a0aaac')

    def test_emitted_enum_preserves_original_variant_order_and_payloads(self):
        output = api_identities.outputs(generate.ROOT, generate.OUTPUT, generate.opcode_identities)
        source = output[generate.OUTPUT.with_name('baseline_opcodes.rs')]
        body = source.split('pub enum Opcode {', 1)[1].split('\n}', 1)[0]
        shapes = re.findall(r'^    (\w+(?:\(\w+\))?),$', body, re.M)
        digest = hashlib.sha256('\n'.join(shapes).encode()).hexdigest()
        self.assertEqual(digest, '86883f60ebd837887d3f1dffc5fc1936914d101759c43b17c9dfb8b5c535e30d')

    def test_duplicate_baseline_spelling_is_rejected(self):
        schema = copy.deepcopy(self.schema)
        schema['baseline'][6]['spellings'] = schema['baseline'][7]['spellings']
        self.assert_invalid(schema, 'duplicate')

    def test_parameter_types_are_explicit_api_policy(self):
        schema = copy.deepcopy(self.schema)
        schema['baseline'][0]['payload'] = 'u8'
        self.assert_invalid(schema, 'payload API changed')

    def test_dynamic_names_cannot_become_static(self):
        schema = copy.deepcopy(self.schema)
        schema['baseline'][0]['mnemonic'] = 'core'
        self.assert_invalid(schema, 'must stay handwritten')

    def assert_invalid(self, schema, message):
        from unittest.mock import patch
        with patch.object(api_identities.json, 'loads', return_value=schema):
            with self.assertRaisesRegex(ValueError, message):
                api_identities.outputs(generate.ROOT, generate.OUTPUT, generate.opcode_identities)


if __name__ == '__main__':
    unittest.main()

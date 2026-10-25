import json
import unittest
from pathlib import Path

import feature_fields


class FeatureFieldsTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parent
        self.fields = json.loads((root / 'feature-fields.json').read_text())
        self.policy = json.loads((root / 'feature-policy.json').read_text())

    def test_policy_may_only_require_declared_capabilities(self):
        feature_fields.validate(self.fields, self.policy)
        # Reject even an unused ISA set, before any Rust is written.
        self.policy['UNEXPOSED_ISA'] = ['nonexistent']
        with self.assertRaisesRegex(ValueError, 'UNEXPOSED_ISA.*nonexistent'):
            feature_fields.generate(self.fields, self.policy)

    def test_explicit_is_target_state_not_an_isa_capability(self):
        self.policy['INVALID'] = ['explicit']
        with self.assertRaisesRegex(ValueError, 'unknown feature fields'):
            feature_fields.generate(self.fields, self.policy)

    def test_ambiguous_cli_spellings_are_rejected(self):
        self.fields[0]['aliases'] = ['pclmul']
        with self.assertRaisesRegex(ValueError, 'duplicate.*spelling'):
            feature_fields.generate(self.fields, self.policy)

    def test_duplicate_fields_are_rejected(self):
        self.fields.append(self.fields[0])
        with self.assertRaisesRegex(ValueError, 'duplicate feature field'):
            feature_fields.generate(self.fields, self.policy)


if __name__ == '__main__':
    unittest.main()

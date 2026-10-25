import copy
import json
import unittest

import generate


class GeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads((generate.ROOT / 'zydis.json').read_text())
        cls.adapters = json.loads((generate.ROOT / 'keel-adapters.json').read_text())
        cls.policy = json.loads((generate.ROOT / 'feature-policy.json').read_text())

    def test_unknown_filter_fails_closed(self):
        row = copy.deepcopy(next(r for r in self.data if r['mnemonic'] == 'lgdt'))
        row['filters']['new_constraint'] = '1'
        with self.assertRaises(ValueError):
            generate.generate([row])

    def test_unclassified_feature_set_requires_policy(self):
        row = copy.deepcopy(next(r for r in self.data if r['mnemonic'] == 'aesenc'))
        row['meta_info']['isa_set'] = 'NEW_FEATURE'
        with self.assertRaisesRegex(ValueError, 'unclassified ISA'):
            generate.feature_adapters([row], [{'opcode': 'Opcode::Example',
                                               'source_mnemonic': 'aesenc'}], self.policy)

    def test_explicit_pextrw_form_retains_its_feature_requirement(self):
        text = generate.feature_adapters(self.data, self.adapters['feature_opcodes'], self.policy)
        self.assertIn('Opcode::V2(V2Opcode::PextrwSse4) => Features { sse41: true', text)
        self.assertNotIn('Opcode::Pextrw =>', text)

    def test_legacy_and_vex_api_adapters_cover_the_existing_inventory(self):
        v2 = generate.v2_adapters(self.data, self.adapters['v2'])
        v3 = generate.v3_adapters(self.data, self.adapters['v3'])
        self.assertEqual(v2.count('=> V2Spec'), 93)
        self.assertEqual(v3.count('V3Form { opcode:'), 1202)
        self.assertIn('isa: "AVX2GATHER"', v3)
        self.assertIn('vsib: 256', v3)

    def test_duplicate_public_spellings_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            generate.opcode_identities('Example', [dict(variant='A', mnemonic='a'),
                                                   dict(variant='B', mnemonic='b', aliases=['a'])])

    def test_extension_exposure_requires_source_forms_and_feature_policy(self):
        with self.assertRaisesRegex(ValueError, 'no imported forms'):
            generate.extension_adapters(self.data, ['nonexistent'])
        identities, features, vex = generate.extension_adapters(self.data, self.adapters['extensions'])
        self.assertEqual(len(identities), 16)
        self.assertEqual([i['mnemonic'] for i in identities if i['scalar_suffix']], ['adcx', 'adox'])
        non_scalar, _, _ = generate.extension_adapters(self.data, ['serialize', 'clwb'])
        self.assertFalse(any(i['scalar_suffix'] for i in non_scalar))
        source = generate.feature_adapters(self.data, features, self.policy)
        self.assertIn('ExtensionOpcode::Adcx) => Features { adx: true', source)
        source = generate.v3_adapters(self.data, vex)
        self.assertIn('isa: "AVX_PCLMULQDQ"', source)
        self.assertIn('isa: "VPCLMULQDQ"', source)

    def test_generated_identities_retain_legacy_aliases(self):
        for group, alias in [('v2', 'pcmpestriq'), ('v3', 'vpcmpestriq')]:
            source = generate.opcode_identities(group.upper() + 'Opcode', self.adapters['identities'][group])
            self.assertIn(f'"{alias}" => Self::', source)


if __name__ == '__main__':
    unittest.main()

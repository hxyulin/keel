import copy
import json
import unittest
from pathlib import Path

import coverage_ledger
import generate


class CoverageLedgerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parent
        cls.data = json.loads((cls.root / 'zydis.json').read_text()) + json.loads((cls.root / 'keel-extensions.json').read_text())
        cls.adapters = json.loads((cls.root / 'keel-adapters.json').read_text())
        cls.ledger = coverage_ledger.build(cls.root, cls.data, cls.adapters, generate.exclusion_reasons, generate.form_isa)

    def test_every_source_record_has_one_unique_content_id_and_locator(self):
        forms = self.ledger['forms']
        self.assertEqual(len(forms), len(self.data))
        self.assertEqual(len({f['id'] for f in forms}), len(forms))
        for row, form in zip(self.data, forms):
            expected = coverage_ledger.digest({k: v for k, v in row.items() if k != 'comment'})
            self.assertEqual(form['id'], form['source'] + ':' + expected)
            self.assertEqual(form['mnemonic'], row['mnemonic'])
        self.assertEqual(forms[-1]['source'], 'keel')
        self.assertEqual(forms[-1]['source_index'], 0)
        self.assertEqual(coverage_ledger.digest({'a': 1, 'b': 2}), coverage_ledger.digest({'b': 2, 'a': 1}))

    def test_shared_selection_and_table_order_are_exhaustive(self):
        included = [f for f in self.ledger['forms'] if f['table_index'] is not None]
        self.assertEqual(len(included), 3563)
        self.assertEqual(sorted(f['table_index'] for f in included), list(range(len(included))))
        ordered = sorted(included, key=lambda f: f['table_index'])
        self.assertEqual([f['mnemonic'] for f in ordered], sorted(f['mnemonic'] for f in included))
        self.assertEqual(len(self.data) - len(included), 41)
        for row, form in zip(self.data, self.ledger['forms']):
            self.assertEqual(form['table_index'] is not None, generate.enabled(row))
            if not generate.enabled(row):
                self.assertEqual(form['decoder'], 'excluded')
                self.assertEqual(form['assembler_candidates'], [])
                self.assertEqual(form['compiler_candidates'], [])

    def test_all_exclusion_reasons_are_retained_including_overlaps(self):
        row = {'filters': {'feature_amd': '1', 'feature_knc': '1', 'feature_mpx': '1', 'rex_2': 'rex2'}}
        self.assertEqual(generate.exclusion_reasons(row), ['feature_amd', 'feature_knc', 'feature_mpx'])
        self.assertFalse(generate.enabled(row))
        self.assertTrue(generate.enabled({'filters': {'rex_2': 'no_rex2'}}))

    def test_indexed_decode_policy_is_not_a_decode_success_claim(self):
        disabled = [f for f in self.ledger['forms'] if f['decoder'] == 'disabled_by_policy']
        self.assertEqual(len(disabled), 1)
        self.assertEqual(disabled[0]['mnemonic'], 'ud0')
        self.assertIsNotNone(disabled[0]['table_index'])
        self.assertTrue(all(f['validation'] == 'untracked' for f in self.ledger['forms']))
        self.assertTrue(all(b['validation'] == 'untracked' for b in self.ledger['compiler_bindings']))

    def test_feature_policy_distinguishes_unknown_baseline_and_optional(self):
        sets = {s['key']: s for s in self.ledger['isa_sets']}
        self.assertEqual(sets['I286REAL']['status'], 'unclassified')
        self.assertIsNone(sets['I286REAL']['requires'])
        self.assertEqual(sets['I286REAL']['profiles'], [])
        self.assertEqual(sets['I86']['status'], 'declared')
        self.assertEqual(sets['I86']['requires'], [])
        self.assertEqual(sets['I86']['profiles'], ['v1', 'v2', 'v3'])
        self.assertEqual(set(sets['AVX_PCLMULQDQ']['requires']), {'avx', 'pclmulqdq', 'xsave', 'osxsave'})
        self.assertEqual(sets['AVX_PCLMULQDQ']['profiles'], [])
        self.assertEqual(sets['KNCV']['status'], 'excluded_policy')

    def test_public_binding_candidates_are_bidirectional_and_not_support_flags(self):
        forms = {f['id']: f for f in self.ledger['forms']}
        bindings = {b['id']: b for b in self.ledger['assembler_bindings']}
        for b in bindings.values():
            for id_ in b['candidate_forms']:
                self.assertIn(b['id'], forms[id_]['assembler_candidates'])
        for f in forms.values():
            for id_ in f['assembler_candidates']:
                self.assertIn(f['id'], bindings[id_]['candidate_forms'])
        # A V3 API identity can describe legacy encoding. Its name is not an encoding tag.
        lzcnt = bindings['Opcode::V3(V3Opcode::Lzcnt)']
        self.assertTrue(lzcnt['candidate_forms'])
        self.assertTrue(any(forms[id_]['encoding'] == 'legacy' for id_ in lzcnt['candidate_forms']))
        # Name candidates can also include another interpretation; they do not prove exposure.
        self.assertTrue(any(forms[id_]['policy_key'] == 'KNCV' for id_ in lzcnt['candidate_forms']))
        self.assertTrue(any(not b['candidate_forms'] for b in bindings.values()))
        self.assertTrue(self.ledger['dynamic_identity_families'])

    def test_compiler_recipe_links_keep_sequence_components_and_privilege_unknown(self):
        bindings = {b['name']: b for b in self.ledger['compiler_bindings']}
        forms = {f['id']: f for f in self.ledger['forms']}
        self.assertEqual(len(bindings), 45)
        self.assertEqual(sum(b['lowering']['kind'] == 'handwritten' for b in bindings.values()), 2)
        pair = bindings['x86_sti_hlt']
        self.assertEqual(pair['relation'], 'components')
        self.assertEqual({forms[id_]['mnemonic'] for id_ in pair['candidate_forms']}, {'sti', 'hlt'})
        self.assertTrue(any(forms[id_]['cpl'] is None for id_ in bindings['x86_lgdt']['candidate_forms']))
        for b in bindings.values():
            self.assertTrue(b['candidate_forms'])
            for id_ in b['candidate_forms']:
                self.assertIn(b['id'], forms[id_]['compiler_candidates'])

    def test_sources_cannot_be_substituted_under_recorded_hashes(self):
        with self.assertRaisesRegex(ValueError, 'recorded source'):
            coverage_ledger.build(self.root, self.data[:-1], self.adapters, generate.exclusion_reasons, generate.form_isa)
        adapters = copy.deepcopy(self.adapters)
        adapters['extensions'] = []
        with self.assertRaisesRegex(ValueError, 'recorded source'):
            coverage_ledger.build(self.root, self.data, adapters, generate.exclusion_reasons, generate.form_isa)

    def test_report_is_deterministic_and_summary_totals_match(self):
        self.assertEqual(json.loads(coverage_ledger.json_report(self.ledger)), self.ledger)
        report = coverage_ledger.markdown(self.ledger)
        self.assertEqual(report, coverage_ledger.markdown(self.ledger))
        summary = self.ledger['summary']
        self.assertEqual(summary['catalogue'] + summary['excluded'], len(self.data))
        self.assertEqual(summary['decoder_registered'] + summary['decoder_disabled'], summary['catalogue'])
        self.assertEqual(sum(s['imported'] for s in self.ledger['isa_sets']), len(self.data))
        self.assertEqual(sum(s['catalogue'] for s in self.ledger['isa_sets']), summary['catalogue'])
        self.assertIn('not complete operand support', report)


    def test_avx512_table_is_separate_and_reused_vex_records_are_identified(self):
        rows = self.ledger['avx512_forms']
        self.assertEqual(len(rows), 1901)
        self.assertEqual(sorted(r['table_index'] for r in rows), list(range(1901)))
        old = {r['id'] for r in self.ledger['forms']}
        self.assertEqual(sum(r['id'] in old for r in rows), 15)
        self.assertEqual(sum(r['encoding'] == 'evex' for r in rows), 1886)
        for row in rows:
            self.assertIn('avx512f', row['requires'])
            self.assertEqual('avx512vl' in row['requires'], row['isa_set'] in ('AVX512F_128', 'AVX512F_256'))
            self.assertEqual(row['validation'], 'untracked')


if __name__ == '__main__':
    unittest.main()

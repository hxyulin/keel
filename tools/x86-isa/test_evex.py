import copy
import json
import unittest
from pathlib import Path

import assembly_smoke
import evex

ROOT = Path(__file__).resolve().parent


class EvexTests(unittest.TestCase):
    def setUp(self):
        self.rows = evex.load(ROOT)

    def test_import_scope_and_smoke_cases(self):
        self.assertEqual(len(self.rows), 1899)
        self.assertEqual(sum(r['encoding'] == 'evex' for r in self.rows), 1884)
        self.assertEqual(len({r['mnemonic'] for r in self.rows}), 351)
        self.assertEqual(len(assembly_smoke.evex_sources(ROOT).splitlines()) - 1, 2423)
        self.assertEqual(json.loads((ROOT / 'evex-source.json').read_text())['commit'],
                         json.loads((ROOT / 'zydis-source.json').read_text())['commit'])

    def test_unknown_encoding_state_fails_closed(self):
        for location, fields in [('filters', {'unreviewed': '0'}),
                                 ('filters', {'evex_u': '0'}),
                                 ('filters', {'rex_w': '2'}),
                                 ('filters', {'vector_length': '1024'}),
                                 ('evex', {'tuple_type': 'unknown'}),
                                 ('evex', {'mask_mode': 'unknown'}),
                                 ('evex', {'mask_flags': ['unknown']}),
                                 ('evex', {'functionality': 'nf'}),
                                 ('evex', {'static_broadcast': '8to16'})]:
            rows = copy.deepcopy(self.rows)
            row = next(r for r in rows if r['encoding'] == 'evex')
            row.setdefault(location, {}).update(fields)
            with self.subTest(fields=fields), self.assertRaises(ValueError):
                evex.validate(rows)
        for field, value in [('operand_type', 'tmm'), ('encoding', 'nf')]:
            rows = copy.deepcopy(self.rows)
            rows[0]['operands'][0][field] = value
            with self.assertRaises(ValueError):
                evex.validate(rows)

    def test_other_families_and_duplicates_are_not_silently_imported(self):
        rows = copy.deepcopy(self.rows)
        rows[0]['meta_info']['isa_set'] = 'APX_F'
        with self.assertRaises(ValueError):
            evex.validate(rows)
        with self.assertRaises(ValueError):
            evex.validate(self.rows + self.rows[:1])

    def test_compiler_smoke_covers_every_declared_binding_once(self):
        source = assembly_smoke.compiler_source(ROOT)
        rows = [r for file in ['system-intrinsics.json', 'query-bindings.json']
                for r in json.loads((ROOT / file).read_text())['intrinsics']]
        self.assertEqual(source.count('func @'), len(rows))
        for row in rows:
            self.assertEqual(source.count(f'func @binding_{row["id"]}('), 1)
            self.assertEqual(source.count(f'intrinsic {row["name"]}('), 1)


if __name__ == '__main__':
    unittest.main()

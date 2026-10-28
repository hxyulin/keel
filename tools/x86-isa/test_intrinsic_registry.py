import copy
import json
import re
import unittest
from pathlib import Path

import intrinsic_registry
import query_bindings
import system_intrinsics

ROOT = Path(__file__).resolve().parent


class IntrinsicRegistryTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads((ROOT / 'intrinsic-registry.json').read_text())
        self.families = {family: json.loads((ROOT / file).read_text())['intrinsics']
                         for family, file in intrinsic_registry.FAMILIES.items()}

    def test_ids_and_inventory_match_independent_frozen_fixture(self):
        rows = intrinsic_registry.validate(self.schema)
        fixture = ROOT.parents[1] / 'crates/keel-ir/tests/fixtures/intrinsic-inventory.txt'
        expected = [line.split('|') for line in fixture.read_text().splitlines()
                    if not line.startswith('#')]
        self.assertEqual([r['variant'] for r in rows[:len(expected)]], [r[0] for r in expected])
        self.assertEqual([r['variant'] for r in rows[len(expected):]], ['LifetimeStart', 'LifetimeEnd'])
        self.assertEqual([r['variant'] for r in sorted(rows, key=lambda r: r['inventory'])[:len(expected)]],
                         [r[1] for r in expected])
        source = intrinsic_registry.generate(self.schema, self.families)
        self.assertIn('ALL: [Intrinsic; 821]', source)
        self.assertIn('X86Lgdt = 774,', source)
        self.assertIn('X86Swapgs = 818,', source)
        self.assertEqual(len(re.findall(r'^    [A-Z][A-Za-z0-9]* = [0-9]+,$', source, re.M)), 821)

    def test_invalid_global_ids_and_inventory_fail(self):
        for field, values in [('id', [-1, 1, True, '0']),
                              ('inventory', [-1, len(self.schema['intrinsics']), True, '0', self.schema['intrinsics'][1]['inventory']])]:
            for value in values:
                schema = copy.deepcopy(self.schema)
                schema['intrinsics'][0][field] = value
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    intrinsic_registry.generate(schema, self.families)

    def test_invalid_registry_shapes_and_variants_fail(self):
        for change in [lambda r: r.update(extra=0), lambda r: r.pop('id'),
                       lambda r: r.update(variant='bad-name'), lambda r: r.update(variant=[]),
                       lambda r: r.update(variant=self.schema['intrinsics'][1]['variant']),
                       lambda r: r.update(metadata='missing'), lambda r: r.update(doc='')]:
            schema = copy.deepcopy(self.schema)
            change(schema['intrinsics'][0])
            with self.assertRaises(ValueError):
                intrinsic_registry.generate(schema, self.families)
        for schema in [{}, {'intrinsics': []}, dict(self.schema, first_id=0)]:
            with self.assertRaises(ValueError):
                intrinsic_registry.generate(schema, self.families)

    def test_family_membership_and_ids_are_checked_in_both_directions(self):
        for change in [lambda fs: fs['x86_system'].pop(),
                       lambda fs: fs['x86_system'][0].update(id=0),
                       lambda fs: fs['x86_system'][0].update(id=True),
                       lambda fs: fs['x86_system'].append(dict(fs['x86_system'][0], variant='X86Missing')),
                       lambda fs: fs['x86_query'].append(fs['x86_system'][0]),
                       lambda fs: fs['x86_query'][0].update(name=fs['x86_system'][0]['name']),
                       lambda fs: fs.pop('x86_query')]:
            families = copy.deepcopy(self.families)
            change(families)
            with self.assertRaises(ValueError):
                intrinsic_registry.generate(self.schema, families)
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'][774]['metadata'] = 'x86_query'
        with self.assertRaisesRegex(ValueError, 'owner'):
            intrinsic_registry.generate(schema, self.families)
        schema['intrinsics'][774]['metadata'] = 'handwritten'
        with self.assertRaisesRegex(ValueError, 'membership'):
            intrinsic_registry.generate(schema, self.families)

    def test_family_docs_have_one_owner(self):
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'][774]['doc'] = 'Duplicated contract'
        with self.assertRaisesRegex(ValueError, 'documentation'):
            intrinsic_registry.generate(schema, self.families)
        source = intrinsic_registry.generate(self.schema, self.families)
        self.assertIn(json.dumps(self.families['x86_system'][0]['doc']), source)
        for row in self.schema['intrinsics']:
            if 'doc' in row:
                self.assertIn(json.dumps(row['doc']), source)

    def test_interleaved_families_do_not_require_a_contiguous_tail(self):
        schema, families = copy.deepcopy(self.schema), copy.deepcopy(self.families)
        start = len(schema['intrinsics'])
        for offset, family, template in [(0, 'handwritten', None),
                                          (1, 'x86_system', self.families['x86_system'][-1]),
                                          (2, 'x86_query', self.families['x86_query'][-1]),
                                          (3, 'x86_system', self.families['x86_system'][-1])]:
            id_ = start + offset
            variant = f'X86Example{id_}'
            schema['intrinsics'].append(dict(id=id_, variant=variant, inventory=id_, metadata=family))
            if template:
                families[family].append(dict(template, id=id_, variant=variant, name=f'x86_example{id_}'))
        system_intrinsics.validate({'intrinsics': families['x86_system']})
        query_bindings.validate({'intrinsics': families['x86_query']}, ROOT)
        source = intrinsic_registry.generate(schema, families)
        self.assertIn(f'ALL: [Intrinsic; {start + 4}]', source)
        for id_ in range(start, start + 4):
            self.assertIn(f'X86Example{id_} = {id_},', source)
        self.assertIn(f'Self::X86Example{start + 3} => IntrinsicInfo',
                      system_intrinsics.generate({'intrinsics': families['x86_system']}))


if __name__ == '__main__':
    unittest.main()

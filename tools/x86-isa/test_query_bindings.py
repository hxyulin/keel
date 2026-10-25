import copy
import json
import unittest

import generate
import query_bindings


class QueryBindingTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads((generate.ROOT / 'query-bindings.json').read_text())

    def rejects(self, index, mutate, message):
        schema = copy.deepcopy(self.schema)
        mutate(schema['intrinsics'][index])
        with self.assertRaisesRegex(ValueError, message):
            query_bindings.generate(schema, generate.ROOT)

    def test_complete_families_and_const_id_checks(self):
        rows = query_bindings.validate(self.schema, generate.ROOT)
        self.assertEqual([r['id'] for r in rows], list(range(687, 729)))
        self.assertEqual({k: sum(r['binding']['kind'] == k for r in rows)
                          for k in query_bindings.GROUPS},
                         {'descriptor_query': 14, 'selector_query': 16, 'port_io': 12})
        source = query_bindings.generate(self.schema, generate.ROOT)
        self.assertEqual(source.count('assert!(Intrinsic::'), 42)
        for method, _, _ in query_bindings.GROUPS.values():
            self.assertIn(f'pub fn {method}(', source)

    def test_ids_and_duplicate_identities_fail(self):
        self.rejects(0, lambda r: r.update(id=-1), 'IDs')
        self.rejects(1, lambda r: r.update(id=687), 'IDs')
        self.rejects(1, lambda r: r.update(variant='X86Sgdt'), 'duplicate')
        self.rejects(1, lambda r: r.update(name='x86_sgdt'), 'duplicate')

    def test_effects_and_targets_cannot_weaken(self):
        for fields in [dict(pure=True), dict(pure=0), dict(memory='None'), dict(target='all')]:
            self.rejects(0, lambda r: r.update(fields), 'barrier policy')
        self.rejects(0, lambda r: r.update(doc=''), 'contract')

    def test_shapes_require_exact_signatures(self):
        for index in [0, 3, 14, 15, 26, 30, 33, 36]:
            self.rejects(index, lambda r: r.update(params=[]), 'signature')
            self.rejects(index, lambda r: r.update(returns=['Ptr']), 'signature')

    def test_descriptor_width_and_footprint_are_not_interchangeable(self):
        for fields in [dict(bits=64), dict(write_bytes=16), dict(write_bytes=True), dict(mnemonic='rsm')]:
            self.rejects(0, lambda r: r['binding'].update(fields), 'descriptor')
        self.rejects(3, lambda r: r['binding'].update(bits=8), 'descriptor')

    def test_selector_and_port_domains_are_closed(self):
        self.rejects(14, lambda r: r['binding'].update(bits=8), 'selector')
        self.rejects(14, lambda r: r['binding'].update(memory=1), 'selector')
        self.rejects(26, lambda r: r['binding'].update(bits=16), 'selector')
        self.rejects(30, lambda r: r['binding'].update(bits=64), 'port')
        self.rejects(30, lambda r: r['binding'].update(immediate=1), 'port')
        self.rejects(30, lambda r: r['binding'].update(output='false'), 'port')

    def test_unknown_schema_fields_and_kinds_fail(self):
        self.rejects(0, lambda r: r.update(extra=True), 'fields')
        self.rejects(0, lambda r: r['binding'].update(argument=2), 'fields')
        for kind in ['arbitrary_rust', None, []]:
            self.rejects(0, lambda r: r['binding'].update(kind=kind), 'kind')



if __name__ == '__main__':
    unittest.main()

import copy
import json
import unittest
from pathlib import Path

import system_intrinsics


class SystemIntrinsicTests(unittest.TestCase):
    def setUp(self):
        self.schema = json.loads(Path(__file__).with_name('system-intrinsics.json').read_text())

    def test_current_scope_and_generation(self):
        rows = system_intrinsics.validate(self.schema)
        self.assertEqual([r['id'] for r in rows], list(range(774, 819)))
        self.assertTrue(all(not r['pure'] and r['memory'] == 'Barrier' for r in rows))
        source = system_intrinsics.generate(self.schema)
        self.assertNotIn('pub enum Intrinsic', source)
        self.assertNotIn('define_intrinsic_inventory', source)
        self.assertIn('(x86_64) =>', source)
        self.assertIn('(Barrier) =>', source)

    def test_ids_are_explicit_nonnegative_and_ordered(self):
        for value in [-1, 775, True, '774']:
            schema = copy.deepcopy(self.schema)
            schema['intrinsics'][0]['id'] = value
            with self.assertRaisesRegex(ValueError, 'IDs'):
                system_intrinsics.generate(schema)
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'].reverse()
        with self.assertRaisesRegex(ValueError, 'IDs'):
            system_intrinsics.generate(schema)
        schema['intrinsics'][1]['id'] = 776
        schema['intrinsics'][2]['id'] = 775
        with self.assertRaisesRegex(ValueError, 'IDs'):
            system_intrinsics.generate(schema)

    def test_duplicate_and_invalid_names(self):
        for key in ['variant', 'name']:
            for value in [self.schema['intrinsics'][1][key], '', 'bad-name', None]:
                schema = copy.deepcopy(self.schema)
                schema['intrinsics'][0][key] = value
                with self.assertRaisesRegex(ValueError, 'invalid or duplicate'):
                    system_intrinsics.generate(schema)

    def test_signature_effect_and_target_domains(self):
        for key, value in [('params', ['Unknown']), ('returns', 'I64'),
                           ('pure', 'false'), ('memory', 'Relaxed'),
                           ('target', 'aarch64'), ('doc', '')]:
            schema = copy.deepcopy(self.schema)
            schema['intrinsics'][0][key] = value
            with self.assertRaises(ValueError):
                system_intrinsics.generate(schema)
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'][0]['pure'] = True
        with self.assertRaisesRegex(ValueError, 'pure'):
            system_intrinsics.generate(schema)

    def test_effect_classification_comes_from_the_declaration(self):
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'][0]['memory'] = 'Read'
        source = system_intrinsics.generate(schema)
        self.assertIn('(Read) => {\n        $crate::Intrinsic::X86Lgdt\n', source)
        self.assertIn('x86_system_intrinsics!(Read) => crate::MemoryEffects::Read', source)
        schema['intrinsics'][0]['memory'] = 'None'
        schema['intrinsics'][0]['pure'] = True
        source = system_intrinsics.generate(schema)
        self.assertIn('x86_system_intrinsics!(None) => crate::MemoryEffects::None', source)

    def test_missing_unknown_and_empty_schema_fields(self):
        for level in ['root', 'row']:
            for operation in ['delete', 'add']:
                schema = copy.deepcopy(self.schema)
                target = schema if level == 'root' else schema['intrinsics'][0]
                if operation == 'delete':
                    del target[next(iter(target))]
                else:
                    target['typo'] = 1
                with self.assertRaisesRegex(ValueError, 'fields'):
                    system_intrinsics.generate(schema)
        schema = copy.deepcopy(self.schema)
        schema['intrinsics'] = []
        with self.assertRaisesRegex(ValueError, 'empty'):
            system_intrinsics.generate(schema)

    def test_appending_updates_metadata_and_all_classifications(self):
        schema = copy.deepcopy(self.schema)
        row = dict(schema['intrinsics'][-1], id=819, variant='X86Example', name='x86_example')
        schema['intrinsics'].append(row)
        source = system_intrinsics.generate(schema)
        self.assertNotIn('ALL:', source)
        self.assertIn('Self::X86Example => IntrinsicInfo', source)
        # Availability and effects follow the family row; the registry owns inventory.
        self.assertEqual(source.count('$crate::Intrinsic::X86Example'), 3)


if __name__ == '__main__':
    unittest.main()

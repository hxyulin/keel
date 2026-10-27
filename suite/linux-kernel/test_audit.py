"""Checks that capture cannot count legacy/host inputs as 64-bit coverage."""
import importlib.util
from pathlib import Path
import tempfile
import types
import unittest
import json
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('kernel_audit', Path(__file__).with_name('audit.py'))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTests(unittest.TestCase):
    def test_mode_boundaries(self):
        for path, command, source in [
            ('arch/x86/realmode/rm/wakeup.s', 'gcc -m64', ''),
            ('arch/x86/boot/main.i', 'gcc -m16', ''),
            ('other.i', 'gcc -m32 -c foo.c', ''),
            ('mixed.s', 'gcc -m64', '.text\n.code32\n'),
            ('mixed.s', 'gcc -m64', '.code16gcc\n'),
            ('scripts/mod/empty.i', 'gcc -m64 -D__KERNEL__', 'int x;'),
        ]:
            self.assertIsNotNone(audit.external_reason(path, command, source))
        self.assertIsNone(audit.external_reason('kernel/fork.i', 'gcc -m64', 'int x;'))
        self.assertIsNone(audit.external_reason('kernel/entry.s', 'gcc -m64', '.code64\n'))
        self.assertIsNone(audit.external_reason('arch/x86/realmode/init.i', 'gcc -m64', 'int x;'))
        self.assertIsNone(audit.external_reason('arch/x86/realmode/rmpiggy.s', 'gcc -m64', '.incbin "realmode.bin"'))

    def test_capture_requires_kernel_command_and_hashes_inputs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / '.config').write_text('CONFIG_64BIT=y\n')
            (root / 'kernel.i').write_text('int x;\n')
            (root / '.kernel.o.cmd').write_text('savedcmd_kernel.o := gcc -m64 -D__KERNEL__ -c kernel.c\n')
            (root / 'host.i').write_text('int host;\n')
            (root / '.host.o.cmd').write_text('savedcmd_host.o := gcc -c host.c\n')
            (root / 'mixed.s').write_text('.code32\n')
            (root / '.mixed.o.cmd').write_text('savedcmd_mixed.o := gcc -m64 -D__KERNEL__ -c mixed.S\n')
            out = root / 'manifest.json'
            audit.capture(types.SimpleNamespace(build=root, source=root / 'src', out=out))
            data = json.loads(out.read_text())
            self.assertEqual(data['counts'], {'kernel64/c': 1, 'external/asm': 1})
            row = next(x for x in data['inputs'] if x['path'] == 'kernel.i')
            self.assertEqual(row['sha256'], audit.sha(root / 'kernel.i'))

    def test_first_error_grouping(self):
        self.assertEqual(audit.category('error: stack-pointer asm outputs require a stack-call contract'),
                         'asm-stack-call-contract')
        self.assertEqual(audit.category('error: asm goto is not supported'), 'asm-goto')
        self.assertEqual(audit.category('unsupported ELF directive `.pushsection`'),
                         'assembler-directive:.pushsection')
        self.assertEqual(audit.category('keel: /tmp/build/kernel.s: line 17: unsupported AT&T instruction `rdtscp`'),
                         'instruction-binding:rdtscp')

    def test_probe_records_separate_explicit_feature_policies(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / '.config').write_text('CONFIG_64BIT=y\n')
            (root / 'compiler').write_text('compiler witness')
            (root / 'kernel.i').write_text('int x;\n')
            (root / 'kernel.s').write_text('rdtscp\n')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({
                'config_sha256': audit.sha(root / '.config'),
                'inputs': [{'kind': kind, 'path': path, 'scope': 'kernel64',
                            'sha256': audit.sha(root / path)}
                           for kind, path in [('c', 'kernel.i'), ('asm', 'kernel.s')]],
            }))
            for name, c_features, asm_features in [('pinned', None, None), ('explicit', '+popcnt', '+rdtscp')]:
                args = types.SimpleNamespace(build=root, manifest=manifest, keel=root / 'compiler',
                    out=root / name, timeout=60, c_target_features=c_features,
                    asm_target_features=asm_features)
                with patch.object(audit.subprocess, 'run', return_value=types.SimpleNamespace(returncode=0, stderr=b'')) as run:
                    audit.probe(args)
                for call, features in zip(run.call_args_list, [c_features, asm_features]):
                    command = call.args[0]
                    self.assertEqual('--target-features' in command, features is not None)
                    if features is not None:
                        self.assertEqual(command[command.index('--target-features') + 1], features)
                result = json.loads((args.out / 'results.json').read_text())
                self.assertEqual(result['target_policy'], {
                    'c': {'cpu_level': 'v1', 'target_features': c_features},
                    'asm': {'cpu_level': 'v3', 'target_features': asm_features},
                })
                self.assertEqual(result['first_error_feature_gates'], [])

    def test_probe_records_instruction_requirements_without_granting_features(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / '.config').write_text('CONFIG_64BIT=y\n')
            (root / 'compiler').write_text('compiler witness')
            inputs = [('c', 'find.i', 'popcnt', 'popcnt'),
                      ('c', 'weight.i', 'popcnt', 'popcnt'),
                      ('c', 'random.i', 'rdrand', 'rdrand'),
                      ('asm', 'vector.s', 'vaddps', 'avx512f,avx512vl')]
            for _, path, _, _ in inputs:
                (root / path).write_text('unchanged witness\n')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({
                'config_sha256': audit.sha(root / '.config'),
                'inputs': [{'kind': kind, 'path': path, 'scope': 'kernel64',
                            'sha256': audit.sha(root / path)}
                           for kind, path, _, _ in inputs],
            }))
            args = types.SimpleNamespace(build=root, manifest=manifest, keel=root / 'compiler',
                out=root / 'probe', timeout=60, c_target_features=None,
                asm_target_features='+avx512f')
            failures = [types.SimpleNamespace(returncode=1, stderr=(
                f'keel: {root / path}: line 1: {instruction} requires x86 target features: {features}'
            ).encode()) for _, path, instruction, features in inputs]
            with patch.object(audit.subprocess, 'run', side_effect=failures) as run:
                audit.probe(args)
            result = json.loads((args.out / 'results.json').read_text())
            self.assertEqual(result['first_error_feature_gates'], [
                {'kind': 'asm', 'instruction': 'vaddps',
                 'required_features': ['avx512f', 'avx512vl'], 'rejected_inputs': 1},
                {'kind': 'c', 'instruction': 'popcnt',
                 'required_features': ['popcnt'], 'rejected_inputs': 2},
                {'kind': 'c', 'instruction': 'rdrand',
                 'required_features': ['rdrand'], 'rejected_inputs': 1},
            ])
            for row, (_, _, instruction, features) in zip(result['results'], inputs):
                self.assertEqual(row['category'], 'feature-policy')
                self.assertEqual(row['feature_gate'], {
                    'instruction': instruction, 'required_features': features.split(',')})
            # Reporting a requirement never changes the caller's explicit policy.
            for call in run.call_args_list[:3]:
                self.assertNotIn('--target-features', call.args[0])
            command = run.call_args_list[3].args[0]
            self.assertEqual(command[command.index('--target-features') + 1], '+avx512f')
            self.assertIsNone(audit.feature_gate('unknown instruction `rdrand`'))


if __name__ == '__main__':
    unittest.main()

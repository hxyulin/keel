"""Checks that capture cannot count legacy/host inputs as 64-bit coverage."""
import importlib.util
from pathlib import Path
import tempfile
import types
import unittest
import json

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
        self.assertEqual(audit.category('error: asm goto is not supported'), 'asm-goto')
        self.assertEqual(audit.category('unsupported ELF directive `.pushsection`'),
                         'assembler-directive:.pushsection')
        self.assertEqual(audit.category('keel: /tmp/build/kernel.s: line 17: unsupported AT&T instruction `rdtscp`'),
                         'instruction-binding:rdtscp')


if __name__ == '__main__':
    unittest.main()

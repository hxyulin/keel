"""Captured-input measurements must not double-count generated assembly."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('compile_bench', Path(__file__).with_name('compile-bench.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class CompileBenchTests(unittest.TestCase):
    def test_generated_assembly_is_not_a_second_compilation(self):
        manifest = {'inputs': [
            {'scope': 'kernel64', 'kind': 'c', 'path': 'kernel/a.i'},
            {'scope': 'kernel64', 'kind': 'asm', 'path': 'kernel/a.s'},
            {'scope': 'kernel64', 'kind': 'asm', 'path': 'entry.s'},
            {'scope': 'external', 'kind': 'c', 'path': 'boot.i'}]}
        self.assertEqual([row['path'] for row in bench.inputs(manifest)], ['kernel/a.i', 'entry.s'])

    def test_replay_preserves_abi_and_replaces_source_output_and_preset(self):
        row = {'path': 'kernel/a.i', 'command': 'gcc -Wp,-MMD,kernel/a.d -I<source>/include -mcmodel=kernel -mno-red-zone -fno-strict-aliasing -Os -save-temps=obj -c -o kernel/a.o <source>/kernel/a.c ; ./tools/objtool/objtool kernel/a.o'}
        command = bench.gcc_command(row, Path('/source'), Path('/build'), Path('/output/a.o'), '-O2')
        self.assertEqual(command[-5:], ['-O2', '-c', '/build/kernel/a.i', '-o', '/output/a.o'])
        for option in ['-mcmodel=kernel', '-mno-red-zone', '-fno-strict-aliasing', '-I/source/include']:
            self.assertIn(option, command)
        for excluded in ['-Os', '-save-temps=obj', '/source/kernel/a.c', 'kernel/a.o', './tools/objtool/objtool']:
            self.assertNotIn(excluded, command)


if __name__ == '__main__':
    unittest.main()

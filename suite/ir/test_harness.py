"""Output-preservation checks in harness.Importer, against a scripted importer."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness

# Accepts any input except one containing "reject" or "leak"; "leak" also
# writes the destination before failing, which a real importer must never do.
FAKE = '''#!{python}
import sys
args = sys.argv[1:]
dest = args[args.index('-o') + 1] if '-o' in args else '-'
source = args[args.index('-o') - 1] if '-o' in args else args[-1]
data = sys.stdin.buffer.read() if source == '-' else open(source, 'rb').read()
if b'leak' in data and dest != '-':
    open(dest, 'wb').write(b'partial')
if b'reject' in data or b'leak' in data:
    sys.stderr.write('llvm-import: unsupported input\\n')
    raise SystemExit(1)
text = b'func @f() {{}}\\n'
if dest == '-':
    sys.stdout.buffer.write(text)
else:
    open(dest, 'wb').write(text)
'''


class ImporterProtocol(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        fake = self.root / 'fake-import'
        fake.write_text(FAKE.format(python=sys.executable))
        fake.chmod(0o755)
        runner = harness.Runner(self.root, 30)
        self.importer = harness.Importer(runner, fake, 'libLLVM', 'keel', self.root)

    def tearDown(self):
        self.tmp.cleanup()

    def source(self, text):
        path = self.root / 'input.ll'
        path.write_text(text)
        return path

    def test_accept_returns_text_and_matches_stdout(self):
        text = self.importer.accept(self.source('ok'), self.root / 'out.keel')
        self.assertEqual(text, 'func @f() {}\n')

    def test_clean_rejection_returns_diagnostic(self):
        diagnostic = self.importer.reject(self.source('reject'))
        self.assertEqual(diagnostic, 'llvm-import: unsupported input\n')

    def test_partial_output_fails_rejection(self):
        with self.assertRaisesRegex(harness.Failed, 'changed the destination'):
            self.importer.reject(self.source('leak'))

    def test_accepted_input_fails_rejection(self):
        with self.assertRaisesRegex(harness.Failed, 'expected a rejection'):
            self.importer.reject(self.source('ok'))

    def test_rejected_input_fails_acceptance(self):
        with self.assertRaisesRegex(harness.Failed, 'import failed'):
            self.importer.accept(self.source('reject'), self.root / 'out.keel')


if __name__ == '__main__':
    unittest.main()

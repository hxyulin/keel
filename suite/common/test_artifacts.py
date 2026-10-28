import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('keel_artifacts',Path(__file__).with_name('artifacts.py'))
artifacts=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(artifacts)

class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)/'repo';self.root.mkdir()
        self.patch=patch.multiple(artifacts,ROOT=self.root,INDEX=self.root/'artifacts.lock.json');self.patch.start();self.addCleanup(self.patch.stop)
        self.env=patch.dict(os.environ,{'KEEL_ARTIFACT_STORE':str(Path(self.temp.name)/'store')});self.env.start();self.addCleanup(self.env.stop)
    def command(self,*args):
        with patch('sys.argv',['artifacts.py',*args]),contextlib.redirect_stdout(io.StringIO()),contextlib.redirect_stderr(io.StringIO()):artifacts.main()
    def add(self,name='capture.gz'):
        p=self.root/name;p.write_bytes(b'exact frozen bytes');self.command('add',name);return p
    def test_deduplicated_copy_restore_and_independent_store(self):
        a=self.add();b=self.root/'other.gz';b.write_bytes(a.read_bytes());self.command('add','other.gz')
        rows=artifacts.read_index()['artifacts'];self.assertEqual(rows['capture.gz'],rows['other.gz'])
        obj=artifacts.object_path(rows['capture.gz']['sha256']);a.write_bytes(b'changed working copy');self.assertEqual(obj.read_bytes(),b'exact frozen bytes')
        a.unlink();self.command('restore','capture.gz');self.assertEqual(a.read_bytes(),b'exact frozen bytes');self.command('verify')
    def test_refuses_overwrite_and_corrupt_store(self):
        p=self.add();p.write_bytes(b'wrong');
        with self.assertRaises(ValueError):self.command('restore','capture.gz')
        self.assertEqual(p.read_bytes(),b'wrong');p.unlink();row=artifacts.read_index()['artifacts']['capture.gz'];artifacts.object_path(row['sha256']).write_bytes(b'bad')
        with self.assertRaises(SystemExit):self.command('restore','capture.gz')
        self.assertFalse(p.exists())
    def test_missing_verify_is_not_success(self):
        p=self.add();p.unlink()
        with self.assertRaises(SystemExit):self.command('verify')
    def test_paths_cannot_escape_or_follow_symlinks(self):
        with self.assertRaises(ValueError):artifacts.destination('../escape')
        (self.root/'link').symlink_to(Path(self.temp.name),target_is_directory=True)
        with self.assertRaises(ValueError):artifacts.destination('link/file')

if __name__=='__main__':unittest.main()

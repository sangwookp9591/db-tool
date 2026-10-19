import sys
sys.dont_write_bytecode = True
import importlib.util
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release_manifest', pathlib.Path(__file__).with_name('release-manifest.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ManifestTests(unittest.TestCase):
    def test_platforms_share_final_universal_archive_and_preserve_notes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            notes = root / 'notes.md'
            notes.write_text('# 릴리스\n- 안전한 업데이트\n')
            for name in ['AxDB_0.2.0_universal.app.tar.gz', 'AxDB_0.2.0_x64-setup.exe']:
                (root / name).write_bytes(b'artifact')
                (root / (name + '.sig')).write_text('signature\n')
            result = module.manifest('0.2.0', root, notes)
            self.assertEqual(set(result['platforms']), {'darwin-aarch64', 'darwin-x86_64', 'windows-x86_64'})
            self.assertEqual(result['platforms']['darwin-aarch64'], result['platforms']['darwin-x86_64'])
            self.assertEqual(result['notes'], notes.read_text())
            self.assertIn('/v0.2.0/', result['platforms']['windows-x86_64']['url'])
            (root / 'AxDB_0.2.0_x64-setup.exe.sig').write_text('')
            with self.assertRaises(ValueError):
                module.manifest('0.2.0', root, notes)
            with self.assertRaises(ValueError):
                module.manifest('../secret', root, notes)


if __name__ == '__main__':
    unittest.main()

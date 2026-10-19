"""Hermetic release pipeline test. No real keys, builds, Apple calls or GitHub writes."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parent.parent


class ReleaseTests(unittest.TestCase):
    def run_release(self, publish):
        with tempfile.TemporaryDirectory(prefix='axdb-release-test-') as directory:
            root = Path(directory)
            for name in ['scripts', 'src-tauri', 'ui', 'docs/release-notes', 'bin', 'home/.tauri']:
                (root/name).mkdir(parents=True)
            for name in ['scripts/release.sh', 'scripts/release-manifest.py', 'docs/release-notes/v0.2.0.md']:
                shutil.copyfile(SOURCE/name, root/name)
            (root/'Cargo.toml').write_text('[workspace.package]\nversion = "0.2.0"\n')
            for name in ['src-tauri/tauri.conf.json', 'ui/package.json']:
                (root/name).write_text('{"version":"0.2.0"}')
            (root/'home/.tauri/axdb-updater.key').write_text('FAKE-PRIVATE-KEY')
            fake = root/'bin/fake'
            fake.write_text(f'#!{sys.executable}\n' + '''import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
root = pathlib.Path(os.environ['AXDB_RELEASE_TEST_ROOT'])
with (root/'calls.jsonl').open('a') as f: f.write(json.dumps([name, args])+'\\n')
if name == 'security': print('FAKE-PASSWORD-DO-NOT-LOG')
if name == 'pnpm' and 'build' in args:
    assert os.environ['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] == 'FAKE-PASSWORD-DO-NOT-LOG'
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    if 'universal-apple-darwin' in args:
        bundle = target/'universal-apple-darwin/release/bundle'
        (bundle/'macos/AxDB.app/Contents').mkdir(parents=True)
        (bundle/'macos/AxDB.app/Contents/file').write_text('app')
        (bundle/'dmg').mkdir()
        (bundle/'dmg/AxDB_0.2.0_universal.dmg').write_text('dmg')
    else:
        nsis = target/'x86_64-pc-windows-msvc/release/bundle/nsis'
        nsis.mkdir(parents=True)
        (nsis/'AxDB_0.2.0_x64-setup.exe').write_text('installer')
        (nsis/'AxDB_0.2.0_x64-setup.exe.sig').write_text('windows-signature')
if name == 'pnpm' and 'signer' in args: pathlib.Path(args[-1]+'.sig').write_text('mac-signature')
if name == 'ditto': pathlib.Path(args[-1]).write_text('zip')
if name == 'xcrun' and args[0] == 'notarytool': print('{"status":"Accepted"}')
if name == 'gh': assert 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD' not in os.environ
''')
            fake.chmod(0o755)
            for name in ['security', 'pnpm', 'codesign', 'ditto', 'xcrun', 'gh', 'llvm-dlltool']:
                (root/'bin'/name).symlink_to(fake)
            result = subprocess.run(['bash', '-x', str(root/'scripts/release.sh'), '0.2.0'] + (['--publish'] if publish else []),
                capture_output=True, text=True, env={**os.environ, 'HOME': str(root/'home'), 'PATH': str(root/'bin')+os.pathsep+os.environ['PATH'], 'CARGO_TARGET_DIR': str(root/'target-u1'), 'APPLE_SIGNING_IDENTITY': 'Developer ID Application: test', 'AXDB_RELEASE_TEST_ROOT': str(root)})
            self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
            self.assertNotIn('FAKE-PASSWORD', result.stdout+result.stderr)
            self.assertNotIn('FAKE-PRIVATE-KEY', result.stdout+result.stderr)
            calls = [json.loads(line) for line in (root/'calls.jsonl').read_text().splitlines()]
            uploads = [args for name, args in calls if name == 'gh']
            self.assertEqual(len(uploads), int(publish))
            if publish:
                self.assertEqual(uploads[0][:3], ['release', 'create', 'v0.2.0'])
            manifest = json.loads((root/'dist/release/0.2.0/latest.json').read_text())
            self.assertEqual(manifest['platforms']['darwin-aarch64']['signature'], 'mac-signature')
            self.assertEqual(manifest['platforms']['windows-x86_64']['signature'], 'windows-signature')
            order = [(name, args[0] if args else '') for name, args in calls]
            self.assertLess(order.index(('xcrun', 'stapler')), next(i for i, (name,args) in enumerate(calls) if name == 'pnpm' and 'signer' in args))

    def test_default_does_not_publish_or_leak_secrets(self):
        self.run_release(False)

    def test_explicit_publish_happens_after_both_signed_artifacts(self):
        self.run_release(True)


if __name__ == '__main__':
    unittest.main()

"""Fault-injection check: no apps, DB calls, resource sampling or user settings."""
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch


class SettingsRecovery(unittest.TestCase):
    def test_probe_failure_preserves_original_settings(self):
        with tempfile.TemporaryDirectory(prefix='axdb-compare-test-') as temporary:
            root = Path(temporary)
            work = root / 'work'; work.mkdir()
            home = root / 'home'
            settings = home / 'Library/Application Support/tabularis'; settings.mkdir(parents=True)
            (settings / 'original.json').write_text('preserved settings')
            executable = root / 'fake-app'; executable.write_text('not executed')
            script = root / 'compare_idle.py'
            source = Path(__file__).with_name('compare_idle.py').read_text()
            self.assertEqual(source.count("pathlib.Path('/tmp/axdb-compare')"), 1)
            script.write_text(source.replace("pathlib.Path('/tmp/axdb-compare')", f'pathlib.Path({str(work)!r})'))
            gate = types.ModuleType('compare_gate')
            gate.wait_gate = lambda *a, **kw: {'passed': True, 'waited_seconds': 0}
            gate.build_processes = lambda: []
            gate.require_test_profiles = lambda *a: None
            with patch.dict('sys.modules', {'compare_gate': gate}), patch('sys.argv', [str(script), str(executable), str(executable), '--p1-after', str(executable)]), patch('pathlib.Path.home', return_value=home), patch('os.getloadavg', return_value=(1, 1, 1)), patch('subprocess.check_output', return_value='fixture ps\n'), patch('subprocess.run', side_effect=subprocess.CalledProcessError(23, 'fixture-probe')):
                with self.assertRaises(subprocess.CalledProcessError):
                    runpy.run_path(str(script), run_name='__main__')
            self.assertEqual((work / 'tabularis-preserved-p1/original.json').read_text(), 'preserved settings')
            self.assertTrue(settings.is_dir())
            rows = json.loads((root / 'results-compare-idle-p1.json').read_text())
            self.assertFalse(rows[0]['measured'])
            self.assertTrue(rows[0]['backup_preserved'])


if __name__ == '__main__':
    unittest.main()

"""Failure injection: settings restoration and refusal of a busy-machine sample."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('compare_generated', Path(__file__).with_name('compare_generated.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class GeneratedRecovery(unittest.TestCase):
    def test_preparation_failure_restores_settings_and_records_every_sample(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            work = root / 'work'
            tab = root / 'tabularis'
            tab.mkdir()
            (tab / 'original').write_bytes(b'original-settings')
            template = work / 'tabularis-connected'
            template.mkdir(parents=True)
            (template / 'config.json').write_text('{}')
            ax = work / 'axdb-connected'
            ax.mkdir()
            (ax / 'profiles.json').write_text('[{"save_secret": true}]')
            exe = root / 'app'
            exe.write_bytes(b'fixture')
            with patch.object(m, 'ROOT', root), patch.object(m, 'WORK', work), patch.object(m, 'TAB', tab), patch.object(m, 'require_test_profiles'), patch.object(m, 'wait_gate', return_value={'passed': True}), patch.object(m, 'build_processes', return_value=[]), patch.object(m, 'processes', return_value={}), patch.object(m.os, 'getloadavg', return_value=(1, 1, 1)), patch.object(m.subprocess, 'check_output', return_value='fixture-head'), patch.object(m.subprocess, 'Popen', side_effect=RuntimeError('launch failed')), patch.object(m.time, 'sleep'), patch('sys.argv', ['compare_generated', str(exe), str(exe), '--preflight']):
                m.main()
            self.assertEqual((tab / 'original').read_bytes(), b'original-settings')
            self.assertFalse((work / 'tabularis-preserved-w9').exists())
            rows = json.loads((root / 'results-compare-gen-preflight.json').read_text())
            self.assertEqual(len(rows), 4)
            self.assertTrue(all(not row['valid'] and not row['measured'] and row['error'] == 'launch failed' for row in rows))

    def test_failed_gate_never_launches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            exe = root / 'app'
            exe.write_bytes(b'fixture')
            with patch.object(m, 'ROOT', root), patch.object(m, 'WORK', root), patch.object(m, 'require_test_profiles'), patch.object(m, 'wait_gate', return_value={'passed': False}), patch.object(m, 'build_processes', return_value=[]), patch.object(m, 'processes', return_value={}), patch.object(m.os, 'getloadavg', return_value=(9, 9, 9)), patch.object(m.subprocess, 'check_output', return_value='fixture-head'), patch.object(m.subprocess, 'Popen') as launch, patch('sys.argv', ['compare_generated', str(exe), str(exe), '--preflight']):
                m.main()
            launch.assert_not_called()
            rows = json.loads((root / 'results-compare-gen-preflight.json').read_text())
            self.assertEqual(len(rows), 4)
            self.assertTrue(all(row['reason'] == 'load_or_build_gate_failed' for row in rows))


if __name__ == '__main__':
    unittest.main()

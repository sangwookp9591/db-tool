#!/usr/bin/env python3
"""Run the opt-in native QA harness using a fresh config directory and local fixtures."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target-u4b/debug/axdb')
parser.add_argument('--output', default='docs/verification/qa-2026-10-21')
parser.add_argument('--keep-open', action='store_true')
args = parser.parse_args()
binary = Path(args.binary).resolve()
output = Path(args.output).resolve()
output.mkdir(parents=True, exist_ok=True)
config = Path(tempfile.mkdtemp(prefix='axdb-qa-native-'))
env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_DRIVER_DIR': str(binary.parent),
       'AXDB_BENCH': '1', 'AXDB_BENCH_TARGET': 'qa'}
with (output / 'native-app.log').open('w') as log:
    app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log, start_new_session=True)
(output / 'native-run.json').write_text(json.dumps({'pid': app.pid, 'config': str(config)}, indent=2))
print(f'QA app PID {app.pid}; isolated config {config}', flush=True)
result = {'complete': False, 'error': 'Native QA timed out'}
last_stage = ''
try:
    until = time.monotonic() + 180
    while time.monotonic() < until:
        path = config / 'perf.json'
        if path.exists():
            try:
                report = json.loads(path.read_text())
            except (OSError, json.JSONDecodeError):
                time.sleep(.2)
                continue
            if report.get('stage') != last_stage:
                last_stage = report.get('stage')
                print(last_stage, flush=True)
            if report.get('complete') or report.get('error'):
                result = report
                break
        if app.poll() is not None:
            result = {'complete': False, 'error': f'App exited {app.returncode}'}
            break
        time.sleep(.5)
finally:
    (output / 'native-results.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
    if not args.keep_open and app.poll() is None:
        app.terminate()
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(0 if result.get('complete') else 1)

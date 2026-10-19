#!/usr/bin/env python3
"""M4 probe only: paired hidden-window WebView recycling with R2 accounting."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading
import time
import urllib.request
import resource_opt as r2

ROOT = Path(__file__).resolve().parent.parent


def accepted(point, viewport):
    return (point['load_before'] <= 14 and point['ownership_count_check']
            and point['role_counts']['WebContent'] == 1 and point['footprint_exit'] == 0
            and all(p['phys_footprint_bytes'] is not None for p in point['processes'])
            and r2.viewport_matches(viewport, 2))


def total(point):
    return sum(p['phys_footprint_bytes'] or 0 for p in point['processes'] if p['role'] in r2.ROLES) / 1024**2


def activate_existing(bundle, pid):
    expected = str(bundle/'Contents/MacOS/axdb')
    assert r2.processes().get(pid, {}).get('command') == expected, 'Only the launched probe PID may be activated'
    script = '''on run argv
tell application "System Events"
    set frontmost of (first application process whose unix id = (item 1 of argv as integer)) to true
end tell
end run'''
    subprocess.run(['osascript', '-e', script, str(pid)], check=True, capture_output=True)
    return r2.place_window(pid, 2)


def bridge_probe(discovery, stop, rows):
    """Read-only requests; never persist discovery credentials or response content."""
    while not stop.is_set():
        start = time.monotonic()
        request = urllib.request.Request(discovery['url'], method='POST',
            headers={'Authorization': 'Bearer ' + discovery['token'], 'Content-Type': 'application/json', 'MCP-Protocol-Version': '2025-11-25'},
            data=json.dumps(dict(jsonrpc='2.0', id=len(rows)+1, method='tools/call', params=dict(name='get_ui_state', arguments={}))).encode())
        try:
            with urllib.request.urlopen(request, timeout=65) as response:
                value = json.load(response)
            result = value.get('result', {})
            content = result.get('structuredContent', result)
            ok = 'error' not in value and 'error' not in content and not result.get('isError', False)
            rows.append(dict(ok=ok, ms=(time.monotonic()-start)*1000))
        except Exception as error:
            rows.append(dict(ok=False, error=type(error).__name__, ms=(time.monotonic()-start)*1000))
        stop.wait(.15)


def main(args):
    bundle = Path(args.app).resolve()
    assert bundle.is_relative_to(ROOT/'target-m4') and bundle.suffix == '.app'
    executable = bundle/'Contents/MacOS/axdb'
    drivers = Path(args.drivers).resolve()
    assert drivers.is_dir() and drivers.parent.name.startswith('axdb-resource-')
    output = ROOT/'bench'/f'results-m4-{args.name}.json'
    assert args.name.replace('-', '').replace('_', '').isalnum() and not output.exists()
    modes = args.modes.split(',')
    assert all(mode in ('control', 'reload', 'replace') for mode in modes)
    result = dict(app=str(bundle), binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                  script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  r2_sha256=hashlib.sha256(Path(r2.__file__).read_bytes()).hexdigest(),
                  profile='dev, debug=0, incremental=0, isolated probe',
                  bridge_stress=args.bridge_probe, score_eligible=not args.bridge_probe, runs=[])
    def save():
        output.write_text(json.dumps(result, indent=2))
    lock = Path(tempfile.gettempdir())/'axdb-viewer-native.lock'
    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.write(fd, str(os.getpid()).encode())
    try:
        for mode in modes:
            config = Path(tempfile.mkdtemp(prefix='axdb-resource-m4-')).resolve()
            (config/'drivers').symlink_to(drivers, target_is_directory=True)
            row = dict(mode=mode, config=str(config), samples=[], bridge=[])
            result['runs'].append(row)
            row['launch_gate'] = r2.gate(f'm4-{mode}', threshold=14)
            assert row['launch_gate']['passed']
            prior = set(r2.processes())
            env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_BENCH': '1', 'AXDB_BENCH_TARGET': 'resource', 'AXDB_AGENT_BRIDGE': '1', 'AXDB_M4_MODE': mode}
            env.pop('AXDB_DRIVER_DIR', None)
            with (config/'stdio.log').open('w') as log:
                app = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
            row['host_pid'] = app.pid
            start = time.monotonic()
            stop = threading.Event()
            thread = None
            seen = 0
            try:
                while time.monotonic()-start < 650:
                    assert app.poll() is None, 'Probe exited early'
                    if (config/'m4-error.json').exists():
                        raise RuntimeError('Native recycle failed; inspect isolated m4-error.json')
                    try:
                        state = json.loads((config/'perf.json').read_text())
                    except (FileNotFoundError, json.JSONDecodeError):
                        assert time.monotonic()-start < 60, 'UI did not bootstrap'
                        time.sleep(.05)
                        continue
                    if 'complete' in state:
                        row['ui'] = state
                        assert state['complete'], state.get('error')
                        row['status'] = 'complete'
                        break
                    if state['checkpoint'] == seen:
                        time.sleep(.05)
                        continue
                    seen = state['checkpoint']
                    stage = state['stage']
                    if stage == 'prepare-window':
                        row['placement'] = activate_existing(bundle, app.pid)
                        assert row['placement']['moved'], 'Retina placement failed'
                    if stage.startswith('prepare-'):
                        assert r2.gate(stage, threshold=14)['passed']
                    else:
                        point = dict(stage=stage, ui=state, **r2.sample(app.pid, prior, start, config))
                        point['accepted'] = accepted(point, state['viewport'])
                        point['app_mib'] = total(point)
                        row['samples'].append(point)
                        print(mode, stage, round(point['app_mib'], 1), point['accepted'], flush=True)
                    save()
                    if args.bridge_probe and stage == 'tabs-closed-10':
                        discovery = json.loads((config/'agent-bridge.json').read_text())
                        thread = threading.Thread(target=bridge_probe, args=(discovery, stop, row['bridge']), daemon=True)
                        thread.start()
                    r2.acknowledge(config, seen)
                else:
                    raise TimeoutError('M4 scenario timeout')
            except BaseException as error:
                row.update(status='failed', error=str(error))
                r2.acknowledge(config, 'abort')
                raise
            finally:
                stop.set()
                if thread:
                    thread.join(timeout=66)
                children = {pid: p for pid, p in r2.processes().items() if p['ppid'] == app.pid and p['command'].startswith(str(drivers))}
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()
                deadline = time.monotonic()+10
                while any(pid in r2.processes() for pid in children) and time.monotonic() < deadline:
                    time.sleep(.2)
                remaining = r2.processes()
                for pid, process in children.items():
                    if pid in remaining and remaining[pid]['command'] == process['command']:
                        os.kill(pid, signal.SIGTERM)
                row['exit_verified'] = app.poll() is not None and not any(pid in r2.processes() for pid in children)
                save()
            time.sleep(3)
    finally:
        save()
        os.close(fd)
        lock.unlink()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', nargs='?')
    parser.add_argument('--drivers')
    parser.add_argument('--name')
    parser.add_argument('--modes', default='control,reload,replace')
    parser.add_argument('--bridge-probe', action='store_true', help='Unscored MCP traffic diagnostic; keep separate from footprint pairs')
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.self_check:
        from unittest.mock import patch
        point = dict(load_before=10, ownership_count_check=True, role_counts={'WebContent': 1}, footprint_exit=0,
                     processes=[dict(role='host', phys_footprint_bytes=1024**2), dict(role='driver-postgres', phys_footprint_bytes=1024**3)])
        viewport = dict(width=1280, height=768, scale=2)
        assert accepted(point, viewport) and total(point) == 1
        assert not accepted({**point, 'load_before': 15}, viewport)
        assert not accepted({**point, 'role_counts': {'WebContent': 2}}, viewport)
        assert not accepted(point, {**viewport, 'scale': 1})
        assert not accepted({**point, 'processes': [dict(phys_footprint_bytes=None)]}, viewport)
        with patch.object(r2, 'processes', return_value={1: dict(command='/unrelated/app')}), patch.object(subprocess, 'run') as launch:
            try:
                activate_existing(Path('/probe.app'), 1)
            except AssertionError:
                pass
            else:
                raise AssertionError('Unrelated PID accepted')
            launch.assert_not_called()
        with patch.object(r2, 'processes', return_value={1: dict(command='/probe.app/Contents/MacOS/axdb')}), patch.object(subprocess, 'run') as launch, patch.object(r2, 'place_window', return_value=dict(moved=True)):
            assert activate_existing(Path('/probe.app'), 1)['moved']
            assert launch.call_args.args[0][0] == 'osascript' and launch.call_args.args[0][-1] == '1'
        print('M4 accounting self-check passed; no app launched')
    else:
        assert args.app and args.drivers and args.name
        main(args)

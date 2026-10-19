#!/usr/bin/env python3
"""Isolated M2 experiment; reuse R2 PID ownership, sampling and geometry gates."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time
import resource_opt as r2

ROOT = Path(__file__).resolve().parent.parent


def pairs(runs):
    result = []
    for index in range(0, len(runs)-1, 2):
        left, right = runs[index:index+2]
        if {left['mode'], right['mode']} not in ({'raw', 'window'}, {'raw', 'dom-release'}): continue
        points = []
        for sample in left['samples']:
            other = next((point for point in right['samples'] if point['stage'] == sample['stage']), None)
            if other:
                delta = abs(sample['load_before'] - other['load_before'])
                points.append(dict(stage=sample['stage'], load_delta=delta, accepted=sample['accepted'] and other['accepted'] and delta <= 3))
        result.append(dict(runs=[index, index+1], points=points))
    return result


def main(args):
    assert re.fullmatch(r'[a-zA-Z0-9_-]+', args.name)
    bundle = Path(args.app).resolve()
    assert bundle.is_relative_to(ROOT / 'target-m2') and bundle.suffix == '.app'
    executable = bundle / 'Contents/MacOS/axdb'
    installed = Path(args.drivers).resolve()
    assert installed.parent.name.startswith('axdb-resource-'), 'Only test-installed drivers'
    output = ROOT / 'bench' / f'results-m2-{args.name}.json'
    assert not output.exists(), 'Preserve earlier attempts'
    result = dict(binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(), app=str(bundle), scenario=args.scenario, diagnostics=args.diagnostics, pressure=args.pressure, script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), r2_script_sha256=hashlib.sha256(Path(r2.__file__).read_bytes()).hexdigest(), platform=subprocess.check_output(['sw_vers'], text=True), build_profile='dev, debug=0, probe-only', runs=[])
    save = lambda: output.write_text(json.dumps(result, indent=2))
    lock = Path(tempfile.gettempdir()) / 'axdb-viewer-native.lock'
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.write(descriptor, str(os.getpid()).encode())
    try:
        for mode in args.modes.split(','):
            assert mode in ('raw', 'window', 'headless', 'dom-release')
            assert mode != 'dom-release' or args.scenario == 'resource'
            config = Path(tempfile.mkdtemp(prefix='axdb-resource-m2-')).resolve()
            (config / 'drivers').symlink_to(installed, target_is_directory=True)
            row = dict(mode=mode, config=str(config), samples=[])
            result['runs'].append(row); save()
            gate = r2.gate(f'm2-{mode}-launch', threshold=14)
            assert gate['passed'], 'Load gate failed'
            prior = set(r2.processes())
            env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_BENCH': '1', 'AXDB_BENCH_TARGET': args.scenario, 'AXDB_M2_MODE': mode, 'AXDB_AGENT_BRIDGE': '0'}
            env.pop('AXDB_DRIVER_DIR', None)
            with (config / 'stdio.log').open('w') as log:
                app = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
            start = time.monotonic(); seen = 0
            row['host_pid'] = app.pid
            try:
                while time.monotonic() - start < 700:
                    assert app.poll() is None, 'Probe exited early'
                    try:
                        state = json.loads((config / 'perf.json').read_text())
                    except (FileNotFoundError, json.JSONDecodeError):
                        assert time.monotonic() - start < 60, 'M2 UI did not bootstrap in 60 seconds'
                        time.sleep(.1); continue
                    if 'complete' in state:
                        row['ui'] = state
                        assert state['complete'], state.get('error')
                        row['status'] = 'complete'; save(); break
                    if state['checkpoint'] == seen:
                        time.sleep(.1); continue
                    seen = state['checkpoint']; stage = state['stage']
                    if stage == 'prepare-window':
                        row['placement'] = r2.activate_window(bundle, app.pid, 2)
                        assert row['placement']['moved'], 'Retina placement failed'
                    if stage.startswith('prepare-'):
                        gate = r2.gate(stage, threshold=14)
                        assert gate['passed'], 'Preparation load gate failed'
                        r2.acknowledge(config, seen); continue
                    point = dict(stage=stage, ui=state, **r2.sample(app.pid, prior, start, config))
                    point['accepted'] = point['load_before'] <= 14 and point['ownership_count_check'] and point['role_counts']['WebContent'] == 1 and point['footprint_exit'] == 0 and all(p['phys_footprint_bytes'] is not None for p in point['processes']) and r2.viewport_matches(state['viewport'], 2)
                    point['score_accepted'] = point['accepted'] and not args.diagnostics
                    row['samples'].append(point); save()
                    print(mode, stage, round(sum(p['phys_footprint_bytes'] or 0 for p in point['processes'] if p['role'] in r2.ROLES)/1024**2, 1), point['accepted'], flush=True)
                    if args.diagnostics and stage in ('table-100000', 'tabs-closed-10', 'cache-requested-3', 'pressure-after'):
                        web = [p['pid'] for p in point['processes'] if p['role'] == 'WebContent']
                        assert len(web) == 1
                        for tool, command in [('vmmap', ['/usr/bin/vmmap', '-summary', str(web[0])]), ('heap', ['/usr/bin/heap', '-s', '-z', '--noContent', str(web[0])])]:
                            probe = subprocess.run(command, capture_output=True, text=True, timeout=45)
                            path = ROOT / 'bench' / f'results-m2-{args.name}-{mode}-{len(result["runs"])}-{stage}-{tool}.txt'
                            path.write_text(probe.stdout + probe.stderr)
                            point.setdefault('diagnostics', {})[tool] = dict(exit=probe.returncode, path=str(path.relative_to(ROOT)))
                        if stage == 'tabs-closed-10':
                            graph = ROOT / 'target-m2' / f'{args.name}-{mode}-{len(result["runs"])}.memgraph'
                            probe = subprocess.run(['/usr/bin/leaks', '--noContent', '--nostacks', f'--outputGraph={graph}', str(web[0])], capture_output=True, text=True, timeout=45)
                            path = ROOT / 'bench' / f'results-m2-{args.name}-{mode}-{len(result["runs"])}-{stage}-leaks.txt'
                            path.write_text(probe.stdout + probe.stderr)
                            point['diagnostics']['leaks'] = dict(exit=probe.returncode, path=str(path.relative_to(ROOT)), graph_created=graph.exists())
                        point['diagnostic_timing_contaminated'] = True
                    if stage == 'pressure-ready' and args.pressure:
                        point['pressure_before'] = subprocess.check_output(['/usr/bin/memory_pressure', '-Q'], text=True)
                        point['swap_before'] = subprocess.check_output(['/usr/sbin/sysctl', 'vm.swapusage'], text=True)
                        if point['load_before'] > 14:
                            raise RuntimeError('Do not add pressure above the approved load gate')
                        helper = subprocess.Popen([str(ROOT/'target-m2/pressure'), '2048'], stdout=subprocess.PIPE, text=True)
                        pressure_samples = []; deadline = time.monotonic() + 25
                        try:
                            while helper.poll() is None and time.monotonic() < deadline:
                                query = subprocess.check_output(['/usr/bin/memory_pressure', '-Q'], text=True)
                                pressure_samples.append(query)
                                available = re.search(r'free percentage:\s*(\d+)%', query)
                                if available and int(available[1]) < 15:
                                    point['pressure_stopped'] = 'available memory below 15%'; break
                                time.sleep(1)
                        finally:
                            if helper.poll() is None: helper.kill()
                            stdout, _ = helper.communicate(timeout=5)
                        point['pressure_exit'] = helper.returncode
                        point['pressure_stdout'] = stdout
                        point['pressure_samples'] = pressure_samples
                        point['pressure_after'] = subprocess.check_output(['/usr/bin/memory_pressure', '-Q'], text=True)
                        point['swap_after'] = subprocess.check_output(['/usr/sbin/sysctl', 'vm.swapusage'], text=True)
                        time.sleep(5); save()
                    r2.acknowledge(config, seen)
                else:
                    raise TimeoutError('M2 probe timeout')
            except BaseException as error:
                row['status'] = 'failed'; row['error'] = str(error); save()
                r2.acknowledge(config, 'abort')
                raise
            finally:
                children = {pid: p for pid,p in r2.processes().items() if p['ppid'] == app.pid and p['command'].startswith(str(installed))}
                app.terminate()
                try: app.wait(timeout=10)
                except subprocess.TimeoutExpired: app.kill(); app.wait()
                deadline = time.monotonic()+10
                while any(pid in r2.processes() for pid in children) and time.monotonic()<deadline: time.sleep(.2)
                remaining = r2.processes()
                for pid,p in children.items():
                    if pid in remaining and remaining[pid]['command']==p['command']: os.kill(pid, signal.SIGTERM)
                row['exit_verified'] = app.poll() is not None and not any(pid in r2.processes() for pid in children)
                save()
            time.sleep(3)
    finally:
        result['pairs'] = pairs(result['runs']); save()
        os.close(descriptor); lock.unlink()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', nargs='?')
    parser.add_argument('--drivers')
    parser.add_argument('--name')
    parser.add_argument('--modes', default='raw,window,headless')
    parser.add_argument('--scenario', choices=('m2', 'resource'), default='m2')
    parser.add_argument('--diagnostics', action='store_true')
    parser.add_argument('--pressure', action='store_true')
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.self_check:
        def row(mode, load, accepted=True): return dict(mode=mode, samples=[dict(stage='startup', load_before=load, accepted=accepted)])
        assert pairs([row('raw', 10), row('window', 13)])[0]['points'][0]['accepted']
        assert not pairs([row('raw', 9), row('window', 13)])[0]['points'][0]['accepted']
        assert not pairs([row('window', 10), row('raw', 10, False)])[0]['points'][0]['accepted']
        assert not pairs([row('headless', 10), row('raw', 10)])
        print('m2 self-check passed (no app launched)')
    else:
        assert args.app and args.drivers and args.name
        main(args)

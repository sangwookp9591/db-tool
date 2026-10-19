#!/usr/bin/env python3
"""macOS release .app resource matrix; serialize with W8 before passing --w8-complete.

python3 bench/resource_opt.py --self-check
python3 bench/resource_opt.py target-p1/release/bundle/macos/AxDB.app --w8-complete --variant baseline
TOKIO_WORKER_THREADS=2 python3 bench/resource_opt.py APP --w8-complete --variant workers2
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time
import zipfile

ROLES = ('host', 'WebContent', 'Networking', 'GPU')


def cpu_seconds(value):
    total = 0.0
    for part in value.split(':'):
        total = total * 60 + float(part)
    return total


def processes():
    rows = {}
    for line in subprocess.check_output(['/bin/ps', '-axo', 'pid=,ppid=,rss=,time=,%cpu=,comm='], text=True).splitlines():
        parts = line.strip().split(None, 5)
        if len(parts) == 6:
            pid, ppid, rss, cpu, percent, command = parts
            rows[int(pid)] = dict(pid=int(pid), ppid=int(ppid), rss_kib=int(rss), cpu_seconds=cpu_seconds(cpu), rolling_cpu_percent=float(percent), command=command)
    return rows


def footprints(raw):
    result = {}
    for pid, size, unit in re.findall(r'\[(\d+)\]:[^\n]*Footprint:\s*([\d.]+)(?:\s*(GB|MB|KB|B))?', raw):
        result[int(pid)] = float(size) * {'': 1, 'B': 1, 'KB': 1024, 'MB': 1024**2, 'GB': 1024**3}[unit]
    return result


def slope(points):
    if len(points) < 2:
        return None
    xs, ys = zip(*points)
    center = statistics.mean(xs)
    return sum((x - center) * (y - statistics.mean(ys)) for x, y in points) / sum((x - center)**2 for x in xs)


def gate(label, timeout=600, threshold=8):
    started = time.monotonic()
    samples = []
    while True:
        load = os.getloadavg()[0]
        elapsed = time.monotonic() - started
        roster = processes()
        samples.append(dict(at=datetime.datetime.now().astimezone().isoformat(), load1=load, waited_seconds=elapsed,
                            ps_top_cpu=sorted(roster.values(), key=lambda p: p['rolling_cpu_percent'], reverse=True)[:20]))
        if load <= threshold or elapsed >= timeout:
            return dict(label=label, passed=load <= threshold, threshold=threshold, timeout_seconds=timeout, samples=samples)
        print(f'gate {label}: load={load:.2f}, waited={elapsed:.0f}s', flush=True)
        time.sleep(min(30, timeout - elapsed))


def owned_processes(host, prior, roster, config=None):
    owned = {pid: dict(row) for pid, row in roster.items() if pid == host or (pid not in prior and 'com.apple.WebKit.' in row['command'])}
    for pid, row in owned.items():
        row['role'] = 'host' if pid == host else next((role for role in ROLES[1:] if f'WebKit.{role}' in row['command']), 'unknown')
    if config is not None:
        root = str((config / 'drivers').resolve()) + '/'
        for pid, row in roster.items():
            if row['ppid'] == host and row['command'].startswith(root):
                owned[pid] = dict(row, role=Path(row['command']).name.removeprefix('axdb-'))
    return owned


def viewport_matches(viewport, scale):
    return viewport == dict(width=1280, height=768, scale=scale)


def screen_position(screens, scale=2):
    screen = next((s for s in screens if s['scale'] == scale and s['width'] >= 1280 and s['height'] >= 800), None)
    return (int(screen['x'] + 10), int(screen['primaryHeight'] - screen['y'] - screen['height'] + 10)) if screen else None


def place_window(host, scale):
    script = 'ObjC.import("AppKit"); const screens=$.NSScreen.screens; const rows=[]; for(let i=0;i<screens.count;i++){const s=screens.objectAtIndex(i);const f=s.visibleFrame;rows.push({scale:s.backingScaleFactor,x:f.origin.x,y:f.origin.y,width:f.size.width,height:f.size.height,primaryHeight:screens.objectAtIndex(0).frame.size.height});}JSON.stringify(rows);'
    try:
        screens = json.loads(subprocess.check_output(['osascript', '-l', 'JavaScript', '-e', script], text=True, timeout=10))
        position = screen_position(screens, scale)
        if position is None:
            return dict(moved=False, screens=screens, requested_scale=scale, reason='No fitting screen; report viewport mismatch')
        # AX size includes the 32px title bar; the benchmark checks the 1280x768 WebView viewport.
        move = '''on run argv
set targetPid to (item 1 of argv) as integer
set targetX to (item 2 of argv) as integer
set targetY to (item 3 of argv) as integer
tell application "System Events"
    tell (first application process whose unix id is targetPid)
        repeat 50 times
            if frontmost and (exists window 1) then exit repeat
            delay 0.1
        end repeat
        delay 0.2
        set position of window 1 to {targetX, targetY}
        set size of window 1 to {1280, 800}
        delay 0.2
        return {position of window 1, size of window 1}
    end tell
end tell
end run'''
        result = subprocess.run(['osascript', '-e', move, str(host), *map(str, position)], capture_output=True, text=True, timeout=15)
        return dict(moved=result.returncode == 0, screens=screens, requested_scale=scale, position=position, exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
    except (subprocess.SubprocessError, json.JSONDecodeError) as error:
        return dict(moved=False, requested_scale=scale, reason=str(error))


def activate_window(bundle, host, scale=None):
    assert not any(p['command'] == '/Applications/AxDB.app/Contents/MacOS/axdb' for p in processes().values()), 'User app is running; refusing LaunchServices activation'
    subprocess.run(['/usr/bin/open', '-a', str(bundle)], check=True, capture_output=True)
    return place_window(host, scale) if scale is not None else None


def sample(host, prior, start, config=None):
    load_before = os.getloadavg()[0]
    first = processes()
    owned = owned_processes(host, prior, first, config)
    assert host in owned, 'App exited while sampling'
    begun = time.monotonic()
    time.sleep(1)
    second = processes()
    elapsed = time.monotonic() - begun
    args = [arg for pid in owned for arg in ('-p', str(pid))]
    footprint = subprocess.run(['/usr/bin/footprint', *args, '--noCategories', '-f', 'bytes'], capture_output=True, text=True, timeout=30)
    sizes = footprints(footprint.stdout)
    for pid, row in owned.items():
        row['phys_footprint_bytes'] = sizes.get(pid)
        row['cpu_interval_percent'] = (second[pid]['cpu_seconds'] - row['cpu_seconds']) / elapsed * 100 if pid in second else None
    counts = {role: sum(row['role'] == role for row in owned.values()) for role in (*ROLES, 'driver-postgres', 'driver-neo4j')}
    # shortcut: XPC helpers are attributed by launch-time set difference; reject multiple helpers per role, upgrade to Instruments for shared-process attribution.
    return dict(at_seconds=time.monotonic() - start, load_before=load_before, processes=list(owned.values()), role_counts=counts,
                system_builders=[p for p in first.values() if Path(p['command']).name in ('rustc', 'cargo', 'node', 'tsgo', 'clang', 'swiftc', 'ld')],
                ownership='host + direct driver children under isolated config + newly observed WebKit PIDs; no unrelated WebKit launch may overlap', ownership_count_check=counts['host'] == 1 and all(n <= 1 for n in counts.values()) and all(p['role'] in counts for p in owned.values()),
                cpu_interval_seconds=elapsed, cpu_sample_started_seconds=begun - start, footprint_exit=footprint.returncode, footprint_raw=footprint.stdout, footprint_error=footprint.stderr,
                rss_kib=sum(row['rss_kib'] for row in owned.values()))


def host_timings(raw):
    result = []
    for line in raw.splitlines():
        if 'AXDB_BENCH_TIME' not in line:
            continue
        name = re.search(r'bench_name="([^"]+)"', line)
        start = re.search(r'bench_start_ms=(\d+)', line)
        elapsed = re.search(r'bench_elapsed_ms=([\d.eE+-]+)', line)
        if not name or not start or not elapsed:
            raise ValueError('Malformed native benchmark timing')
        result.append(dict(name=name[1], start_unix_ms=int(start[1]), elapsed_ms=float(elapsed[1])))
    return result


def uptime_load(value):
    match = re.search(r'load averages?:\s*([\d.]+)', value)
    if not match:
        raise ValueError('Missing native scroll load value')
    return float(match[1])


def paired_load_quality(runs, threshold=14, delta_limit=3):
    result = {}
    for repeat in sorted({run['repeat'] for run in runs}):
        group = {run['artifact']: run for run in runs if run['repeat'] == repeat}
        if set(group) != {0, 1}:
            result[str(repeat)] = dict(accepted=False, reason='Incomplete pair', points=[]); continue
        def points(run):
            if run['samples']:
                return {(s['stage'], s['iteration']): s['load_before'] for s in run['samples']}
            return {(key, trial): uptime_load(sample['load']) for key, samples in run.get('ui', {}).get('results', {}).items() if key.startswith('table-') for trial, sample in enumerate(samples)}
        left, right = points(group[0]), points(group[1]); checks = []
        for key in sorted(set(left) & set(right)):
            delta = abs(left[key] - right[key])
            checks.append(dict(stage=key[0], iteration=key[1], baseline_load=left[key], candidate_load=right[key], delta=delta,
                               accepted=max(left[key], right[key]) <= threshold and delta <= delta_limit))
        complete = all(r.get('status') == 'complete' and r.get('exit_verified') and all(s['accepted'] for s in r['samples']) for r in group.values())
        result[str(repeat)] = dict(accepted=complete and bool(checks) and set(left) == set(right) and all(p['accepted'] for p in checks), points=checks)
    return result


def acknowledge(config, value):
    temporary = config / 'resource-ack.tmp'
    temporary.write_text(str(value))
    temporary.replace(config / 'resource-ack')


def run(args):
    assert args.w8_complete, 'Wait for coordinator W8 completion notification, then pass --w8-complete'
    assert args.app, 'A release .app bundle is required'
    load_limit = 14 if args.paired_load_14 else 8
    assert not args.paired_load_14 or args.compare_app and len(args.compare_app) == 1 and not args.workers and not args.connections, 'Relaxed load is only for two-app paired A/B'
    bundles = [Path(value).resolve() for value in [args.app, *(args.compare_app or [])] if value]
    artifacts = []
    for bundle in bundles:
        assert bundle.suffix == '.app' and bundle.is_dir(), 'A release .app bundle is required'
        executable = bundle / 'Contents/MacOS/axdb'
        assert executable.is_file(), executable
        artifacts.append(dict(app=str(bundle), sha256=hashlib.sha256(executable.read_bytes()).hexdigest(), binary_bytes=executable.stat().st_size))
    assert bool(args.driver_packages) == bool(args.installer_test), 'Pass both --driver-packages and --installer-test'
    packages = [str((Path(args.driver_packages) / f'{driver}-0.2.0.axdriver').resolve()) for driver in ('postgres', 'neo4j')] if args.driver_packages else []
    result_packages = [dict(path=p, sha256=hashlib.sha256(Path(p).read_bytes()).hexdigest()) for p in packages]
    installed_root = None
    receipt = None
    if args.installed_receipt:
        assert packages and args.installer_test, 'Provide package and installer provenance with the receipt'
        receipt = json.loads(Path(args.installed_receipt).read_text())
        assert receipt['exit'] == 0 and '1 passed' in receipt['stdout'], 'Actual host installation receipt required'
        assert receipt['helper_sha256'] == hashlib.sha256(Path(args.installer_test).read_bytes()).hexdigest(), 'Installer receipt mismatch'
        installed_root = (Path(receipt['config']) / 'drivers').resolve()
        assert installed_root.parent.is_relative_to(Path(tempfile.gettempdir()).resolve()) and installed_root.parent.name.startswith('axdb-resource-'), 'Only isolated test installations may be reused'
        for package_path, driver in zip(packages, ('postgres', 'neo4j')):
            marker = json.loads((installed_root / driver / 'current.json').read_text())
            assert marker['version'] == '0.2.0' and not marker['allow_unverified'], 'Signed release installation required'
            with zipfile.ZipFile(package_path) as package:
                assert package.read('manifest.json') == (installed_root / driver / '0.2.0/manifest.json').read_bytes(), 'Installed package mismatch'

    output = Path(__file__).parent / f'results-resource-{args.variant}.json'
    assert not output.exists(), f'Preserve existing evidence: {output}'
    workers = args.workers.split(',') if args.workers else [os.environ.get('TOKIO_WORKER_THREADS', 'default')]
    assert all(w == 'default' or w.isdecimal() and int(w) > 0 for w in workers), 'Invalid worker count'
    configurations = [(index, 'default') for index in range(1, len(bundles))] + [(0, worker) for worker in workers]
    schedule = [(repeat, index, worker) for repeat in range(1, args.runs + 1) for index, worker in (configurations if repeat % 2 else configurations[::-1])]
    result = dict(variant=args.variant, scenario="scroll" if args.scroll_check else "connections" if args.connections else "resource", artifacts=artifacts, workers=workers, load_policy=dict(threshold=load_limit, paired_max_delta=3 if args.paired_load_14 else None), script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  installer_sha256=hashlib.sha256(Path(args.installer_test).read_bytes()).hexdigest() if args.installer_test else None, preinstalled_receipt=args.installed_receipt, installed_driver_root=str(installed_root) if installed_root else None, packages=result_packages, platform=subprocess.check_output(['sw_vers'], text=True), power_settings=subprocess.check_output(['pmset', '-g'], text=True), runs=[])
    save = lambda: output.write_text(json.dumps(result, indent=2))
    lock = Path(tempfile.gettempdir()) / 'axdb-viewer-native.lock'
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(descriptor, str(os.getpid()).encode())
        for repeat, index, worker in schedule:
            bundle = bundles[index]; executable = bundle / 'Contents/MacOS/axdb'
            launch_gate = gate(f'{args.variant}-app{index}-workers{worker}-{repeat}-launch', threshold=load_limit)
            config = Path(tempfile.mkdtemp(prefix='axdb-resource-')).resolve()
            row = dict(repeat=repeat, artifact=index, workers=worker, config=str(config), launch_gate=launch_gate, samples=[])
            result['runs'].append(row)
            if not launch_gate['passed']:
                row['status'] = 'gate-failed'; save()
                raise RuntimeError('Launch load gate failed after 600 seconds; no app launched')
            if installed_root:
                (config / 'drivers').symlink_to(installed_root, target_is_directory=True)
                row['installation'] = dict(reused_actual_installation=str(installed_root), receipt=args.installed_receipt)
            elif packages:
                install = subprocess.run([str(Path(args.installer_test).resolve()), '--ignored', '--exact', 'driver_install::tests::resource_bench_install_signed_packages', '--nocapture'],
                    env={**os.environ, 'AXDB_RESOURCE_INSTALL_ROOT': str(config), 'AXDB_RESOURCE_INSTALL_PACKAGES': json.dumps(packages)}, capture_output=True, text=True, timeout=120)
                row['installation'] = dict(exit=install.returncode, stdout=install.stdout, stderr=install.stderr)
                save()
                assert install.returncode == 0 and '1 passed' in install.stdout, 'Real host driver installation failed'
            prior_rows = processes(); prior = set(prior_rows)
            row['launch_ps'] = list(prior_rows.values())
            row['vm_stat'] = subprocess.check_output(['/usr/bin/vm_stat'], text=True)
            row['swapusage'] = subprocess.check_output(['/usr/sbin/sysctl', 'vm.swapusage'], text=True)
            env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_AGENT_BRIDGE': '0', 'AXDB_BENCH': '1',
                   'AXDB_BENCH_TARGET': 'matrix' if args.scroll_check else 'connections' if args.connections else 'resource-leak' if repeat == 1 and not args.no_leak else 'resource'}
            env.pop('TOKIO_WORKER_THREADS', None)
            if worker != 'default': env['TOKIO_WORKER_THREADS'] = worker
            with (config / 'stderr.log').open('w') as log:
                row["launch_unix_ms"] = time.time() * 1000
                app = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
                started = time.monotonic()
                row['host_pid'] = app.pid
                seen = 0
                preparation_passed = launch_gate['passed']
                try:
                    duration_limit = 900 if args.scroll_check else 3600
                    while time.monotonic() - started < duration_limit:
                        assert app.poll() is None, f'App exited early: {app.returncode}'
                        try:
                            state = json.loads((config / 'perf.json').read_text())
                        except (FileNotFoundError, json.JSONDecodeError):
                            assert time.monotonic() - started < 60, 'Resource UI did not bootstrap'
                            time.sleep(.05); continue
                        if args.scroll_check and 'complete' not in state and not row.get('window_activated'):
                            row['window_placement'] = activate_window(bundle, app.pid, 1 if args.scale_one else 2 if args.retina else None)
                            if args.retina or args.scale_one:
                                assert row['window_placement']['moved'], 'Native scroll window placement failed'
                            row['window_activated'] = True
                            save()
                        if 'complete' in state:
                            row['ui'] = state
                            if args.connections:
                                row['host_timings'] = host_timings((config / 'axdb.log').read_text())
                                assert row['host_timings'], 'Native command timings were not recorded'
                            assert state['complete'], state
                            if args.scroll_check:
                                tables = {key: value for key, value in state['results'].items() if key.startswith('table-')}
                                assert len(tables) == 8 and all(len(samples) == 3 and all(sample['frames']['p95'] > 0 for sample in samples) for samples in tables.values()), 'Native table scroll cases incomplete'
                            row['status'] = 'complete'; break
                        if args.scroll_check:
                            count = sum(len(samples) for samples in state.get('results', {}).values())
                            if count != row.get('scroll_case_count'):
                                row['scroll_case_count'] = count; save()
                                print(args.variant, index, repeat, 'native cases', count, flush=True)
                            time.sleep(.05); continue
                        if state['checkpoint'] != seen:
                            seen = state['checkpoint']
                            if seen == 1:
                                row['window_placement'] = activate_window(bundle, app.pid, 1 if args.scale_one else 2 if args.retina else None)
                            current = dict(viewport_matches_requested=viewport_matches(state.get('viewport'), 1 if args.scale_one else 2), viewport_matches_baseline=viewport_matches(state.get('viewport'), 2), stage=state['stage'], iteration=state['iteration'], ui_checkpoint_unix_ms=state.get('at'), codec=state.get('codec'), viewers=state.get('viewers'), canvas_pixels=state.get('canvasPixels'), graphWorkers=state['graphWorkers'], viewport=state.get('viewport'), last_query=state.get('lastQuery'))
                            label = f"{args.variant}-app{index}-workers{worker}-{repeat}-{state['stage']}-{state['iteration']}"
                            if state['stage'].startswith('prepare-'):
                                current['gate'] = gate(label, timeout=0 if state['stage'] == 'prepare-window' else 600, threshold=load_limit)
                                preparation_passed = current['gate']['passed']
                                row.setdefault('preparation_gates', []).append(current); save()
                                acknowledge(config, seen if preparation_passed else 'abort')
                                continue
                            current['gate'] = gate(label, timeout=0, threshold=load_limit)
                            current['preparation_gate_passed'] = preparation_passed
                            current.update(sample(app.pid, prior, started, config))
                            if state['stage'] == 'idle-60':
                                previous = next(s for s in row['samples'] if s['stage'] == 'idle-10')
                                span = current['cpu_sample_started_seconds'] - previous['cpu_sample_started_seconds']
                                initial = {p['pid']: p['cpu_seconds'] for p in previous['processes']}
                                for process in current['processes']:
                                    process['idle_interval_percent'] = (process['cpu_seconds'] - initial[process['pid']]) / span * 100 if process['pid'] in initial else None
                                current['idle_cpu_interval_seconds'] = span
                            required_drivers = () if state['stage'] in ('startup', 'idle-10', 'idle-60') else ('driver-postgres',) if state['stage'] == 'table-100000' else ('driver-postgres', 'driver-neo4j')
                            current['required_driver_count_check'] = not packages or all(current['role_counts'][role] == 1 for role in required_drivers)
                            current['accepted'] = (not (args.retina or args.scale_one) or current['viewport_matches_requested']) and current['required_driver_count_check'] and preparation_passed and current['gate']['passed'] and current['load_before'] <= load_limit and current['ownership_count_check'] and current['footprint_exit'] == 0 and all(p['phys_footprint_bytes'] is not None for p in current['processes'])
                            row['samples'].append(current); save()
                            acknowledge(config, seen)
                            print(args.variant, index, worker, repeat, state['stage'], state['iteration'], round(current['rss_kib'] / 1024, 1), 'MiB', current['accepted'], flush=True)
                        time.sleep(.05)
                    else:
                        raise TimeoutError(f'Native benchmark exceeded {duration_limit} seconds')
                except BaseException as error:
                    row['status'] = 'failed'; row['error'] = str(error) or type(error).__name__; save()
                    if app.poll() is None:
                        acknowledge(config, 'abort')
                        cleanup_until = time.monotonic() + 15
                        while app.poll() is None and time.monotonic() < cleanup_until:
                            try:
                                final = json.loads((config / 'perf.json').read_text())
                                if 'complete' in final:
                                    row['ui'] = final; break
                            except (FileNotFoundError, json.JSONDecodeError):
                                pass
                            time.sleep(.1)
                    raise
                finally:
                    driver_pids = {pid: p['command'] for pid, p in processes().items() if p['ppid'] == app.pid and p['command'].startswith(str((config / 'drivers').resolve()) + '/')}
                    app.terminate()
                    try: app.wait(timeout=10)
                    except subprocess.TimeoutExpired: app.kill(); app.wait()
                    until = time.monotonic() + 10
                    while any(pid in processes() for pid in driver_pids) and time.monotonic() < until:
                        time.sleep(.1)
                    remaining = processes()
                    for pid, command in driver_pids.items():
                        if pid in remaining and remaining[pid]['command'] == command:
                            os.kill(pid, 15)
                    time.sleep(.2)
                    row['exit_verified'] = app.poll() is not None and not any(pid in processes() for pid in driver_pids)
                    for stage in ('repeat-query', 'repeat-tabs'):
                        points = [(s['iteration'], s['rss_kib'] / 1024) for s in row['samples'] if s['stage'] == stage and s['accepted']]
                        row[f'{stage}-rss-mib-per-cycle'] = slope(points)
                        row[f'{stage}-warm-rss-mib-per-cycle'] = slope([(x, y) for x, y in points if x >= 15])
                    save()
            time.sleep(3)
    finally:
        os.close(descriptor); lock.unlink()
    if args.paired_load_14:
        result['paired_load_quality'] = paired_load_quality(result['runs'])
        result['load_policy']['unit'] = 'point'
        for run in result['runs']:
            run['complete_pair_load_accepted'] = result['paired_load_quality'][str(run['repeat'])]['accepted']
        save()
    assert len(result['runs']) == len(schedule) and all(r.get('status') == 'complete' and r['exit_verified'] and all(s['accepted'] for s in r['samples']) for r in result['runs']), 'Incomplete or contaminated samples: inspect report'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', nargs='?')
    parser.add_argument('--w8-complete', action='store_true')
    parser.add_argument('--variant', default='baseline')
    parser.add_argument('--compare-app', action='append', help='Alternate another fixed bundle using default workers; may be repeated')
    parser.add_argument('--workers', help='Primary app comma-separated default/worker counts, alternated within each repeat')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--paired-load-14', action='store_true', help='Coordinator-approved A/B only: load<=14 and paired load difference<=3')
    parser.add_argument('--no-leak', action='store_true')
    scenario_options = parser.add_mutually_exclusive_group()
    scenario_options.add_argument('--connections', action='store_true')
    scenario_options.add_argument('--scroll-check', action='store_true', help='Run the existing native matrix to verify table scroll frames')
    display_options = parser.add_mutually_exclusive_group()
    display_options.add_argument('--retina', action='store_true', help='Move only the measured PID window to a fitting scale=2 screen')
    display_options.add_argument('--scale-one', action='store_true', help='Use the explicitly approved scale=1 fallback')
    parser.add_argument('--installed-receipt', help='Reuse the actual signed installation in this successful host-test receipt')
    parser.add_argument('--driver-packages', help='Directory containing signed PG/Neo4j 0.2.0 packages')
    parser.add_argument('--installer-test', help='axdb library test executable built with CARGO_TARGET_DIR=target-r2')
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    assert re.fullmatch(r'[a-zA-Z0-9_-]+', args.variant), 'Invalid variant name'
    assert args.runs > 0, 'Positive run count required'
    if args.self_check:
        from unittest.mock import patch
        assert screen_position([dict(scale=2, x=0, y=65, width=1512, height=884, primaryHeight=982)]) == (10, 43)
        assert screen_position([dict(scale=1, width=1920, height=1080)]) is None
        assert screen_position([dict(scale=1, x=-1920, y=-98, width=1920, height=1080, primaryHeight=982)], 1) == (-1910, 10)
        assert cpu_seconds('1:02:03.45') == 3723.45
        with patch('subprocess.check_output', return_value='42 1 2048 1:02.34 0.5 /tmp/space name/axdb\n'):
            assert processes()[42] == dict(pid=42, ppid=1, rss_kib=2048, cpu_seconds=62.34, rolling_cpu_percent=.5, command='/tmp/space name/axdb')
        fixture_root = Path('/tmp/config').resolve()
        roster = {1: dict(ppid=0, command='/tmp/app/axdb'), 2: dict(ppid=1, command=str(fixture_root / 'drivers/postgres/bin/axdb-driver-postgres')), 3: dict(ppid=9, command=str(fixture_root / 'drivers/neo4j/bin/axdb-driver-neo4j')), 4: dict(ppid=1, command='/other/drivers/axdb-driver-postgres')}
        owned = owned_processes(1, set(), roster, Path('/tmp/config'))
        assert set(owned) == {1, 2} and owned[2]['role'] == 'driver-postgres'
        assert 'role' not in roster[2], 'Do not mutate process snapshots'
        with tempfile.TemporaryDirectory() as directory:
            shared = Path(directory).resolve() / 'installed'; shared.mkdir()
            config = Path(directory).resolve() / 'config'; config.mkdir()
            (config / 'drivers').symlink_to(shared, target_is_directory=True)
            linked = {1: dict(ppid=0, command='/app/axdb'), 2: dict(ppid=1, command=str(shared / 'postgres/axdb-driver-postgres'))}
            assert owned_processes(1, set(), linked, config)[2]['role'] == 'driver-postgres'
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 B\nphys_footprint: 3145728 B') == {123: 3145728}
        assert footprints('WebContent [4]: 64-bit    Footprint: 1.5 MB') == {4: 1572864}
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 (16384 bytes per page)') == {123: 3145728}
        assert uptime_load('load averages: 13.5 12.0 9.0') == 13.5
        pair = [dict(repeat=1, artifact=i, status='complete', exit_verified=True, samples=[dict(stage='idle-60', iteration=0, load_before=9+i, accepted=True)]) for i in (0, 1)]
        assert paired_load_quality(pair)['1']['accepted']
        pair[1]['samples'][0]['load_before'] = 13
        assert not paired_load_quality(pair)['1']['accepted']
        pair[0]['samples'][0]['load_before'] = 14.1; pair[1]['samples'][0]['load_before'] = 14.1
        assert not paired_load_quality(pair)['1']['accepted']
        assert slope([(1, 3), (2, 5), (3, 7)]) == 2
        assert slope([]) is None
        assert host_timings('AXDB_BENCH_TIME bench_name="profile_save" bench_start_ms=12 bench_elapsed_ms=0.25 ignored=private') == [dict(name='profile_save', start_unix_ms=12, elapsed_ms=.25)]
        with patch('os.getloadavg', return_value=(8, 0, 0)), patch(__name__ + '.processes', return_value={}):
            assert gate('self-check')['passed']
        with patch('os.getloadavg', return_value=(8.01, 0, 0)), patch(__name__ + '.processes', return_value={}):
            assert not gate('self-check', timeout=0)['passed']
        assert not viewport_matches(dict(width=1280, height=768, scale=2), 1)
        assert viewport_matches(dict(width=1280, height=768, scale=2), 2)
        assert not viewport_matches(dict(width=1280, height=736, scale=2), 2)
        print('resource_opt self-check passed (no measurements)')
    else:
        run(args)

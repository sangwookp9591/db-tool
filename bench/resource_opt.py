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


def gate(label, timeout=600):
    started = time.monotonic()
    samples = []
    while True:
        load = os.getloadavg()[0]
        elapsed = time.monotonic() - started
        roster = processes()
        samples.append(dict(at=datetime.datetime.now().astimezone().isoformat(), load1=load, waited_seconds=elapsed,
                            ps_top_cpu=sorted(roster.values(), key=lambda p: p['rolling_cpu_percent'], reverse=True)[:20]))
        if load <= 8 or elapsed >= timeout:
            return dict(label=label, passed=load <= 8, threshold=8, timeout_seconds=timeout, samples=samples)
        print(f'gate {label}: load={load:.2f}, waited={elapsed:.0f}s', flush=True)
        time.sleep(min(30, timeout - elapsed))


def owned_processes(host, prior, roster, config=None):
    owned = {pid: dict(row) for pid, row in roster.items() if pid == host or (pid not in prior and 'com.apple.WebKit.' in row['command'])}
    for pid, row in owned.items():
        row['role'] = 'host' if pid == host else next((role for role in ROLES[1:] if f'WebKit.{role}' in row['command']), 'unknown')
    if config is not None:
        root = str(config / 'drivers') + '/'
        for pid, row in roster.items():
            if row['ppid'] == host and row['command'].startswith(root):
                owned[pid] = dict(row, role=Path(row['command']).name.removeprefix('axdb-'))
    return owned


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


def acknowledge(config, value):
    temporary = config / 'resource-ack.tmp'
    temporary.write_text(str(value))
    temporary.replace(config / 'resource-ack')


def run(args):
    assert args.w8_complete, 'Wait for coordinator W8 completion notification, then pass --w8-complete'
    assert args.app, 'A release .app bundle is required'
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
    output = Path(__file__).parent / f'results-resource-{args.variant}.json'
    assert not output.exists(), f'Preserve existing evidence: {output}'
    workers = args.workers.split(',') if args.workers else [os.environ.get('TOKIO_WORKER_THREADS', 'default')]
    assert all(w == 'default' or w.isdecimal() and int(w) > 0 for w in workers), 'Invalid worker count'
    configurations = [(index, 'default') for index in range(1, len(bundles))] + [(0, worker) for worker in workers]
    schedule = [(repeat, index, worker) for repeat in range(1, args.runs + 1) for index, worker in (configurations if repeat % 2 else configurations[::-1])]
    result = dict(variant=args.variant, scenario="connections" if args.connections else "resource", artifacts=artifacts, workers=workers, script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  installer_sha256=hashlib.sha256(Path(args.installer_test).read_bytes()).hexdigest() if args.installer_test else None, packages=result_packages, platform=subprocess.check_output(['sw_vers'], text=True), power_settings=subprocess.check_output(['pmset', '-g'], text=True), runs=[])
    save = lambda: output.write_text(json.dumps(result, indent=2))
    lock = Path(tempfile.gettempdir()) / 'axdb-viewer-native.lock'
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(descriptor, str(os.getpid()).encode())
        for repeat, index, worker in schedule:
            bundle = bundles[index]; executable = bundle / 'Contents/MacOS/axdb'
            launch_gate = gate(f'{args.variant}-app{index}-workers{worker}-{repeat}-launch')
            config = Path(tempfile.mkdtemp(prefix='axdb-resource-')).resolve()
            row = dict(repeat=repeat, artifact=index, workers=worker, config=str(config), launch_gate=launch_gate, samples=[])
            result['runs'].append(row)
            if not launch_gate['passed']:
                row['status'] = 'gate-failed'; save()
                raise RuntimeError('Launch load gate failed after 600 seconds; no app launched')
            if packages:
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
                   'AXDB_BENCH_TARGET': 'connections' if args.connections else 'resource-leak' if repeat == 1 and not args.no_leak else 'resource'}
            env.pop('TOKIO_WORKER_THREADS', None)
            if worker != 'default': env['TOKIO_WORKER_THREADS'] = worker
            with (config / 'stderr.log').open('w') as log:
                app = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
                started = time.monotonic()
                seen = 0
                preparation_passed = launch_gate['passed']
                try:
                    while time.monotonic() - started < 3600:
                        assert app.poll() is None, f'App exited early: {app.returncode}'
                        try:
                            state = json.loads((config / 'perf.json').read_text())
                        except (FileNotFoundError, json.JSONDecodeError):
                            assert time.monotonic() - started < 60, 'Resource UI did not bootstrap'
                            time.sleep(.05); continue
                        if 'complete' in state:
                            row['ui'] = state
                            if args.connections:
                                row['host_timings'] = host_timings((config / 'axdb.log').read_text())
                                assert row['host_timings'], 'Native command timings were not recorded'
                            assert state['complete'], state
                            row['status'] = 'complete'; break
                        if state['checkpoint'] != seen:
                            seen = state['checkpoint']
                            if seen == 1:
                                assert not any(p['command'] == '/Applications/AxDB.app/Contents/MacOS/axdb' for p in processes().values()), 'User app is running; refusing LaunchServices activation'
                                subprocess.run(['/usr/bin/open', '-a', str(bundle)], check=True, capture_output=True)
                            current = dict(stage=state['stage'], iteration=state['iteration'], graphWorkers=state['graphWorkers'], viewport=state.get('viewport'), last_query=state.get('lastQuery'))
                            label = f"{args.variant}-app{index}-workers{worker}-{repeat}-{state['stage']}-{state['iteration']}"
                            if state['stage'].startswith('prepare-'):
                                current['gate'] = gate(label)
                                preparation_passed = current['gate']['passed']
                                row.setdefault('preparation_gates', []).append(current); save()
                                acknowledge(config, seen if preparation_passed else 'abort')
                                continue
                            current['gate'] = gate(label, timeout=0)
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
                            current['accepted'] = current['required_driver_count_check'] and preparation_passed and current['gate']['passed'] and current['load_before'] <= 8 and current['ownership_count_check'] and current['footprint_exit'] == 0 and all(p['phys_footprint_bytes'] is not None for p in current['processes'])
                            row['samples'].append(current); save()
                            acknowledge(config, seen)
                            print(args.variant, index, worker, repeat, state['stage'], state['iteration'], round(current['rss_kib'] / 1024, 1), 'MiB', current['accepted'], flush=True)
                        time.sleep(.05)
                    else:
                        raise TimeoutError('Resource matrix exceeded one hour')
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
                    driver_pids = {pid: p['command'] for pid, p in processes().items() if p['ppid'] == app.pid and p['command'].startswith(str(config / 'drivers') + '/')}
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
    assert len(result['runs']) == len(schedule) and all(r.get('status') == 'complete' and r['exit_verified'] and all(s['accepted'] for s in r['samples']) for r in result['runs']), 'Incomplete or contaminated samples: inspect report'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', nargs='?')
    parser.add_argument('--w8-complete', action='store_true')
    parser.add_argument('--variant', default='baseline')
    parser.add_argument('--compare-app', action='append', help='Alternate another fixed bundle using default workers; may be repeated')
    parser.add_argument('--workers', help='Primary app comma-separated default/worker counts, alternated within each repeat')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--no-leak', action='store_true')
    parser.add_argument('--connections', action='store_true')
    parser.add_argument('--driver-packages', help='Directory containing signed PG/Neo4j 0.2.0 packages')
    parser.add_argument('--installer-test', help='axdb library test executable built with CARGO_TARGET_DIR=target-r2')
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    assert re.fullmatch(r'[a-zA-Z0-9_-]+', args.variant), 'Invalid variant name'
    assert args.runs > 0, 'Positive run count required'
    if args.self_check:
        from unittest.mock import patch
        assert cpu_seconds('1:02:03.45') == 3723.45
        with patch('subprocess.check_output', return_value='42 1 2048 1:02.34 0.5 /tmp/space name/axdb\n'):
            assert processes()[42] == dict(pid=42, ppid=1, rss_kib=2048, cpu_seconds=62.34, rolling_cpu_percent=.5, command='/tmp/space name/axdb')
        roster = {1: dict(ppid=0, command='/tmp/app/axdb'), 2: dict(ppid=1, command='/tmp/config/drivers/postgres/bin/axdb-driver-postgres'), 3: dict(ppid=9, command='/tmp/config/drivers/neo4j/bin/axdb-driver-neo4j'), 4: dict(ppid=1, command='/other/drivers/axdb-driver-postgres')}
        owned = owned_processes(1, set(), roster, Path('/tmp/config'))
        assert set(owned) == {1, 2} and owned[2]['role'] == 'driver-postgres'
        assert 'role' not in roster[2], 'Do not mutate process snapshots'
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 B\nphys_footprint: 3145728 B') == {123: 3145728}
        assert footprints('WebContent [4]: 64-bit    Footprint: 1.5 MB') == {4: 1572864}
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 (16384 bytes per page)') == {123: 3145728}
        assert slope([(1, 3), (2, 5), (3, 7)]) == 2
        assert slope([]) is None
        assert host_timings('AXDB_BENCH_TIME bench_name="profile_save" bench_start_ms=12 bench_elapsed_ms=0.25 ignored=private') == [dict(name='profile_save', start_unix_ms=12, elapsed_ms=.25)]
        with patch('os.getloadavg', return_value=(8, 0, 0)), patch(__name__ + '.processes', return_value={}):
            assert gate('self-check')['passed']
        with patch('os.getloadavg', return_value=(8.01, 0, 0)), patch(__name__ + '.processes', return_value={}):
            assert not gate('self-check', timeout=0)['passed']
        print('resource_opt self-check passed (no measurements)')
    else:
        run(args)

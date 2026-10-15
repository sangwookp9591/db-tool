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
    for line in subprocess.check_output(['/bin/ps', '-axo', 'pid=,rss=,time=,comm='], text=True).splitlines():
        parts = line.strip().split(None, 3)
        if len(parts) == 4:
            pid, rss, cpu, command = parts
            rows[int(pid)] = dict(pid=int(pid), rss_kib=int(rss), cpu_seconds=cpu_seconds(cpu), command=command)
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
        samples.append(dict(at=datetime.datetime.now().astimezone().isoformat(), load1=load, waited_seconds=elapsed))
        if load <= 8 or elapsed >= timeout:
            return dict(label=label, passed=load <= 8, threshold=8, timeout_seconds=timeout, samples=samples)
        print(f'gate {label}: load={load:.2f}, waited={elapsed:.0f}s', flush=True)
        time.sleep(min(30, timeout - elapsed))


def sample(host, prior, start):
    load_before = os.getloadavg()[0]
    first = processes()
    owned = {pid: row for pid, row in first.items() if pid == host or (pid not in prior and 'com.apple.WebKit.' in row['command'])}
    assert host in owned, 'App exited while sampling'
    for pid, row in owned.items():
        row['role'] = 'host' if pid == host else next((role for role in ROLES[1:] if f'WebKit.{role}' in row['command']), 'unknown')
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
    counts = {role: sum(row['role'] == role for row in owned.values()) for role in ROLES}
    # shortcut: XPC helpers are attributed by launch-time set difference; reject multiple helpers per role, upgrade to Instruments for shared-process attribution.
    return dict(at_seconds=time.monotonic() - start, load_before=load_before, processes=list(owned.values()), role_counts=counts,
                ownership='host + newly observed WebKit PIDs; no unrelated WebKit launch may overlap', ownership_unambiguous=counts['host'] == 1 and all(n <= 1 for n in counts.values()) and all(p['role'] != 'unknown' for p in owned.values()),
                cpu_interval_seconds=elapsed, cpu_sample_started_seconds=begun - start, footprint_exit=footprint.returncode, footprint_raw=footprint.stdout, footprint_error=footprint.stderr,
                rss_kib=sum(row['rss_kib'] for row in owned.values()))


def run(args):
    assert args.w8_complete, 'Wait for coordinator W8 completion notification, then pass --w8-complete'
    bundle = Path(args.app).resolve()
    assert bundle.suffix == '.app' and bundle.is_dir(), 'A release .app bundle is required'
    executable = bundle / 'Contents/MacOS/axdb'
    assert executable.is_file(), executable
    output = Path(__file__).parent / f'results-resource-{args.variant}.json'
    assert not output.exists(), f'Preserve existing evidence: {output}'
    result = dict(variant=args.variant, app=str(bundle), sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                  binary_bytes=executable.stat().st_size, workers=os.environ.get('TOKIO_WORKER_THREADS', 'default'),
                  platform=subprocess.check_output(['sw_vers'], text=True), runs=[])
    save = lambda: output.write_text(json.dumps(result, indent=2))
    lock = Path(tempfile.gettempdir()) / 'axdb-viewer-native.lock'
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(descriptor, str(os.getpid()).encode())
        for repeat in range(1, args.runs + 1):
            launch_gate = gate(f'{args.variant}-{repeat}-launch')
            config = Path(tempfile.mkdtemp(prefix='axdb-resource-'))
            row = dict(repeat=repeat, config=str(config), launch_gate=launch_gate, samples=[])
            result['runs'].append(row)
            if not launch_gate['passed']:
                row['status'] = 'gate-failed'; save(); continue
            prior = set(processes())
            env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_AGENT_BRIDGE': '0', 'AXDB_BENCH': '1',
                   'AXDB_BENCH_TARGET': 'resource-leak' if repeat == 1 and not args.no_leak else 'resource'}
            with (config / 'stderr.log').open('w') as log:
                app = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
                started = time.monotonic()
                seen = 0
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
                            assert state['complete'], state
                            row['status'] = 'complete'; break
                        if state['checkpoint'] != seen:
                            seen = state['checkpoint']
                            if seen == 1:
                                subprocess.run(['/usr/bin/open', '-a', str(bundle)], check=True, capture_output=True)
                            current = dict(stage=state['stage'], iteration=state['iteration'], graphWorkers=state['graphWorkers'])
                            current['gate'] = gate(f"{args.variant}-{repeat}-{state['stage']}-{state['iteration']}")
                            current.update(sample(app.pid, prior, started))
                            if state['stage'] == 'idle-60':
                                previous = next(s for s in row['samples'] if s['stage'] == 'idle-10')
                                span = current['cpu_sample_started_seconds'] - previous['cpu_sample_started_seconds']
                                initial = {p['pid']: p['cpu_seconds'] for p in previous['processes']}
                                for process in current['processes']:
                                    process['idle_interval_percent'] = (process['cpu_seconds'] - initial[process['pid']]) / span * 100 if process['pid'] in initial else None
                                current['idle_cpu_interval_seconds'] = span
                            current['accepted'] = current['gate']['passed'] and current['load_before'] <= 8 and current['ownership_unambiguous'] and current['footprint_exit'] == 0 and all(p['phys_footprint_bytes'] is not None for p in current['processes'])
                            row['samples'].append(current); save()
                            temporary = config / 'resource-ack.tmp'; temporary.write_text(str(seen)); temporary.replace(config / 'resource-ack')
                            print(args.variant, repeat, state['stage'], state['iteration'], round(current['rss_kib'] / 1024, 1), 'MiB', current['accepted'], flush=True)
                        time.sleep(.05)
                    else:
                        raise TimeoutError('Resource matrix exceeded one hour')
                except BaseException as error:
                    row['status'] = 'failed'; row['error'] = str(error); save(); raise
                finally:
                    app.terminate()
                    try: app.wait(timeout=10)
                    except subprocess.TimeoutExpired: app.kill(); app.wait()
                    for stage in ('repeat-query', 'repeat-tabs'):
                        points = [(s['iteration'], s['rss_kib'] / 1024) for s in row['samples'] if s['stage'] == stage and s['accepted']]
                        row[f'{stage}-rss-mib-per-cycle'] = slope(points)
                        row[f'{stage}-warm-rss-mib-per-cycle'] = slope([(x, y) for x, y in points if x >= 15])
                    save()
            time.sleep(3)
    finally:
        os.close(descriptor); lock.unlink()
    assert len(result['runs']) == args.runs and all(r.get('status') == 'complete' and all(s['accepted'] for s in r['samples']) for r in result['runs']), 'Incomplete or contaminated samples: inspect report'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', nargs='?')
    parser.add_argument('--w8-complete', action='store_true')
    parser.add_argument('--variant', default='baseline')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--no-leak', action='store_true')
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    assert re.fullmatch(r'[a-zA-Z0-9_-]+', args.variant), 'Invalid variant name'
    assert args.runs > 0, 'Positive run count required'
    if args.self_check:
        assert cpu_seconds('1:02:03.45') == 3723.45
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 B\nphys_footprint: 3145728 B') == {123: 3145728}
        assert footprints('WebContent [4]: 64-bit    Footprint: 1.5 MB') == {4: 1572864}
        assert footprints('axdb [123]: 64-bit    Footprint: 3145728 (16384 bytes per page)') == {123: 3145728}
        assert slope([(1, 3), (2, 5), (3, 7)]) == 2
        assert slope([]) is None
        print('resource_opt self-check passed (no measurements)')
    else:
        run(args)

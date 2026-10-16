#!/usr/bin/env python3
"""W9: equal-limit real-app query pairs; preserve W8 data and user settings."""
import argparse, hashlib, json, os, pathlib, re, shutil, subprocess, time
from compare_gate import wait_gate, build_processes, require_test_profiles

ROOT = pathlib.Path(__file__).resolve().parent
WORK = pathlib.Path('/tmp/axdb-compare')
TAB = pathlib.Path.home() / 'Library/Application Support/tabularis'
FIRST = 'c4ca4238a0b923820dcc509a6f75849b'
QUERIES = {'G1': 'SELECT g, md5(g::text), now() FROM generate_series(1,100000) g', 'big': 'SELECT * FROM e2e.big'}


def gui(app, *args):
    op = subprocess.run(['orca', 'computer', *args, '--app', app, '--restore-window', '--no-screenshot', '--json'], text=True, capture_output=True, timeout=45)
    result = json.loads(op.stdout)
    if not result.get('ok') and args[0] == 'get-app-state' and result.get('error', {}).get('code') in ('app_not_found', 'window_not_found'):
        return ''
    if not result.get('ok'):
        raise RuntimeError(result)
    return result['result'].get('snapshot', {}).get('treeText', '')


def find(app, pattern, timeout=30):
    start = time.monotonic()
    tree = ''
    while time.monotonic() - start < timeout:
        tree = gui(app, 'get-app-state')
        for line in tree.splitlines():
            match = re.match(r'\s*(\d+) (.*)', line)
            if match and re.search(pattern, match[2]):
                return match[1], tree
        time.sleep(.2)
    raise RuntimeError(f'{app}: missing {pattern}: {tree}')


def click(app, pattern):
    index, _ = find(app, pattern)
    return gui(app, 'click', '--element-index', index)


def processes():
    rows = {}
    for line in subprocess.check_output(['ps', '-axo', 'pid=,rss=,time=,comm='], text=True).splitlines():
        pid, rss, cpu, comm = line.strip().split(maxsplit=3)
        seconds = 0.0
        for part in cpu.split(':'):
            seconds = seconds * 60 + float(part)
        rows[int(pid)] = {'rss_kib': int(rss), 'cpu_seconds': seconds, 'comm': comm}
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tabularis_exe')
    parser.add_argument('axdb_exe')
    parser.add_argument('--preflight', action='store_true')
    args = parser.parse_args()
    require_test_profiles(WORK / 'tabularis-connected', WORK / 'axdb-connected')
    backup = WORK / 'tabularis-preserved-w9'
    assert not backup.exists(), 'Restore preserved settings before rerunning'
    stage = 'preflight' if args.preflight else 'pairs'
    output = ROOT / f'results-compare-gen-{stage}.json'
    assert not output.exists(), 'Preserve previous W9 samples before rerunning'
    apps = [('Tabularis', str(pathlib.Path(args.tabularis_exe).resolve()), 'tabularis'), ('AxDB', str(pathlib.Path(args.axdb_exe).resolve()), 'dev.axdb.app')]
    hashes = {app: hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest() for app, exe, _ in apps}
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    rows = []
    def save(row):
        rows.append(row)
        output.write_text(json.dumps(rows, indent=2))
    for limit in (1000, 100000):
        for query_id in (('G1',) if args.preflight else ('G1', 'big')):
            for run in range(1, 2 if args.preflight else 6):
                gate = wait_gate(f'w9-{stage}-{limit}-{query_id}-{run}', output=ROOT / 'results-compare-gen-gates.json')
                order = apps[::-1] if run % 2 == 0 else apps
                for position, (app, exe, selector) in enumerate(order, 1):
                    row = {'stage': stage, 'app': app, 'limit': limit, 'query_id': query_id, 'query': QUERIES[query_id], 'run': run, 'order': position, 'gate': gate, 'head': head, 'binary_sha256': hashes[app], 'valid': False, 'measured': False}
                    before = processes()
                    row['load_before'] = os.getloadavg()[0]
                    builders = build_processes()
                    if not gate['passed'] or builders or row['load_before'] > 8:
                        row.update(reason='load_or_build_gate_failed', builders=builders)
                        save(row)
                        continue
                    p = None
                    env = os.environ.copy()
                    for name in ('AXDB_BENCH', 'AXDB_BENCH_TARGET', 'TOKIO_WORKER_THREADS'):
                        env.pop(name, None)
                    env['AXDB_AGENT_BRIDGE'] = '0'
                    try:
                        if app == 'Tabularis':
                            shutil.move(TAB, backup)
                            shutil.copytree(WORK / 'tabularis-connected', TAB)
                            config = json.loads((TAB / 'config.json').read_text())
                            config.update(resultPageSize=limit, autoConnectLastConnection=False, lastOpenConnectionIds=[], lastActiveConnectionId=None)
                            (TAB / 'config.json').write_text(json.dumps(config))
                        else:
                            cfg = WORK / f'w9-{stage}-{limit}-{query_id}-{run}-{time.time_ns()}'
                            cfg.mkdir()
                            shutil.copy2(WORK / 'axdb-connected/profiles.json', cfg / 'profiles.json')
                            profiles = json.loads((cfg / 'profiles.json').read_text())
                            profiles[0]['save_secret'] = False
                            (cfg / 'profiles.json').write_text(json.dumps(profiles))
                            env['AXDB_CONFIG_DIR'] = str(cfg)
                        p = subprocess.Popen([exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                        selector = f'pid:{p.pid}'
                        find(selector, r'standard window')
                        subprocess.run(['/tmp/compare-probe', 'activate', str(p.pid)], check=True)
                        if app == 'Tabularis':
                            _, tree = find(selector, r'button Connect$')
                            if 'button Maybe later' in tree:
                                click(selector, r'button Maybe later$')
                            click(selector, r'button Connect$')
                            find(selector, r'editor .*Editor content')
                            click(selector, r'button New Console$')
                            index, _ = find(selector, r'editor .*Editor content, Secondary Actions')
                            gui(selector, 'set-value', '--element-index', index, '--value', row['query'])
                            count = rf'^(?:{limit:,}|{limit}) rows retrieved(?: \(|$)'
                        else:
                            click(selector, r'button Connect or disconnect e2e-pg')
                            index, _ = find(selector, r'secure text field .*Password')
                            gui(selector, 'set-value', '--element-index', index, '--value', 'testpw')
                            click(selector, r'button Connect$')
                            find(selector, r'editor .*Query editor|text entry area .*Query editor')
                            subprocess.run(['/tmp/compare-probe', 'activate', str(p.pid)], check=True)
                            time.sleep(.2)
                            index, _ = find(selector, r'number field .*Row limit')
                            gui(selector, 'click', '--element-index', index)
                            gui(selector, 'hotkey', '--key', 'CmdOrCtrl+A')
                            gui(selector, 'type-text', '--text', str(limit))
                            subprocess.run(['/tmp/compare-probe', 'activate', str(p.pid)], check=True)
                            gui(selector, 'click', '--x', '650', '--y', '230')
                            gui(selector, 'hotkey', '--key', 'CmdOrCtrl+A')
                            gui(selector, 'type-text', '--text', row['query'])
                            count = rf'^{limit:,}$'
                        _, initial = find(selector, r'(?:editor|text entry area).*' + re.escape(row['query']))
                        row['gui_before'] = initial
                        assert f'(pid {p.pid})' in initial, 'GUI recipient differs from measured PID'
                        editor_lines = [line for line in initial.splitlines() if re.match(r'\s*\d+ (editor|text entry area) ', line) and ('Editor content' in line or 'Query editor' in line)]
                        assert len(editor_lines) == 1, f'Editor missing or ambiguous: {editor_lines}'
                        actual = editor_lines[0].split('Value: ', 1)[1].split(', Secondary Actions:', 1)[0] if app == 'Tabularis' else editor_lines[0].split('Query editor ', 1)[1].split(', Secondary Actions:', 1)[0]
                        assert actual == row['query'], f'Editor input not verified: {actual!r}'
                        assert FIRST not in initial, 'Stale results visible'
                        if app == 'AxDB':
                            assert re.search(rf'Row limit.*\b{limit}\b', initial), 'Row limit not verified'
                        row['builders_before_query'] = build_processes()
                        row['load_before_query'] = os.getloadavg()[0]
                        assert not row['builders_before_query'] and row['load_before_query'] <= 8, 'Gate invalidated before query'
                        cpu_before = processes()
                        cpu_start = time.monotonic()
                        raw = WORK / 'w9-query-probe.json'
                        subprocess.run(['/tmp/compare-probe', 'query', str(p.pid), FIRST, count, str(raw)], check=True, stdout=subprocess.DEVNULL, timeout=75)
                        elapsed = time.monotonic() - cpu_start
                        row.update(json.loads(raw.read_text()), measured=True)
                        after = processes()
                        ids = [pid for pid, v in after.items() if pid == p.pid or (pid not in before and 'com.apple.WebKit' in v['comm'])]
                        row.update(rss_kib=sum(after[pid]['rss_kib'] for pid in ids), processes=[dict(pid=pid, **after[pid]) for pid in ids], cpu_interval_seconds=elapsed, cpu_delta_seconds=sum(after[pid]['cpu_seconds'] - cpu_before.get(pid, {}).get('cpu_seconds', 0) for pid in ids), load_after=os.getloadavg()[0], builders_after=build_processes())
                        row['cpu_percent'] = row['cpu_delta_seconds'] / elapsed * 100
                        row['footprint'] = subprocess.run(['footprint', *[arg for pid in ids for arg in ('-p', str(pid))], '--noCategories'], text=True, capture_output=True, timeout=30).stdout
                        row['valid'] = row['first_ms'] is not None and row['count_ms'] is not None and not row['builders_after'] and row['load_after'] <= 8
                        row['reason'] = 'observed' if row['valid'] else 'observation_timeout_or_gate_invalidated'
                    except Exception as error:
                        row.update(reason='preparation_or_probe_failed', error=str(error))
                    finally:
                        if p is not None:
                            p.terminate()
                            try:
                                p.wait(timeout=10)
                            except subprocess.TimeoutExpired:
                                p.kill()
                                p.wait()
                        if app == 'Tabularis' and backup.exists():
                            if TAB.exists():
                                shutil.rmtree(TAB)
                            shutil.move(backup, TAB)
                        save(row)
                        print(stage, limit, query_id, run, app, row['valid'], row.get('first_ms'), row.get('count_ms'), row.get('error', ''), flush=True)
                        time.sleep(2)
    assert all(hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest() == hashes[app] for app, exe, _ in apps), 'Measured binary changed'


if __name__ == '__main__':
    main()

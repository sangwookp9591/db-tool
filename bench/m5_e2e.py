#!/usr/bin/env python3
"""Run the three existing desktop regressions in an isolated M5 app/config only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time
import resource_opt as r2

ROOT = Path(__file__).resolve().parent.parent


def main(app):
    app = Path(app).resolve()
    assert app.is_relative_to(ROOT/'target-m5') and app.suffix == '.app'
    executable = app/'Contents/MacOS/axdb'
    config = Path(tempfile.mkdtemp(prefix='axdb-resource-m5-e2e-')).resolve()
    fixtures = json.loads((ROOT/'bench/fixtures.json').read_text())
    assert fixtures['table']['fields']['database'] == 'axdb_test'
    assert fixtures['table']['fields']['port'] == '55432'
    assert fixtures['graph']['fields']['uri'] == 'bolt://127.0.0.1:57687'
    database = config/'fixture.sqlite'
    with sqlite3.connect(database) as db:
        db.execute('CREATE TABLE items(id INTEGER PRIMARY KEY, value TEXT)')
        db.executemany('INSERT INTO items VALUES(?,?)', [(i, f'item-{i}') for i in range(1, 101)])
    def profile(id, name, driver, fields):
        return dict(id=id, name=name, driver_id=driver, fields=fields, save_secret=True, access_mode='read_write')
    profiles = [profile('e2e-pg-0001', 'e2e-pg', 'postgres', fixtures['table']['fields']),
                profile('e2e-neo-0001', 'e2e-neo4j', 'neo4j', fixtures['graph']['fields']),
                profile('e2e-sqlite-0001', 'e2e-sqlite', 'sqlite', dict(path=str(database))),
                profile('e2e-pg-badport-0001', 'e2e-pg-badport', 'postgres', {**fixtures['table']['fields'], 'port':'1'})]
    (config/'profiles.json').write_text(json.dumps(profiles))
    (config/'profiles.json').chmod(0o600)
    env = {**os.environ, 'AXDB_CONFIG_DIR':str(config), 'AXDB_AGENT_BRIDGE':'1', 'AXDB_M5_E2E':'1', 'AXDB_DRIVER_DIR':str(ROOT/'target-m5/debug')}
    for key in ['AXDB_BENCH', 'AXDB_BENCH_TARGET', 'AXDB_DEV_SECRET_STORE', 'AXDB_M5_QUICK']:
        env.pop(key, None)
    result = dict(config=str(config), app=str(app), binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                  credentials='isolated debug fixture seam in RAM; does not verify OS keychain', checks=[])
    output = ROOT/'bench/results-m5-e2e.json'
    assert not output.exists()
    app_process = subprocess.Popen([str(executable)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    result['pid'] = app_process.pid
    try:
        until = time.monotonic()+60
        while not (config/'agent-bridge.json').exists():
            assert app_process.poll() is None
            assert time.monotonic() < until, 'Isolated desktop bridge did not become ready'
            time.sleep(.1)
        env['AXDB_APP_PID'] = str(app_process.pid)
        for name in ['e2e-bridge', 'preview-e2e', 'permissions-e2e']:
            started = time.monotonic()
            done = subprocess.run(['node', str(ROOT/f'bench/{name}.mjs')], env=env, capture_output=True, text=True, timeout=240)
            log = done.stdout+'\n'+done.stderr
            for fixture in fixtures.values():
                log = log.replace(fixture['secret'], '[redacted]')
            path = ROOT/f'bench/results-m5-{name}.txt'
            path.write_text(log)
            result['checks'].append(dict(name=name, exit=done.returncode, seconds=time.monotonic()-started, log=str(path.relative_to(ROOT))))
            output.write_text(json.dumps(result, indent=2))
            print(name, 'exit', done.returncode, flush=True)
    finally:
        children = {pid:p for pid,p in r2.processes().items() if p['ppid']==app_process.pid and p['command'].startswith(str(ROOT/'target-m5/debug'))}
        app_process.terminate()
        try:
            app_process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app_process.kill(); app_process.wait()
        deadline = time.monotonic()+10
        while any(pid in r2.processes() for pid in children) and time.monotonic()<deadline:
            time.sleep(.2)
        remaining = r2.processes()
        for pid, process in children.items():
            if pid in remaining and remaining[pid]['command']==process['command']:
                os.kill(pid, signal.SIGTERM)
        result['exit_verified'] = app_process.poll() is not None and not any(pid in r2.processes() for pid in children)
        output.write_text(json.dumps(result, indent=2))
    assert len(result['checks']) == 3 and all(check['exit'] == 0 for check in result['checks']) and result['exit_verified'], 'Desktop regressions failed; inspect the redacted evidence logs'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app')
    main(parser.parse_args().app)

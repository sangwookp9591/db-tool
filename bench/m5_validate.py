#!/usr/bin/env python3
"""M5 isolated workspace validation with existing test-fixture secrets redacted."""
import ast
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent
source = ROOT/'target-m5/source'
# Read constants without executing W1's SSH setup or accessing its hosts.
tree = ast.parse((ROOT/'scripts/test-network-run.py').read_text())
keys = {'AXDB_TEST_PG', 'AXDB_TEST_MONGO', 'AXDB_TEST_NEO4J', 'AXDB_TEST_REDIS'}
fixtures = {node.arg:node.value.value for node in ast.walk(tree) if isinstance(node, ast.keyword)
            and node.arg in keys and isinstance(node.value, ast.Constant)}
assert set(fixtures) == keys
assert 'host=127.0.0.1 port=55432 ' in fixtures['AXDB_TEST_PG'] and 'dbname=axdb_test' in fixtures['AXDB_TEST_PG']
assert fixtures['AXDB_TEST_NEO4J'].startswith('bolt://127.0.0.1:57687|')
assert urlparse(fixtures['AXDB_TEST_MONGO']).port == 57017
assert urlparse(fixtures['AXDB_TEST_REDIS']).port == 56379
secrets = [word.split('=',1)[1] for word in fixtures['AXDB_TEST_PG'].split() if word.startswith('password=')]
secrets += [fixtures['AXDB_TEST_NEO4J'].split('|')[2], urlparse(fixtures['AXDB_TEST_MONGO']).password, urlparse(fixtures['AXDB_TEST_REDIS']).password]
env = {**os.environ, **fixtures, 'CARGO_TARGET_DIR':str(ROOT/'target-m5'), 'CARGO_BUILD_JOBS':'6',
       'CARGO_PROFILE_DEV_DEBUG':'0', 'CARGO_PROFILE_DEV_INCREMENTAL':'false', 'AXDB_DRIVER_DIR':str(ROOT/'target-m5/debug'),
       'TAURI_CONFIG':json.dumps(dict(identifier='dev.axdb.m5probe', build=dict(devUrl=None, frontendDist=str(ROOT/'target-m5/final-ui'))))}
checks = []
host_only = '--host-only' in sys.argv
jobs = [('rust', ['cargo','test','--workspace'], source),
                           ('clippy', ['cargo','clippy','--workspace','--all-targets','--','-D','warnings'], source),
                           ('tsc', ['pnpm','--dir','ui','exec','tsc','--noEmit'],ROOT),
                           ('ui', ['pnpm','--dir','ui','test'],ROOT)]
if host_only:
    jobs = [('host-final', ['cargo','test','-p','axdb','--lib'],source), ('clippy-final',['cargo','clippy','-p','axdb','--all-targets','--','-D','warnings'],source), ('tsc-final',['pnpm','--dir','ui','exec','tsc','--noEmit'],ROOT), ('ui-final',['pnpm','--dir','ui','test'],ROOT)]
for name, command, cwd in jobs:
    since = time.monotonic()
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    path = ROOT/f'bench/results-m5-{name}.txt'
    with path.open('w') as log:
        for line in process.stdout:
            for secret in secrets:
                if secret:
                    line = line.replace(secret, '[redacted]')
            log.write(line); log.flush()
    code = process.wait()
    checks.append(dict(name=name, command=command, cwd=str(cwd), exit=code, seconds=time.monotonic()-since, log=str(path.relative_to(ROOT))))
    (ROOT/('bench/results-m5-focused-validation.json' if host_only else 'bench/results-m5-validation.json')).write_text(json.dumps(checks, indent=2))
    print(name, 'exit', code, flush=True)

import json, os, subprocess, sys, tempfile, time
from pathlib import Path
bundle = Path(sys.argv[1]); stop_at = int(sys.argv[2]); out = Path(sys.argv[3]); out.mkdir(exist_ok=True)
config = Path(tempfile.mkdtemp(prefix='axdb-leak-'))
env = {**os.environ, 'AXDB_CONFIG_DIR': str(config), 'AXDB_AGENT_BRIDGE': '0', 'AXDB_BENCH': '1', 'AXDB_BENCH_TARGET': 'resource-leak'}
app = subprocess.Popen([str(bundle / 'Contents/MacOS/axdb')], env=env, stdout=open(config/'log','w'), stderr=subprocess.STDOUT)
def ack(v):
    (config/'resource-ack.tmp').write_text(str(v)); (config/'resource-ack.tmp').replace(config/'resource-ack')
def fp():
    r = subprocess.run(['footprint', str(app.pid)], capture_output=True, text=True).stdout
    return r
seen = 0; t0 = time.monotonic()
try:
    while time.monotonic() - t0 < 1800:
        assert app.poll() is None
        try: st = json.loads((config/'perf.json').read_text())
        except Exception: time.sleep(.05); continue
        if 'complete' in st: print('complete', st.get('complete')); break
        if st['checkpoint'] != seen:
            seen = st['checkpoint']
            if seen == 1: subprocess.run(['/usr/bin/open', '-a', str(bundle)])
            s, i = st['stage'], st['iteration']
            if not s.startswith('prepare-'):
                f = fp(); line = next(l for l in f.splitlines() if 'Footprint:' in l)
                print(s, i, line.split('Footprint:')[1].strip(), flush=True)
                if s == 'repeat-query' and i in (1, stop_at):
                    (out/f'footprint-{i}.txt').write_text(f)
                    (out/f'vmmap-{i}.txt').write_text(subprocess.run(['vmmap', '-summary', str(app.pid)], capture_output=True, text=True).stdout)
                    (out/f'heap-{i}.txt').write_text(subprocess.run(['heap', '-s', str(app.pid)], capture_output=True, text=True).stdout)
                    if i == stop_at: ack('abort'); time.sleep(3); break
            ack(seen)
        time.sleep(.05)
finally:
    app.terminate()
    try: app.wait(10)
    except Exception: app.kill()

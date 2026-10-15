#!/usr/bin/env python3
"""Three supplemental pairs/state; load gate waits total at most 40 minutes."""
import json, os, pathlib, shutil, subprocess, sys, time
from compare_gate import wait_gate, build_processes, require_test_profiles
root=pathlib.Path(__file__).resolve().parent
work=pathlib.Path('/tmp/axdb-compare')
tab=pathlib.Path.home()/'Library/Application Support/tabularis'
backup=work/'tabularis-preserved'
output=root/'results-compare-idle-gated.json'
assert len(sys.argv)==3, 'compare_idle.py TABULARIS_EXE AXDB_EXE'
assert not backup.exists(), 'Restore/review preserved settings before rerunning'
assert not output.exists(), 'Archive the previous supplemental output before rerunning'
require_test_profiles(work/'tabularis-connected',work/'axdb-connected')
results=[];wait_used=0.0;stamp=str(time.time_ns())

def save(row):
 results.append(row);output.write_text(json.dumps(results,indent=2))

for state in ('empty','connected'):
 for run in range(1,4):
  remaining=2400-wait_used
  if remaining<=0:
   gate={'pair':f'idle-{state}-{run}','passed':False,'waited_seconds':0,'reason':'total_gate_wait_budget_exhausted'}
  else:
   gate=wait_gate(f'idle-{state}-{run}',timeout=min(600,remaining));wait_used+=gate['waited_seconds']
  for app,exe,marker in [('Tabularis',sys.argv[1],'Connections'),('AxDB',sys.argv[2],'CONNECTIONS')]:
   meta={'gate':gate,'app':app,'state':state,'run':run,'gate_wait_used_seconds':wait_used}
   if not gate['passed']:
    save(dict(meta,measured=False,reason=gate.get('reason','load_or_build_gate_timeout')));continue
   builders=build_processes()
   if builders:
    save(dict(meta,measured=False,reason='build_started_after_gate',builders=builders));continue
   pre=subprocess.check_output(['ps','-axo','pid=,ppid=,pcpu=,comm='],text=True)
   with (root/'results-compare-idle-gated.txt').open('a') as log:
    log.write(f'\n{state} {run} {app}\n'+subprocess.check_output(['uptime'],text=True)+pre)
   env=os.environ.copy()
   try:
    if app=='Tabularis':
     shutil.move(str(tab),backup);tab.mkdir()
     if state=='connected':
      for name in ('connections.json','config.json'):shutil.copy2(work/'tabularis-connected'/name,tab/name)
    else:
     cfg=work/f'axdb-gated-{stamp}-{state}-{run}';cfg.mkdir()
     if state=='connected':shutil.copy2(work/'axdb-connected/profiles.json',cfg/'profiles.json')
     env['AXDB_CONFIG_DIR']=str(cfg)
    raw=work/f'results-compare-idle-gated-{state}-{run}-{app.lower()}.json'
    subprocess.run(['/tmp/compare-probe','launch',exe,marker,str(raw)],env=env,check=True,stdout=subprocess.DEVNULL)
    row=json.loads(raw.read_text());after=build_processes()
    row.update(meta,measured=True,builders_after=after,valid_no_build=not after)
    save(row);print(state,run,app,row['window_ms'],row['ax_marker_ms'],flush=True)
   finally:
    if app=='Tabularis' and backup.exists():
     if tab.exists():shutil.rmtree(tab)
     shutil.move(backup,tab)
   time.sleep(3)

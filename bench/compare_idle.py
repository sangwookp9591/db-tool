#!/usr/bin/env python3
"""Three supplemental pairs/state; load gate waits total at most 40 minutes."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, time
from compare_gate import wait_gate, build_processes, require_test_profiles
root=pathlib.Path(__file__).resolve().parent
work=pathlib.Path('/tmp/axdb-compare')
tab=pathlib.Path.home()/'Library/Application Support/tabularis'
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('tabularis_exe');parser.add_argument('axdb_exe')
parser.add_argument('--p1-after', help='Compare before/after AxDB and Tabularis without overwriting W8 data')
args=parser.parse_args();p1=args.p1_after is not None;suffix='p1' if p1 else 'gated'
for exe in [args.tabularis_exe,args.axdb_exe,args.p1_after]:
 if exe is not None:assert pathlib.Path(exe).is_file(), f'Executable not found: {exe}'
backup=work/('tabularis-preserved-p1' if p1 else 'tabularis-preserved')
output=root/f'results-compare-idle-{suffix}.json'
assert not backup.exists(), 'Restore/review preserved settings before rerunning'
assert not output.exists(), 'Archive the previous supplemental output before rerunning'
require_test_profiles(work/'tabularis-connected',work/'axdb-connected')
results=[];wait_used=0.0;stamp=str(time.time_ns())
apps=[('Tabularis',args.tabularis_exe,'Connections'),('AxDB-before' if p1 else 'AxDB',args.axdb_exe,'CONNECTIONS')]
if p1:apps.append(('AxDB-after',args.p1_after,'CONNECTIONS'))
hashes={exe:hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest() for _,exe,_ in apps}

def save(row):
 results.append(row);output.write_text(json.dumps(results,indent=2))

for state in ('empty','connected'):
 for run in range(1,4):
  remaining=2400-wait_used
  if remaining<=0:
   gate={'pair':f'idle-{state}-{run}','passed':False,'waited_seconds':0,'reason':'total_gate_wait_budget_exhausted'}
  else:
   gate=wait_gate(f'{"p1-" if p1 else ""}idle-{state}-{run}',timeout=min(600,remaining));wait_used+=gate['waited_seconds']
  for app,exe,marker in (apps[::-1] if p1 and run%2==0 else apps):
   meta={'gate':gate,'app':app,'state':state,'run':run,'gate_wait_used_seconds':wait_used,'load_before':os.getloadavg()[0],'binary_sha256':hashes[exe]}
   if not gate['passed']:
    save(dict(meta,measured=False,reason=gate.get('reason','load_or_build_gate_timeout')));continue
   builders=build_processes()
   if builders:
    save(dict(meta,measured=False,reason='build_started_after_gate',builders=builders));continue
   pre=subprocess.check_output(['ps','-axo','pid=,ppid=,pcpu=,comm='],text=True)
   with (root/f'results-compare-idle-{suffix}.txt').open('a') as log:
    log.write(f'\n{state} {run} {app}\n'+subprocess.check_output(['uptime'],text=True)+pre)
   env=os.environ.copy()
   env.pop('AXDB_BENCH',None);env.pop('AXDB_BENCH_TARGET',None);env.pop('TOKIO_WORKER_THREADS',None)
   env['AXDB_AGENT_BRIDGE']='0'
   restore_safe=True
   try:
    if app=='Tabularis':
     shutil.move(str(tab),backup);tab.mkdir()
     if state=='connected':
      for name in ('connections.json','config.json'):shutil.copy2(work/'tabularis-connected'/name,tab/name)
    else:
     cfg=work/f'axdb-{suffix}-{stamp}-{state}-{run}-{app.lower()}';cfg.mkdir()
     if state=='connected':shutil.copy2(work/'axdb-connected/profiles.json',cfg/'profiles.json')
     env['AXDB_CONFIG_DIR']=str(cfg)
    raw=work/f'results-compare-idle-{suffix}-{state}-{run}-{app.lower()}.json'
    restore_safe=False
    subprocess.run(['/tmp/compare-probe','launch',exe,marker,str(raw)],env=env,check=True,stdout=subprocess.DEVNULL)
    restore_safe=True
    row=json.loads(raw.read_text());after=build_processes()
    row.update(meta,measured=True,builders_after=after,valid_no_build=not after,load_after=os.getloadavg()[0])
    row['valid_load']=row['load_before']<=8 and row['load_after']<=8
    save(row);print(state,run,app,row['window_ms'],row['ax_marker_ms'],flush=True)
   except BaseException as error:
    save(dict(meta,measured=False,reason='probe_or_preparation_failed',error=str(error),backup_preserved=app=='Tabularis' and backup.exists() and not restore_safe))
    raise
   finally:
    if app=='Tabularis' and backup.exists() and restore_safe:
     if tab.exists():shutil.rmtree(tab)
     shutil.move(backup,tab)
   time.sleep(3)

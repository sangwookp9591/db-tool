"""Record load <=8 and absence of build processes before an A/B pair."""
import datetime, json, os, pathlib, subprocess, time

def require_test_profiles(tab_dir,ax_dir):
 tab=json.loads((tab_dir/'connections.json').read_text())['connections']
 ax=json.loads((ax_dir/'profiles.json').read_text())
 if len(tab)!=1 or len(ax)!=1:
  raise SystemExit('Benchmark requires exactly one test PG profile per app')
 for driver,fields in [(tab[0]['params']['driver'],tab[0]['params']),(ax[0]['driver_id'],ax[0]['fields'])]:
  if driver!='postgres' or fields.get('host')!='127.0.0.1' or fields.get('port') not in (55432,'55432') or fields.get('database')!='dbtool_test':
   raise SystemExit('Benchmark target must be PostgreSQL 127.0.0.1:55432/dbtool_test')

def build_processes():
 rows=[]
 for line in subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines():
  pid,comm=line.strip().split(maxsplit=1)
  if pathlib.Path(comm).name in ('cargo','rustc','vite'):
   rows.append({'pid':int(pid),'comm':comm})
 return rows

def wait_gate(label,timeout=600,output=None):
 start=time.monotonic();samples=[]
 while True:
  load=os.getloadavg()[0];builders=build_processes();elapsed=time.monotonic()-start
  samples.append({'time':datetime.datetime.now().astimezone().isoformat(),'load1':load,'waited_seconds':elapsed,'builders':builders})
  print(f'gate {label}: load1={load:.2f}, builds={len(builders)}, waited={elapsed:.0f}s',flush=True)
  if (load<=8 and not builders) or elapsed>=timeout:break
  time.sleep(min(30,timeout-elapsed))
 result={'pair':label,'passed':load<=8 and not builders,'waited_seconds':elapsed,'threshold':8,'timeout_seconds':timeout,'samples':samples}
 path=pathlib.Path(output) if output else pathlib.Path(__file__).resolve().parent/'results-compare-gates.json'
 rows=json.loads(path.read_text()) if path.exists() else [];rows.append(result);path.write_text(json.dumps(rows,indent=2))
 return result

if __name__=='__main__':
 import sys
 wait_gate(sys.argv[1])

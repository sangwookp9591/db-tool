#!/usr/bin/env python3
"""GUI setup and paired query probes; test PostgreSQL only. Run after compare_idle.py."""
import atexit, hashlib, json, os, pathlib, re, subprocess, sys, time
from compare_gate import wait_gate, require_test_profiles
root=pathlib.Path(__file__).resolve().parent
work=pathlib.Path('/tmp/axdb-compare')
tab=pathlib.Path.home()/'Library/Application Support/tabularis'
assert len(sys.argv)==3, 'compare_queries.py TABULARIS_EXE AXDB_EXE'

def gui(app,*args):
 operation=subprocess.run(['orca','computer',*args,'--app',app,'--restore-window','--no-screenshot','--json'],text=True,capture_output=True)
 r=json.loads(operation.stdout)
 if not r.get('ok') and args[0]=='get-app-state' and r.get('error',{}).get('code') in ('app_not_found','window_not_found'):return ''
 if not r.get('ok'): raise RuntimeError(r)
 return r['result'].get('snapshot',{}).get('treeText','')
def find(app,pattern,timeout=30):
 start=time.monotonic()
 while time.monotonic()-start<timeout:
  tree=gui(app,'get-app-state')
  for line in tree.splitlines():
   m=re.match(r'\s*(\d+) (.*)',line)
   if m and re.search(pattern,m[2]):return m[1],tree
  time.sleep(.2)
 raise RuntimeError(f'{app}: missing {pattern}: {tree}')
def click(app,pattern):
 index,_=find(app,pattern);return gui(app,'click','--element-index',index)
def ps():
 out={}
 for line in subprocess.check_output(['ps','-axo','pid=,rss=,comm='],text=True).splitlines():
  p,r,c=line.strip().split(maxsplit=2);out[int(p)]={'rss_kib':int(r),'comm':c}
 return out
require_test_profiles(tab,work/'axdb-connected')
saved_config=(tab/'config.json').read_bytes()
atexit.register(lambda: (tab/'config.json').write_bytes(saved_config))
rows=json.loads((root/'results-compare-queries.json').read_text()) if (root/'results-compare-queries.json').exists() else []
limit='default';query_id='big';query='SELECT * FROM e2e.big'
for run in range(1,6):
 if all(any(r['app']==app and r['limit']==limit and r['query_id']==query_id and r['run']==run for r in rows) for app in ('Tabularis','AxDB')):continue
 gate=wait_gate(f'query-{limit}-{query_id}-{run}')
 for app,exe,selector in [('Tabularis',sys.argv[1],'tabularis'),('AxDB',sys.argv[2],'dev.axdb.app')]:
  if any(r['app']==app and r['limit']==limit and r['query_id']==query_id and r['run']==run for r in rows):continue
  before=ps(); env=os.environ.copy()
  assert not any(pathlib.Path(v['comm']).name in ('cargo','rustc') for v in before.values())
  with (root/'results-compare-queries.txt').open('a') as log:
   log.write(f'\n{limit} {query_id} {run} {app}\n'+subprocess.check_output(['uptime'],text=True)+subprocess.check_output(['ps','-axo','pid=,ppid=,pcpu=,comm='],text=True))
  if app=='Tabularis':
   config=json.loads((tab/'config.json').read_text());config.update(resultPageSize=500,autoConnectLastConnection=False,lastOpenConnectionIds=[],lastActiveConnectionId=None)
   (tab/'config.json').write_text(json.dumps(config))
  else:env['AXDB_CONFIG_DIR']=str(work/'axdb-connected')
  p=subprocess.Popen([exe],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  try:
   find(selector,r'standard window')
   subprocess.run(['open','-a',str(pathlib.Path(exe).parents[2])],check=True)
   if app=='Tabularis':
    # Dismiss modal if it appears, before connecting.
    _,tree=find(selector,r'button Connect$')
    if 'button Maybe later' in tree:click(selector,r'button Maybe later$')
    click(selector,r'button Connect$');find(selector,r'editor .*Editor content')
    click(selector,r'button New Console$')
    index,_=find(selector,r'editor .*Editor content, Secondary Actions')
    gui(selector,'set-value','--element-index',index,'--value',query)
    count=r'[0-9,]+ rows retrieved'
   else:
    click(selector,r'button Connect or disconnect e2e-pg')
    find(selector,r'editor .*Query editor|text entry area .*Query editor')
    subprocess.run(['open','-a',str(pathlib.Path(exe).parents[2])],check=True)
    gui(selector,'click','--x','650','--y','230')
    gui(selector,'hotkey','--key','CmdOrCtrl+A')
    gui(selector,'type-text','--text',query)
    count=r'^1,000$'
   initial=gui(selector,'get-app-state')
   editor_lines=[line for line in initial.splitlines() if re.match(r'\s*\d+ (editor|text entry area) ',line) and ('Editor content' in line or 'Query editor' in line)]
   assert len(editor_lines)==1, f'Editor missing or ambiguous: {editor_lines}'
   actual=editor_lines[0].split('Value: ',1)[1].split(', Secondary Actions:',1)[0] if app=='Tabularis' else editor_lines[0].split('Query editor ',1)[1].split(', Secondary Actions:',1)[0]
   assert actual==query, f'Editor input not verified: {actual!r}'
   assert 'c4ca4238a0b923820dcc509a6f75849b' not in initial, 'Stale result must not be visible'
   out=work/'query-probe.json'
   subprocess.run(['/tmp/compare-probe','query',str(p.pid),'c4ca4238a0b923820dcc509a6f75849b',count,str(out)],check=True,stdout=subprocess.DEVNULL)
   result=json.loads(out.read_text());after=ps();ids=[pid for pid,v in after.items() if pid==p.pid or (pid not in before and 'com.apple.WebKit' in v['comm'])]
   result.update(gate=gate,app=app,limit=limit,query=query,query_id=query_id,run=run,gui_before=initial,rss_kib=sum(after[pid]['rss_kib'] for pid in ids),processes=[dict(pid=pid,**after[pid]) for pid in ids],uptime=subprocess.check_output(['uptime'],text=True),footprint=subprocess.run(['footprint',*[arg for pid in ids for arg in ('-p',str(pid))],'--noCategories'],text=True,capture_output=True).stdout)
   result['valid']=result['first_ms'] is not None and result['count_ms'] is not None
   if not result['valid']:
    result.update(scroll_seconds=None,scroll_bottom_observed=False,scroll_reason='Query observation timed out')
    rows.append(result);(root/'results-compare-queries.json').write_text(json.dumps(rows,indent=2))
    print(limit,query_id,run,app,'observation timeout',flush=True)
    continue
   try:
    expected=500 if app=='Tabularis' else 1000
    bottom=hashlib.md5(str(expected).encode()).hexdigest()
    subprocess.run(['open','-a',str(pathlib.Path(exe).parents[2])],check=True)
    scroll_start=time.monotonic()
    scroll_tree=gui(selector,'scroll','--x','500','--y','500','--direction','down','--pages','10000')
    for _ in range(10):
     if bottom in scroll_tree:break
     time.sleep(.1);scroll_tree=gui(selector,'get-app-state')
    result.update(scroll_seconds=(time.monotonic()-scroll_start) if bottom in scroll_tree else None,scroll_bottom_observed=bottom in scroll_tree,scroll_final_tree=scroll_tree)
   except RuntimeError as error:
    result.update(scroll_seconds=None,scroll_bottom_observed=False,scroll_reason=str(error))
   rows.append(result);(root/'results-compare-queries.json').write_text(json.dumps(rows,indent=2))
   print(limit,query_id,run,app,result['first_ms'],result['count_ms'],flush=True)
  finally:
   p.terminate();p.wait(timeout=10);time.sleep(2)

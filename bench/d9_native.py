import json,os,pathlib,re,subprocess,time,tempfile,uuid
root=pathlib.Path(__file__).resolve().parents[1]
config=pathlib.Path(tempfile.mkdtemp(prefix='axdb-d9-'))
fixture=json.loads((root/'bench/fixtures.json').read_text())['table']
assert fixture['fields']['host']=='127.0.0.1' and fixture['fields']['port']=='55432'
assert ':55432' in subprocess.check_output(['docker','port','dbtool-test-pg','5432'],text=True)
fields=fixture['fields'];table='d9_approval_'+uuid.uuid4().hex[:12]
def sql(text):
 return subprocess.check_output(['docker','exec','dbtool-test-pg','psql','-U',fields['user'],'-d',fields['database'],'-v','ON_ERROR_STOP=1','-tA','-c',text],env={**os.environ,'PGPASSWORD':fixture['secret']},text=True).strip()
profile=dict(id='d9-test',name='D9 test',driver_id='postgres',fields=fields,save_secret=False,access_mode='read_write',network=None,auth=dict(kind='password'))
(config/'profiles.json').write_text(json.dumps([profile]))
(config/'ai.json').write_text(json.dumps(dict(enabled=True,provider='anthropic',model='claude-opus-5-5',effort='medium',auth='api_key')))
(config/'fixture.md').write_text('검토할 SQL 파일입니다.\n\n'+ '\n\n'.join(f'```sql migrations/00{i}.sql\nSELECT {i} AS proposal;\n```' for i in range(1,4)))
env={**os.environ,'AXDB_CONFIG_DIR':str(config),'AXDB_AI_FIXTURE':str(config/'fixture.md'),'AXDB_DRIVER_DIR':str(root/'target-d7/debug'),'AXDB_BENCH':'1','AXDB_BENCH_TARGET':'d9','AXDB_AGENT_BRIDGE':'0'}
log=(config/'native.log').open('w');process=None;native_pointer=[]
print(json.dumps(dict(config=str(config),table=table)),flush=True)
try:
 sql(f'CREATE TABLE {table}(id integer); INSERT INTO {table} VALUES(1),(2)')
 process=subprocess.Popen([str(root/'target-d7/debug/axdb')],env=env,stdout=log,stderr=subprocess.STDOUT)
 subprocess.run([str(root/'target-d7/d9-input'),str(process.pid),'focus'],check=True)
 seen=0;deadline=time.monotonic()+240
 while time.monotonic()<deadline:
  time.sleep(.2)
  try: report=json.loads((config/'perf.json').read_text())
  except (FileNotFoundError,json.JSONDecodeError):
   if process.poll() is not None: raise RuntimeError('native app exited before report')
   continue
  if report.get('checkpoint',0)>seen:
   seen=report['checkpoint'];stage=report['stage'];print(stage,flush=True)
   if stage.startswith('d9-select-file-'):
    subprocess.run([str(root/'target-d7/d9-input'),str(process.pid),'save',str(config)+'/',f'00{stage[-1]}.sql'],check=True)
   elif stage=='d9-keyboard-undo': subprocess.run([str(root/'target-d7/d9-input'),str(process.pid),'undo'],check=True)
   elif stage in ['d9-fuse-arm','d9-fuse-run','d9-fuse-cancel']:
    action=stage.removeprefix('d9-');value=subprocess.check_output([str(root/'target-d7/d9-input'),str(process.pid),action],text=True).strip()
    if value:native_pointer.append(json.loads(value))
   elif stage.startswith('d9-fixture-'):
    name=stage.removeprefix('d9-fixture-')
    if name=='read': proposal=dict(name='run_query',arguments=dict(text='SELECT 1 AS connection_check'),reason='테스트 연결의 읽기 쿼리를 확인합니다.')
    elif name=='delete': proposal=dict(name='run_query',arguments=dict(text=f'DELETE FROM {table}'),reason='이 실행에서 만든 테스트 테이블의 두 행을 삭제합니다.')
    elif name=='mcp': proposal=dict(name='lint_query',arguments=dict(query='SELECT 1'),client_id=report['client'],reason='저장된 읽기 에이전트 권한으로 MCP 린트를 호출합니다.')
    (config/'fixture.md').write_text('```tool\n'+json.dumps(proposal,ensure_ascii=False)+'\n```')
   else:
    wid=subprocess.check_output(['swift',str(root/'bench/d7_window.swift'),str(process.pid)],text=True).strip();assert wid,'Owned debug window missing'
    subprocess.run(['screencapture','-x','-l',wid,str(root/'docs/design-shots'/f'{stage}.png')],check=True)
    if stage.startswith(('d9-delete-preview-','d9-fuse-armed-','d9-fuse-cancelled-')):assert sql(f'SELECT count(*) FROM {table}')=='2','Preview committed a delete'
    if stage.startswith('d9-delete-trace-'):assert sql(f'SELECT count(*) FROM {table}')=='0','Apply did not delete test rows'
   (config/'resource-ack').write_text(str(seen))
  if report.get('complete') or report.get('error'):
   if report.get('error'):
    wid=subprocess.check_output(['swift',str(root/'bench/d7_window.swift'),str(process.pid)],text=True).strip()
    if wid:subprocess.run(['screencapture','-x','-l',wid,str(root/'docs/design-shots/d9-debug-error.png')])
   report['native_pointer']=native_pointer;report['test_db']=fields['host']+':'+fields['port']+'/'+fields['database'];report['no_automatic_file_write']=not any(config.glob('*.sql'))
   audit='\n'.join(p.read_text() for p in config.rglob('agent-audit.jsonl'))
   report['audit_sanitized']=bool(audit) and fixture['secret'] not in audit and 'SELECT 1 AS' not in audit and 'DELETE FROM' not in audit
   (root/'docs/design-shots/d9-native-result.json').write_text(json.dumps(report,ensure_ascii=False,indent=2));print(json.dumps(report,ensure_ascii=False),flush=True)
   assert report.get('complete') is True and report['audit_sanitized'] and report['no_automatic_file_write'],'D9 native checks failed'
   break
 else:raise TimeoutError('D9 native harness deadline')
finally:
 if process:
  process.terminate()
  try:process.wait(timeout=10)
  except subprocess.TimeoutExpired:process.kill()
 log.close();sql(f'DROP TABLE IF EXISTS {table}')

import json,os,pathlib,subprocess,time,tempfile
root=pathlib.Path(__file__).resolve().parents[1]
config=pathlib.Path(tempfile.mkdtemp(prefix='axdb-d7-'))
fixture=json.loads((root/'bench/fixtures.json').read_text())['table']
assert fixture['fields']['host']=='127.0.0.1' and fixture['fields']['port']=='55432'
profile=dict(id='d7-test',name='D7 test',driver_id='postgres',fields=fixture['fields'],save_secret=False,access_mode='read_write',network=None,auth=dict(kind='password'))
(config/'profiles.json').write_text(json.dumps([profile]))
(config/'ai.json').write_text(json.dumps(dict(enabled=False,provider='anthropic',model='claude-opus-5-5',effort='medium',auth='api_key')))
(config/'fixture.md').write_text('## 쿼리 제안\n\n현재 연결의 스키마를 참고했습니다. 다음 쿼리는 실행하지 않았습니다.\n\n```sql\nSELECT 1 AS connection_check;\n```\n\n적용 후 편집기에서 내용을 검토하세요. 자동 저장과 실행은 하지 않습니다.\n')
env={**os.environ,'AXDB_CONFIG_DIR':str(config),'AXDB_AI_FIXTURE':str(config/'fixture.md'),'AXDB_DRIVER_DIR':str(root/'target-d7/debug'),'AXDB_BENCH':'1','AXDB_BENCH_TARGET':'d7','AXDB_AGENT_BRIDGE':'0'}
log=(config/'native.log').open('w');process=subprocess.Popen([str(root/'target-d7/debug/axdb')],env=env,stdout=log,stderr=subprocess.STDOUT)
print(json.dumps(dict(config=str(config),pid=process.pid)),flush=True)
seen=0;deadline=time.monotonic()+180
try:
 while time.monotonic()<deadline:
  time.sleep(.3)
  try: report=json.loads((config/'perf.json').read_text())
  except (FileNotFoundError,json.JSONDecodeError):
   if process.poll() is not None: raise RuntimeError('native app exited before report')
   continue
  if report.get('checkpoint',0)>seen:
   seen=report['checkpoint'];stage=report['stage'];wid=subprocess.check_output(['swift',str(root/'bench/d7_window.swift'),str(process.pid)],text=True).strip()
   assert wid, 'Owned debug window not found for capture'
   subprocess.run(['screencapture','-x','-l',wid,str(root/'docs/design-shots'/f'{stage if stage.startswith("d8-") else "d7-"+stage}.png')],check=True)
   (config/'resource-ack').write_text(str(seen))
  if report.get('complete') or report.get('error'):
   if report.get('error'):
    wid=subprocess.check_output(['swift',str(root/'bench/d7_window.swift'),str(process.pid)],text=True).strip()
    if wid: subprocess.run(['screencapture','-x','-l',wid,str(root/'docs/design-shots/d7-debug-error.png')])
   (root/'docs/design-shots/d7-native-result.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
   print(json.dumps(report,ensure_ascii=False),flush=True)
   assert report.get('complete') is True, 'D7 native checks failed; see d7-native-result.json'
   break
 else: raise TimeoutError('D7 native harness deadline')
finally:
 process.terminate()
 try: process.wait(timeout=10)
 except subprocess.TimeoutExpired: process.kill()
 log.close()

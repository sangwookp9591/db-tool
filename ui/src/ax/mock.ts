import type { AiSettings } from '../ai/types';
import type { EditRequest, ChangePreview, Value } from '../contract';
import type { ImportPlan, Snapshot, SnapshotDiff } from './managementContract';
import { specKey, type Pipeline, type PreviewReport, type RunEntry, type RunReport, type Step } from './etlContract';
let settings:AiSettings={enabled:false,model:'claude-opus-5-5',effort:'medium',auth:'api_key',provider:'anthropic'};
let loggedIn=false,hasKey=false;
const logins:Record<string,boolean>={anthropic:false,openai:false},keys=new Set<string>();
const previews=new Set<string>();
const preview=(text:string):ChangePreview=>({committed:false,notes:['모의 미리보기 · 실제 DB에 접속하지 않았습니다.'],statements:[{text,kind:'Other',affected:3,before:null,after:null,schema_diff:null,counters:[],warnings:[]}]});
let etlPipelines:Pipeline[]=[],etlRuns:RunEntry[]=[];
const etlReceipts=new Set<string>();
const etlInput:{name:string;type_name:string}[]=[{name:'id',type_name:'integer'},{name:'name',type_name:'text'},{name:'team',type_name:'text'}];
const etlRows:Value[][]=Array.from({length:60},(_,i)=>[{t:'Int',v:String(i+1)},{t:'Text',v:` 사용자 ${i+1} `},i%7===0?{t:'Null'}:{t:'Text',v:['개발','디자인','운영'][i%3]}]);
function etlApply(steps:Step[],columns:{name:string;type_name:string}[],rows:Value[][]){
 let cols=columns,data=rows,filtered=0;
 const at=(name:string)=>cols.findIndex(c=>c.name===name);
 for(const step of steps){
  if(step.op==='select'){const idx=step.columns.map(c=>at(c.from)).filter(i=>i>=0);data=data.map(r=>idx.map(i=>r[i]));cols=step.columns.filter(c=>at(c.from)>=0).map(c=>({name:c.to||c.from,type_name:cols[at(c.from)].type_name}));}
  else if(step.op==='rename'){const i=at(step.from);if(i>=0)cols=cols.map((c,j)=>j===i?{...c,name:step.to}:c);}
  else if(step.op==='drop'){const keep=cols.map((c,i)=>step.columns.includes(c.name)?-1:i).filter(i=>i>=0);data=data.map(r=>keep.map(i=>r[i]));cols=keep.map(i=>cols[i]);}
  else if(step.op==='constant'){const i=at(step.column);if(i>=0)data=data.map(r=>r.map((v,j)=>j===i?step.value:v));else{cols=[...cols,{name:step.column,type_name:'text'}];data=data.map(r=>[...r,step.value]);}}
  else if(step.op==='text'){const i=at(step.column),f=(t:string)=>step.func==='trim'?t.trim():step.func==='upper'?t.toUpperCase():t.toLowerCase();if(i>=0)data=data.map(r=>r.map((v,j)=>j===i&&v.t==='Text'?{t:'Text',v:f(v.v)}:v));}
  else if(step.op==='filter'){const i=at(step.column),text=(v:Value)=>v.t==='Null'?null:String(v.v);if(i>=0){const before=data.length;data=data.filter(r=>{const t=text(r[i]),w=step.value?text(step.value):null;switch(step.cmp){case 'is_null':return t===null;case 'not_null':return t!==null;case 'eq':return t!==null&&t===w;case 'ne':return t!==null&&t!==w;case 'contains':return t!==null&&w!==null&&t.includes(w);case 'starts_with':return t!==null&&w!==null&&t.startsWith(w);default:{if(t===null||w===null)return false;const a=Number(t),b=Number(w),c=Number.isNaN(a)||Number.isNaN(b)?t.localeCompare(w):a-b;return step.cmp==='lt'?c<0:step.cmp==='le'?c<=0:step.cmp==='gt'?c>0:c>=0;}}});filtered+=before-data.length;}}
 }
 return{cols,data,filtered};
}
export const axMockCommands=['import_target','ai_login_status','ai_login_submit','snapshot_encode','snapshot_decode','ai_settings','ai_configure','ai_provider_login_start','ai_provider_login_poll','ai_provider_login_cancel','ai_provider_logout','ai_account_select','ai_models','ai_operation','ai_diagnose_connection','import_plan','import_preview','import_apply','schema_snapshot','schema_diff','ddl_render','migration_scan','migration_preview','migration_apply','etl_list','etl_save','etl_delete','etl_runs','etl_preview','etl_run'];
export async function axMock(command:string,args:Record<string,unknown>):Promise<unknown>{
 switch(command){
 case 'import_target':if(!String(args.name??'').trim())throw new Error('테이블 이름 필요');return `demo-new:${args.namespace??'public'}.${args.name}`;
 case 'ai_login_status':return{installed:true,logged_in:logins.anthropic,account:logins.anthropic?'모의 계정':null};
 case 'ai_login_submit':logins.anthropic=true;loggedIn=settings.provider!=='openai';return{logged_in:true,account:'모의 계정'};
 case 'snapshot_encode':return JSON.stringify(args.snapshot,null,2);
 case 'snapshot_decode':{const parsed=JSON.parse(new TextDecoder().decode(new Uint8Array(args.bytes as number[])));if(parsed.version!==1||typeof parsed.dialect!=='string'||!Array.isArray(parsed.objects)||parsed.objects.some((o:Record<string,unknown>)=>typeof o.name!=='string'||!Array.isArray(o.columns)))throw new Error('유효하지 않은 스냅숏');return parsed;}
 case 'ai_settings':return{settings,has_key:hasKey,has_ant_cli:true,usage:{anthropic:{input_tokens:0,output_tokens:0},openai:{input_tokens:0,output_tokens:0}},accounts:logins.openai?[{id:'demo',label:'모의 계정 · 외부 로그인 없음',active:true,logged_in:true}]:[]};
 case 'ai_configure':settings=args.settings as AiSettings;if(args.apiKey!==null){if(args.apiKey)keys.add(settings.provider??'anthropic');else keys.delete(settings.provider??'anthropic');}hasKey=keys.has(settings.provider??'anthropic');loggedIn=logins[settings.provider??'anthropic'];return;
 case 'ai_provider_login_start':if(args.provider==='openai')logins.openai=true;loggedIn=logins[settings.provider??'anthropic'];return{url:null,waiting_for_code:args.provider==='anthropic'};
 case 'ai_provider_login_poll':return{state:logins[String(args.provider)]?'done':'waiting',account:logins[String(args.provider)]?'모의 계정':null};
 case 'ai_provider_login_cancel':return;
 case 'ai_provider_logout':logins[String(args.provider)]=false;if(!args.accountId)keys.delete(String(args.provider));loggedIn=logins[settings.provider??'anthropic'];hasKey=keys.has(settings.provider??'anthropic');return{logged_in:false,remote_revoked:true};
 case 'ai_account_select':return;
 case 'ai_models':{const provider=args.provider??settings.provider;const id=provider==='openai'?'gpt-6.1-sol':'claude-opus-5-5';return[{id,name:`${id} · 모의 모델`,efforts:['low','medium','high','xhigh','max'],status:logins[String(provider)]||keys.has(String(provider))?'connected':'disconnected',cached:false,auth:keys.has(String(provider))?'api_key':provider==='anthropic'?'claude_login':'chatgpt_login'}];}
 case 'ai_operation':if(!loggedIn&&!hasKey)throw new Error('로그인 필요');return{text:'모의 제안입니다. 스키마를 확인하고 실행 전에 검토하세요.',usage:{input_tokens:0,output_tokens:0},operations:[{id:crypto.randomUUID(),kind:args.purpose==='recommend_indexes'?'migrate':'explain',connection:args.sessionId,params:{},plan:{reason:'전체 스캔 후보를 확인합니다.',evidence:['people 스키마 · 모의','통계 10,000행 · 모의'],statements:args.purpose==='recommend_indexes'?['CREATE INDEX people_team_idx ON people (team_id)']:['SELECT id, name FROM people LIMIT 100']},risk:'write',needs_approval:true}]};
 case 'ai_diagnose_connection':return{text:'모의 진단: 실패한 단계의 호스트·포트와 자격 증명을 확인하세요. 비밀값을 전송하거나 TLS 검증을 끄지 마세요.'};
 case 'import_plan':{const bytes=args.bytes as number[];if(bytes.length>8*1024*1024)throw new Error('파일이 너무 큽니다.');const raw=new TextDecoder().decode(new Uint8Array(bytes));let records:Record<string,unknown>[];try{const parsed=JSON.parse(raw);records=Array.isArray(parsed)?parsed:[parsed];}catch{throw new Error('브라우저 모의 가져오기는 JSON 배열만 지원합니다. CSV/TSV/NDJSON/XLSX 파서는 네이티브 호스트에서 검증하세요.');}if(!records.length||records.some(row=>!row||typeof row!=='object'||Array.isArray(row)))throw new Error('JSON 객체 배열이 필요합니다.');const columns=(args.mapping as ImportPlan['columns']|undefined)??Object.keys(records[0]).map(source=>({source,target:source,type_name:'text'}));const value=(v:unknown):Value=>v==null?{t:'Null'}:{t:'Text',v:typeof v==='object'?JSON.stringify(v):String(v)};return{format:'JSON · 모의',columns,sample:records.slice(0,100).map(row=>columns.map(c=>value(row[c.source]))),row_count:records.length,errors:[],warnings:['모의 매핑: 네이티브 타입 변환·제약 검증이 아닙니다.'],request:{node_id:args.nodeId,...(args.createTable?{create_table:columns.map(c=>({name:c.target,type_name:c.type_name}))}:{}),edits:records.map(row=>({op:'Insert',values:columns.map(c=>[c.target,value(row[c.source])])}))}};}
 case 'import_preview':{const req=args.req as EditRequest;previews.add(JSON.stringify(req));return preview(`INSERT · ${req.node_id} · ${req.edits.length}행`);}
 case 'import_apply':{const req=args.req as EditRequest;if(!previews.delete(JSON.stringify(req)))throw new Error('먼저 미리보기하세요.');return{affected:req.edits.map(()=>1),statements:['모의 적용 · 실제 DB 변경 없음']};}
 case 'schema_snapshot':return{version:1,dialect:'sql-demo',objects:[{namespace:'public',name:String(args.sessionId).includes('1')?'orders':'people',kind:'table',definition:`CREATE TABLE ${String(args.sessionId).includes('1')?'orders':'people'} (id INTEGER, name TEXT)`,columns:[{name:'id',type_name:'integer',nullable:false,default:null},{name:'name',type_name:'text',nullable:true,default:null}]}]} satisfies Snapshot;
 case 'schema_diff':{const before=args.before as Snapshot,after=args.after as Snapshot;return{dialect:before.dialect,changes:[...before.objects.filter(o=>!after.objects.some(n=>n.name===o.name)).map(o=>({action:'remove',before:o,after:null,risk:'destructive'})),...after.objects.filter(o=>!before.objects.some(n=>n.name===o.name)).map(o=>({action:'add',before:null,after:o,risk:'write'}))]};}
 case 'ddl_render':return(args.diff as SnapshotDiff).changes.map(c=>c.action==='remove'?`DROP TABLE "${c.before?.name}"`:c.after?.definition??'');
 case 'migration_scan':return{migrations:[{id:'001',name:'001_create_people.sql',format:'plain',text:'CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT);',checksum:'demo-initial',applied:true},{id:'002',name:'002_people_team_index.sql',format:'plain',text:'CREATE INDEX people_team_idx ON people (team_id);',checksum:'demo-pending',applied:false}],history_tables:['axdb_migration_history'],warnings:['모의 폴더입니다. 로컬 파일을 읽지 않았습니다.']};
 case 'migration_preview':previews.add(String(args.text));return preview(String(args.text));
 case 'migration_apply':if(!previews.delete(String(args.text)))throw new Error('먼저 미리보기하세요.');return[];
 case 'etl_list':return structuredClone(etlPipelines);
 case 'etl_save':{const p=structuredClone(args.pipeline as Pipeline);if(!p.name.trim())throw new Error('파이프라인 이름은 1..200바이트여야 합니다');if(!p.id)p.id=crypto.randomUUID();const i=etlPipelines.findIndex(x=>x.id===p.id);if(i>=0)etlPipelines[i]=p;else etlPipelines.push(p);return structuredClone(p);}
 case 'etl_delete':etlPipelines=etlPipelines.filter(p=>p.id!==args.id);return null;
 case 'etl_runs':return etlRuns.slice(0,Number(args.limit??50));
 case 'etl_preview':{const p=args.pipeline as Pipeline,limit=Math.min(Number(args.sampleRows??200),etlRows.length),source=etlRows.slice(0,limit),out=etlApply(p.steps,etlInput,source);etlReceipts.add(`${args.sourceSessionId}:${args.targetSessionId}:${specKey(p)}`);const rows=out.data;return{input_columns:etlInput,output_columns:out.cols,sample:rows,rows_read:source.length,rows_filtered:out.filtered,rows_rejected:0,rejected:[],target:preview(`INSERT · ${p.target.node_id} · ${rows.length}행 (표본)`),source_truncated:false,warnings:['모의 미리보기: 샘플 데이터로 일부 단계만 근사 계산합니다. 실제 DB에 접속하지 않았습니다.']} satisfies PreviewReport;}
 case 'etl_run':{const p=args.pipeline as Pipeline,key=`${args.sourceSessionId}:${args.targetSessionId}:${specKey(p)}`;if(!etlReceipts.delete(key))throw new Error('먼저 미리보기하세요. 미리보기는 60초 동안 한 번만 사용할 수 있습니다.');const out=etlApply(p.steps,etlInput,etlRows),batch=p.options.batch_size,written=Math.min(out.data.length,p.options.max_rows??Infinity),report:RunReport={status:'completed',phase:'done',rows_read:etlRows.length,rows_written:written,rows_filtered:out.filtered,rows_rejected:0,batches_committed:Math.ceil(written/batch),rejected:[],error:null,source_truncated:false,elapsed_ms:12};const{rejected:_r,error:_e,...rest}=report;etlRuns=[{pipeline_id:p.id,pipeline_name:p.name,run_id:String(args.runId),finished_at:new Date().toISOString(),...rest},...etlRuns].slice(0,500);return report;}
 default:throw new Error(`지원되지 않는 모의 작업: ${command}`);
 }
}

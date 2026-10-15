// Real desktop bridge; isolated SQLite fixture only. Never prints client credentials.
import assert from 'node:assert/strict';
import {readFile,writeFile,rename} from 'node:fs/promises';
import {randomUUID,randomBytes,createHash} from 'node:crypto';
const dir=process.env.AXDB_CONFIG_DIR;
assert(dir,'Set AXDB_CONFIG_DIR to an isolated test config');
const profiles=JSON.parse(await readFile(`${dir}/profiles.json`,'utf8'));
const profile=profiles.find(p=>p.id==='ai1-sqlite');
assert(profile?.driver_id==='sqlite'&&profile.fields.path.startsWith('/tmp/axdb-ai1-'),'AI1 isolated SQLite required');
const discovery=JSON.parse(await readFile(`${dir}/agent-bridge.json`,'utf8'));
let seq=0;
async function rpc(token,method,params={}) {
 const response=await fetch(discovery.url,{method:'POST',headers:{'content-type':'application/json',authorization:`Bearer ${token}`},body:JSON.stringify({jsonrpc:'2.0',id:++seq,method,params}),signal:AbortSignal.timeout(30000)});
 const value=await response.json();assert(!value.error,JSON.stringify(value.error));return value.result;
}
async function tool(token,name,args={}) {const value=await rpc(token,'tools/call',{name,arguments:args});return value.structuredContent??value;}
async function put(clients) {const temp=`${dir}/ai1-${randomUUID()}.tmp`;await writeFile(temp,JSON.stringify(clients),{mode:0o600});await rename(temp,`${dir}/agent_clients.json`);}
const checks=[];const check=async(name,run)=>{await run();checks.push(name);console.log(`PASS ${name}`)};
const token=randomBytes(32).toString('hex'),salt=randomUUID(),id=randomUUID();
let session;let clients;
try {
 const local=discovery.token;
 session=(await tool(local,'open_connection',{profile_id:profile.id})).session_id;assert(session);
 await check('native MCP schema uses manifest, entities and FK without rows',async()=>{const context=await tool(local,'get_schema_context',{session_id:session,focus:'ai_child'});assert(context.schema.includes('ai_child'));assert(context.schema.includes('foreign_key'));assert.equal(context.query_language.id,'sql');assert.equal(context.selected_rows,null);assert(context.schema.length<=6000);});
 await tool(local,'open_query_tab',{session_id:session,text:'SELECT id FROM ai_child'});
 await check('native work context reads active editor without execution',async()=>{const context=await tool(local,'get_work_context');assert.equal(context.text,'SELECT id FROM ai_child');assert.equal(context.session_id,session);assert.equal(context.selected_rows,null);});
 await check('error tool resolves failed run and related schema',async()=>{await tool(local,'run_query',{session_id:session,text:'SELECT id FROM ai_child',read_only:true});const result=await tool(local,'run_query',{session_id:session,text:'SELECT * FROM ai_missing_table',read_only:true});assert(result.error);assert(result.run);const context=await tool(local,'explain_error',{run:result.run});assert.equal(context.error.kind,'Query');assert(context.schema.includes('ai_child'));assert.equal(context.results[0].columns[0].name,'id');});
 clients=JSON.parse(await readFile(`${dir}/agent_clients.json`,'utf8'));
 const client={id,name:'AI1 test read client',token_salt:salt,token_hash:createHash('sha256').update(`${salt}:${token}`).digest('hex'),connections:{[profile.id]:'read'},allowed_tools:['get_schema_context','get_work_context','explain_error'],max_rows:3,expires_at:null,revoked:false};
 clients.push(client);await put(clients);
 await check('read client sees read permission and no write grant',async()=>{const context=await tool(token,'get_schema_context',{session_id:session});assert.equal(context.permissions.client_grant,'read');assert.equal(context.permissions.write,false);});
 client.connections[profile.id]='none';await put(clients);
 await check('none hides context tools and prevents all context access',async()=>{const list=await rpc(token,'tools/list');assert(!list.tools.some(t=>client.allowed_tools.includes(t.name)));for(const [name,args] of [['get_schema_context',{session_id:session}],['get_work_context',{}],['explain_error',{run:'unknown'}]]){const result=await tool(token,name,args);assert(result.error);assert(!result.schema);}});
 await check('context arguments cannot opt into row export',async()=>{assert((await tool(local,'get_schema_context',{session_id:session,include_rows:true})).error);});
 await writeFile('bench/results-ai1-native.json',JSON.stringify({source:'target-ai1/debug/axdb native MCP + isolated SQLite, no provider call',checks},null,2));
} finally {if(clients)await put(clients.filter(c=>c.id!==id));if(session)await tool(discovery.token,'close_connection',{session_id:session});}

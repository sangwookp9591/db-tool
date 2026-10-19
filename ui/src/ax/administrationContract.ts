export interface BackupManifest { version:number;format:string;sha256:string;bytes:number;counts:[string,number][] }
export interface BackupStatus {run_id:string;phase:string;completed:number;total:number;manifest:BackupManifest|null}
export interface GrantChange {role:string;namespace:string;object:string;privilege:string;revoke:boolean}
export interface GrantPlan {statements:string[];warnings:string[];apply_supported:boolean}
export interface RolesSnapshot {supported:boolean;reason:string|null;roles:{name:string;login:boolean;superuser:boolean;member_of:string[];summary:string[]}[];matrix:{role:string;namespace:string;object:string;privilege:string;allowed:boolean}[]}
export interface RestorePreview {manifest:BackupManifest;risk:'write'|'destructive';needs_approval:boolean;warnings:string[]}
declare module '../api' {interface Commands {
 backup_create:{args:{sessionId:string;runId:string};result:{manifest:BackupManifest;bytes:number[]}};
 backup_status:{args:{sessionId:string};result:BackupStatus};
 restore_preview:{args:{sessionId:string;bytes:number[];overwrite:boolean};result:RestorePreview};
 restore_apply:{args:{sessionId:string;runId:string;bytes:number[];overwrite:boolean;approved:boolean;confirmation?:string};result:void};
 roles:{args:{sessionId:string};result:RolesSnapshot};
 plan_grants:{args:{sessionId:string;changes:GrantChange[]};result:GrantPlan};
 grants_preview:{args:{sessionId:string;changes:GrantChange[]};result:GrantPlan};
 grants_apply:{args:{sessionId:string;runId:string;changes:GrantChange[];approved:boolean};result:void};
}}
export async function administrationMock(command:string,args:Record<string,unknown>):Promise<unknown>{
 if(command==='roles')return {supported:true,reason:null,roles:['analyst','editor'].map(name=>({name,login:true,superuser:false,member_of:[],summary:['모의 사용자 · 실제 DB에 접속하지 않았습니다.']})),matrix:['SELECT','UPDATE'].flatMap(privilege=>['analyst','editor'].map(role=>({role,namespace:'public',object:'people',privilege,allowed:privilege==='SELECT'||role==='editor'})))};
 if(command==='plan_grants'||command==='grants_preview')return {statements:(args.changes as GrantChange[]).map(c=>`MOCK ${c.revoke?'REVOKE':'GRANT'} ${c.privilege} ${c.namespace}.${c.object} ${c.role}`),warnings:['모의 계획 · 실제 DB를 변경하지 않습니다.'],apply_supported:true};
 if(command==='grants_apply')return;
 if(command==='backup_status')return {run_id:'',phase:'idle',completed:0,total:0,manifest:null};
 throw {kind:'Unsupported',message:'브라우저 모의 모드에서는 실제 백업 파일과 복원을 제공하지 않습니다.'};
}

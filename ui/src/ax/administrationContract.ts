export interface BackupManifest { version:number;format:string;sha256:string;bytes:number;counts:[string,number][];warnings?:string[] }
export interface BackupStatus {run_id:string;phase:string;completed:number;total:number;manifest:BackupManifest|null}
export interface GrantChange {role:string;namespace:string;object:string;privilege:string;revoke:boolean}
export interface GrantPlan {statements:string[];warnings:string[];apply_supported:boolean}
export interface RolesPageQuery {role?:string|null;namespace?:string|null;object?:string|null;offset:number;limit:number}
export interface RolesPageInfo extends RolesPageQuery {total:number;has_more:boolean;role_required:boolean}
export interface RolesSnapshot {page?:RolesPageInfo|null;supported:boolean;reason:string|null;roles:{name:string;login:boolean;superuser:boolean;member_of:string[];summary:string[]}[];matrix:{role:string;namespace:string;object:string;privilege:string;allowed:boolean}[]}
export interface RestorePreview {manifest:BackupManifest;risk:'write'|'destructive';needs_approval:boolean;warnings:string[]}
declare module '../api' {interface Commands {
 backup_create_file:{args:{sessionId:string;runId:string;path:string;token:string};result:BackupManifest};
 restore_options:{args:{sessionId:string};result:{overwrite:boolean;new_database:boolean;guidance:string}};
 restore_pick_file:{args:{sessionId:string;runId:string};result:{token:string;name:string;manifest:BackupManifest}|null};
 restore_file_preview:{args:{sessionId:string;token:string;overwrite:boolean;newDatabase?:string};result:RestorePreview};
 restore_file_apply:{args:{sessionId:string;runId:string;token:string;overwrite:boolean;newDatabase?:string;approved:boolean;confirmation?:string};result:void};
 restore_file_discard:{args:{sessionId:string;token:string};result:void};
 backup_create:{args:{sessionId:string;runId:string};result:{manifest:BackupManifest;bytes:number[]}};
 backup_status:{args:{sessionId:string};result:BackupStatus};
 restore_preview:{args:{sessionId:string;bytes:number[];overwrite:boolean};result:RestorePreview};
 restore_apply:{args:{sessionId:string;runId:string;bytes:number[];overwrite:boolean;approved:boolean;confirmation?:string};result:void};
 roles:{args:{sessionId:string};result:RolesSnapshot};
 roles_page:{args:{sessionId:string;query:RolesPageQuery};result:RolesSnapshot};
 plan_grants:{args:{sessionId:string;changes:GrantChange[]};result:GrantPlan};
 grants_preview:{args:{sessionId:string;changes:GrantChange[]};result:GrantPlan};
 grants_apply:{args:{sessionId:string;runId:string;changes:GrantChange[];approved:boolean};result:void};
}}
export async function administrationMock(command:string,args:Record<string,unknown>):Promise<unknown>{
 if(command==='roles_page'){const snapshot=await administrationMock('roles',args) as RolesSnapshot;const q=args.query as RolesPageQuery;const matches=snapshot.matrix.filter(cell=>cell.role===q.role&&cell.namespace.toLowerCase().includes((q.namespace??'').toLowerCase())&&cell.object.toLowerCase().includes((q.object??'').toLowerCase()));return {...snapshot,matrix:q.role?matches.slice(q.offset,q.offset+q.limit):[],page:{...q,total:matches.length,has_more:q.offset+q.limit<matches.length,role_required:!q.role}};}
 if(command==='roles')return {supported:true,reason:null,roles:['analyst','editor'].map(name=>({name,login:true,superuser:false,member_of:[],summary:['모의 사용자 · 실제 DB에 접속하지 않았습니다.']})),matrix:['SELECT','UPDATE'].flatMap(privilege=>['analyst','editor'].map(role=>({role,namespace:'public',object:'people',privilege,allowed:privilege==='SELECT'||role==='editor'})))};
 if(command==='plan_grants'||command==='grants_preview')return {statements:(args.changes as GrantChange[]).map(c=>`MOCK ${c.revoke?'REVOKE':'GRANT'} ${c.privilege} ${c.namespace}.${c.object} ${c.role}`),warnings:['모의 계획 · 실제 DB를 변경하지 않습니다.'],apply_supported:true};
 if(command==='grants_apply')return;
 if(command==='restore_options')return {overwrite:false,new_database:false,guidance:'모의 화면 · 실제 복원은 네이티브 앱에서 지원합니다.'};
 if(command==='restore_file_discard')return;
 if(command==='backup_status')return {run_id:'',phase:'idle',completed:0,total:0,manifest:null};
 throw {kind:'Unsupported',message:'브라우저 모의 모드에서는 실제 백업 파일과 복원을 제공하지 않습니다.'};
}

import type { DriverManifest } from '../contract';
export type AxPage = 'import'|'schema_diff'|'migrate'|'backup'|'grant'|'accounts'|'intent';
export const operationLabels: Record<AxPage,string> = {import:'가져오기',schema_diff:'스키마 비교',migrate:'마이그레이션',backup:'백업 · 복원',grant:'사용자 · 권한',accounts:'AI 계정',intent:'AI 작업 계획'};
export const supportsOperation=(key:AxPage,cap?:DriverManifest['capabilities'])=>key==='accounts'||key==='intent'||!!(key==='import'?cap?.import_rows:key==='schema_diff'?cap?.schema_snapshot&&cap?.ddl_render:key==='migrate'?cap?.ddl_render:key==='backup'?cap?.backup:cap?.roles);

import type { ChangePreview, Column, DriverManifest, Profile, Value } from '../contract';
export type CastType = 'text' | 'int' | 'float' | 'decimal' | 'bool' | 'date' | 'date_time' | 'json';
export type Cmp = 'eq' | 'ne' | 'lt' | 'le' | 'gt' | 'ge' | 'is_null' | 'not_null' | 'contains' | 'starts_with';
export type TextFunc = 'trim' | 'upper' | 'lower';
export type Step =
  | { op: 'select'; columns: { from: string; to: string }[] }
  | { op: 'rename'; from: string; to: string }
  | { op: 'drop'; columns: string[] }
  | { op: 'cast'; column: string; to: CastType }
  | { op: 'filter'; column: string; cmp: Cmp; value?: Value }
  | { op: 'constant'; column: string; value: Value }
  | { op: 'text'; column: string; func: TextFunc }
  | { op: 'fill_null'; column: string; value: Value }
  | { op: 'concat'; column: string; inputs: string[]; separator: string }
  | { op: 'dedupe'; columns: string[] };
export type StepOp = Step['op'];
export type Source = { kind: 'query'; profile_id: string; text: string } | { kind: 'table'; profile_id: string; node_id: string };
export interface Target { profile_id: string; node_id: string; create_table: boolean }
export interface Options { batch_size: number; max_rows: number | null; on_error: 'abort' | 'skip_row'; batch_timeout_ms: number }
export interface Pipeline { id: string; name: string; description: string; version: 1; source: Source; steps: Step[]; target: Target; options: Options }
export interface Progress { phase: 'extract' | 'load' | 'done'; rows_read: number; rows_written: number; rows_filtered: number; rows_rejected: number; batches_committed: number }
export type EtlProgressEvent = Progress & { run_id: string };
export interface RejectedRow { row: number; step: number; message: string }
export type RunStatus = 'completed' | 'failed' | 'cancelled';
export interface RunReport extends Progress { status: RunStatus; rejected: RejectedRow[]; error: string | null; source_truncated: boolean; elapsed_ms: number }
export interface RunEntry extends Progress { pipeline_id: string; pipeline_name: string; run_id: string; finished_at: string; status: RunStatus; source_truncated: boolean; elapsed_ms: number }
export interface PreviewReport { input_columns: Column[]; output_columns: Column[]; sample: Value[][]; rows_read: number; rows_filtered: number; rows_rejected: number; rejected: RejectedRow[]; target: ChangePreview | null; source_truncated: boolean; warnings: string[] }
declare module '../api' { interface Commands {
  etl_list: { args: Record<string, never>; result: Pipeline[] };
  etl_save: { args: { pipeline: Pipeline }; result: Pipeline };
  etl_delete: { args: { id: string }; result: null };
  etl_runs: { args: { limit?: number }; result: RunEntry[] };
  etl_preview: { args: { pipeline: Pipeline; sourceSessionId: string; targetSessionId: string; runId: string; sampleRows?: number }; result: PreviewReport };
  etl_run: { args: { pipeline: Pipeline; sourceSessionId: string; targetSessionId: string; runId: string }; result: RunReport };
} }

export const MAX_BATCH_ROWS = 10_000, MAX_STEPS = 64, MAX_NAME_BYTES = 200, MAX_QUERY_BYTES = 256 * 1024, MAX_TIMEOUT_MS = 3_600_000;
export const STEP_OPS: { value: StepOp; label: string }[] = [
  { value: 'select', label: '컬럼 선택' }, { value: 'rename', label: '이름 변경' }, { value: 'drop', label: '컬럼 제거' }, { value: 'cast', label: '형 변환' },
  { value: 'filter', label: '필터' }, { value: 'constant', label: '상수 컬럼' }, { value: 'text', label: '텍스트 변환' }, { value: 'fill_null', label: 'NULL 대체' },
  { value: 'concat', label: '컬럼 합치기' }, { value: 'dedupe', label: '중복 제거' }];
export const CAST_TYPES: { value: CastType; label: string }[] = [{ value: 'text', label: '텍스트' }, { value: 'int', label: '정수' }, { value: 'float', label: '실수' }, { value: 'decimal', label: '십진수' }, { value: 'bool', label: '참/거짓' }, { value: 'date', label: '날짜' }, { value: 'date_time', label: '날짜·시간' }, { value: 'json', label: 'JSON' }];
export const CMPS: { value: Cmp; label: string; symbol: string }[] = [{ value: 'eq', label: '같음', symbol: '=' }, { value: 'ne', label: '다름', symbol: '≠' }, { value: 'lt', label: '미만', symbol: '<' }, { value: 'le', label: '이하', symbol: '≤' }, { value: 'gt', label: '초과', symbol: '>' }, { value: 'ge', label: '이상', symbol: '≥' }, { value: 'contains', label: '포함', symbol: '포함' }, { value: 'starts_with', label: '시작', symbol: '시작' }, { value: 'is_null', label: '값이 없음', symbol: '' }, { value: 'not_null', label: '값이 있음', symbol: '' }];
export const TEXT_FUNCS: { value: TextFunc; label: string }[] = [{ value: 'trim', label: '앞뒤 공백 제거' }, { value: 'upper', label: '대문자' }, { value: 'lower', label: '소문자' }];
export const needsValue = (cmp: Cmp) => cmp !== 'is_null' && cmp !== 'not_null';

export const emptyPipeline = (): Pipeline => ({ id: '', name: '', description: '', version: 1, source: { kind: 'query', profile_id: '', text: '' }, steps: [], target: { profile_id: '', node_id: '', create_table: false }, options: { batch_size: 1000, max_rows: null, on_error: 'abort', batch_timeout_ms: 300_000 } });
export const newStep = (op: StepOp): Step => {
  switch (op) {
    case 'select': return { op, columns: [{ from: '', to: '' }] };
    case 'rename': return { op, from: '', to: '' };
    case 'drop': return { op, columns: [] };
    case 'cast': return { op, column: '', to: 'text' };
    case 'filter': return { op, column: '', cmp: 'eq', value: { t: 'Text', v: '' } };
    case 'constant': return { op, column: '', value: { t: 'Text', v: '' } };
    case 'text': return { op, column: '', func: 'trim' };
    case 'fill_null': return { op, column: '', value: { t: 'Text', v: '' } };
    case 'concat': return { op, column: '', inputs: [], separator: ' ' };
    case 'dedupe': return { op, columns: [] };
  }
};
const byteLength = (text: string) => new TextEncoder().encode(text).length;
export function valueText(value: Value | undefined): string { if (!value) return ''; if (value.t === 'Null') return 'NULL'; if (value.t === 'Bool') return String(value.v); return typeof value.v === 'object' ? JSON.stringify(value.v) : String(value.v); }
export type ValueKind = 'text' | 'int' | 'float' | 'bool' | 'null';
export const VALUE_KINDS: { value: ValueKind; label: string }[] = [{ value: 'text', label: '텍스트' }, { value: 'int', label: '정수' }, { value: 'float', label: '실수' }, { value: 'bool', label: '참/거짓' }, { value: 'null', label: 'NULL' }];
export const valueKind = (value: Value | undefined): ValueKind => value?.t === 'Int' ? 'int' : value?.t === 'Float' ? 'float' : value?.t === 'Bool' ? 'bool' : value?.t === 'Null' ? 'null' : 'text';
/** 입력 문자열을 지정한 종류의 값으로 만든다. 해석할 수 없으면 null. */
export function makeValue(kind: ValueKind, text: string): Value | null {
  switch (kind) {
    case 'text': return { t: 'Text', v: text };
    case 'null': return { t: 'Null' };
    case 'int': return /^-?\d+$/.test(text.trim()) ? { t: 'Int', v: text.trim() } : null;
    case 'float': { const n = Number(text.trim()); return text.trim() !== '' && Number.isFinite(n) ? { t: 'Float', v: n } : null; }
    case 'bool': return text.trim() === 'true' ? { t: 'Bool', v: true } : text.trim() === 'false' ? { t: 'Bool', v: false } : null;
  }
}
/** Rust `Pipeline::validate`와 같은 규칙. 첫 오류에서 멈추지 않고 모두 모은다. */
export function validatePipeline(p: Pipeline): string[] {
  const errors: string[] = [];
  const name = p.name.trim();
  if (!name || byteLength(name) > MAX_NAME_BYTES) errors.push('파이프라인 이름은 1..200바이트여야 합니다');
  if (p.source.kind === 'query') {
    if (!p.source.profile_id) errors.push('소스 연결을 선택하세요');
    if (!p.source.text.trim() || byteLength(p.source.text) > MAX_QUERY_BYTES) errors.push('소스 쿼리는 1바이트 이상 256 KiB 이하여야 합니다');
  } else if (!p.source.profile_id || !p.source.node_id) errors.push('소스 연결과 테이블을 선택하세요');
  if (!p.target.profile_id || !p.target.node_id) errors.push('대상 연결과 테이블을 선택하세요');
  if (p.steps.length > MAX_STEPS) errors.push('변환 단계는 64개 이하여야 합니다');
  const o = p.options;
  if (!Number.isInteger(o.batch_size) || o.batch_size < 1 || o.batch_size > MAX_BATCH_ROWS) errors.push('배치 크기는 1..10000행이어야 합니다');
  if (o.max_rows !== null && (!Number.isInteger(o.max_rows) || o.max_rows < 1)) errors.push('최대 행 수는 1 이상이어야 합니다');
  if (!Number.isInteger(o.batch_timeout_ms) || o.batch_timeout_ms < 1 || o.batch_timeout_ms > MAX_TIMEOUT_MS) errors.push('배치 제한 시간은 1ms..1시간이어야 합니다');
  p.steps.forEach((step, index) => {
    const prefix = `단계 ${index + 1}: `;
    if (step.op === 'filter' && needsValue(step.cmp) && !step.value) errors.push(`${prefix}비교할 값이 필요합니다`);
    const columns = step.op === 'select' ? step.columns.flatMap(c => [c.from]) : step.op === 'drop' || step.op === 'dedupe' ? step.columns : step.op === 'rename' ? [step.from, step.to] : step.op === 'concat' ? [step.column, ...step.inputs] : [step.column];
    if (!columns.length || columns.some(c => !c.trim()) || (step.op === 'select' && step.columns.some(c => !c.to.trim()))) errors.push(`${prefix}컬럼 이름을 입력하세요`);
  });
  return errors;
}
export function stepLabel(step: Step): string {
  switch (step.op) {
    case 'select': return `컬럼 선택 · ${step.columns.map(c => c.from === c.to || !c.to ? c.from : `${c.from} → ${c.to}`).join(', ') || '(미지정)'}`;
    case 'rename': return `이름 변경 · ${step.from} → ${step.to}`;
    case 'drop': return `컬럼 제거 · ${step.columns.join(', ') || '(미지정)'}`;
    case 'cast': return `형 변환 · ${step.column} → ${CAST_TYPES.find(t => t.value === step.to)?.label ?? step.to}`;
    case 'filter': { const c = CMPS.find(c => c.value === step.cmp); return needsValue(step.cmp) ? `필터 · ${step.column} ${c?.symbol} ${valueText(step.value)}` : `필터 · ${step.column} ${c?.label}`; }
    case 'constant': return `상수 컬럼 · ${step.column} = ${valueText(step.value)}`;
    case 'text': return `텍스트 변환 · ${step.column} ${TEXT_FUNCS.find(f => f.value === step.func)?.label}`;
    case 'fill_null': return `NULL 대체 · ${step.column} → ${valueText(step.value)}`;
    case 'concat': return `컬럼 합치기 · ${step.inputs.join(', ') || '(미지정)'} → ${step.column}`;
    case 'dedupe': return `중복 제거 · ${step.columns.join(', ') || '(미지정)'}`;
  }
}
type Caps = Partial<DriverManifest['capabilities']> & { import_rows?: boolean };
export const canBeSource = (kind: Source['kind'], cap?: Caps) => !!(kind === 'query' ? cap?.read_only_exec : cap?.paged_browse);
export const canBeTarget = (profile: Pick<Profile, 'access_mode'>, cap?: Caps) => profile.access_mode !== 'read_only' && !!cap?.import_rows;
/** 미리보기 영수증은 명세 전체에 묶인다. 같은 문자열이면 같은 명세다. */
export const specKey = (p: Pipeline) => JSON.stringify(p);
const applied = (r: Progress) => r.batches_committed > 0 ? `${r.batches_committed}개 배치(${r.rows_written}행)가 이미 반영되었습니다.` : '반영된 행이 없습니다.';
export function summarizeRun(r: RunReport): string {
  if (r.status === 'completed') {
    const extra = [r.rows_filtered ? `필터로 제외 ${r.rows_filtered}행` : '', r.rows_rejected ? `오류로 건너뜀 ${r.rows_rejected}행` : ''].filter(Boolean).join(' · ');
    return `완료: ${r.rows_written}행을 ${r.batches_committed}개 배치로 반영했습니다.${extra ? ` ${extra}` : ''}${r.source_truncated ? ' 쿼리 결과가 행 수 상한에서 잘렸습니다.' : ''}`;
  }
  if (r.status === 'cancelled') return `취소됨. ${applied(r)}`;
  return `실패: ${r.error ?? '알 수 없는 오류'}. ${applied(r)}`;
}
export const statusLabel = (s: RunStatus) => s === 'completed' ? '완료' : s === 'failed' ? '실패' : '취소됨';

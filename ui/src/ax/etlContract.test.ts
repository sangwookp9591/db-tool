import { describe, expect, it } from 'vitest';
import { canBeSource, canBeTarget, emptyPipeline, makeValue, newStep, specKey, stepLabel, summarizeRun, validatePipeline, type Pipeline, type RunReport } from './etlContract';
import { operationLabels, supportsOperation } from './operations';

const valid = (): Pipeline => ({ ...emptyPipeline(), name: '사용자 복사', source: { kind: 'query', profile_id: 'a', text: 'select 1' }, target: { profile_id: 'b', node_id: 'table:users', create_table: false } });
const report = (over: Partial<RunReport>): RunReport => ({ status: 'completed', phase: 'done', rows_read: 0, rows_written: 0, rows_filtered: 0, rows_rejected: 0, batches_committed: 0, rejected: [], error: null, source_truncated: false, elapsed_ms: 5, ...over });

describe('emptyPipeline', () => {
  it('uses the backend defaults', () => {
    const p = emptyPipeline();
    expect(p).toMatchObject({ id: '', version: 1, steps: [], options: { batch_size: 1000, max_rows: null, on_error: 'abort', batch_timeout_ms: 300000 } });
  });
  it('returns independent objects', () => { const a = emptyPipeline(); a.steps.push(newStep('dedupe')); expect(emptyPipeline().steps).toHaveLength(0); });
});

describe('validatePipeline', () => {
  it('accepts a complete pipeline', () => expect(validatePipeline(valid())).toEqual([]));
  it('rejects an empty or blank name', () => {
    expect(validatePipeline({ ...valid(), name: '' })).toContain('파이프라인 이름은 1..200바이트여야 합니다');
    expect(validatePipeline({ ...valid(), name: '   ' })).toHaveLength(1);
  });
  it('measures the name in bytes, not characters', () => {
    expect(validatePipeline({ ...valid(), name: 'a'.repeat(200) })).toEqual([]);
    expect(validatePipeline({ ...valid(), name: 'a'.repeat(201) })).toHaveLength(1);
    expect(validatePipeline({ ...valid(), name: '가'.repeat(67) })).toHaveLength(1);
    expect(validatePipeline({ ...valid(), name: '가'.repeat(66) })).toEqual([]);
  });
  it('requires source connection and query text', () => {
    expect(validatePipeline({ ...valid(), source: { kind: 'query', profile_id: '', text: 'select 1' } })).toContain('소스 연결을 선택하세요');
    expect(validatePipeline({ ...valid(), source: { kind: 'query', profile_id: 'a', text: '  ' } })).toContain('소스 쿼리는 1바이트 이상 256 KiB 이하여야 합니다');
  });
  it('requires table source connection and table', () => {
    expect(validatePipeline({ ...valid(), source: { kind: 'table', profile_id: 'a', node_id: '' } })).toContain('소스 연결과 테이블을 선택하세요');
    expect(validatePipeline({ ...valid(), source: { kind: 'table', profile_id: 'a', node_id: 'n' } })).toEqual([]);
  });
  it('requires the target', () => {
    const p = valid(); p.target.node_id = '';
    expect(validatePipeline(p)).toContain('대상 연결과 테이블을 선택하세요');
  });
  it('checks batch_size boundaries 0 / 1 / 10000 / 10001', () => {
    const at = (n: number) => { const p = valid(); p.options.batch_size = n; return validatePipeline(p); };
    expect(at(0)).toContain('배치 크기는 1..10000행이어야 합니다');
    expect(at(1)).toEqual([]);
    expect(at(10000)).toEqual([]);
    expect(at(10001)).toContain('배치 크기는 1..10000행이어야 합니다');
    expect(at(1.5)).toHaveLength(1);
  });
  it('checks max_rows and batch timeout', () => {
    const p = valid(); p.options.max_rows = 0;
    expect(validatePipeline(p)).toContain('최대 행 수는 1 이상이어야 합니다');
    p.options.max_rows = 1; expect(validatePipeline(p)).toEqual([]);
    p.options.batch_timeout_ms = 0; expect(validatePipeline(p)).toContain('배치 제한 시간은 1ms..1시간이어야 합니다');
    p.options.batch_timeout_ms = 3_600_001; expect(validatePipeline(p)).toHaveLength(1);
    p.options.batch_timeout_ms = 3_600_000; expect(validatePipeline(p)).toEqual([]);
  });
  it('requires a value for comparison filters but not for is_null / not_null', () => {
    const f = (cmp: 'eq' | 'is_null' | 'not_null' | 'contains', value?: ReturnType<typeof makeValue>) => { const p = valid(); p.steps = [{ op: 'filter', column: 'age', cmp, ...(value ? { value } : {}) }]; return validatePipeline(p); };
    expect(f('eq')).toEqual(['단계 1: 비교할 값이 필요합니다']);
    expect(f('contains')).toHaveLength(1);
    expect(f('is_null')).toEqual([]);
    expect(f('not_null')).toEqual([]);
    expect(f('eq', { t: 'Int', v: '1' })).toEqual([]);
  });
  it('numbers the failing step and flags empty column names', () => {
    const p = valid(); p.steps = [{ op: 'cast', column: 'a', to: 'int' }, { op: 'filter', column: 'a', cmp: 'gt' }];
    expect(validatePipeline(p)).toContain('단계 2: 비교할 값이 필요합니다');
    const q = valid(); q.steps = [{ op: 'cast', column: '', to: 'int' }];
    expect(validatePipeline(q)).toContain('단계 1: 컬럼 이름을 입력하세요');
  });
  it('limits steps to 64', () => {
    const p = valid(); p.steps = Array.from({ length: 65 }, () => ({ op: 'cast', column: 'a', to: 'int' }) as const);
    expect(validatePipeline(p)).toContain('변환 단계는 64개 이하여야 합니다');
    p.steps.pop(); expect(validatePipeline(p)).toEqual([]);
  });
  it('collects several problems at once', () => { expect(validatePipeline({ ...emptyPipeline() }).length).toBeGreaterThanOrEqual(3); });
});

describe('newStep / stepLabel / makeValue', () => {
  it('creates a step for every op that has a label', () => {
    for (const op of ['select', 'rename', 'drop', 'cast', 'filter', 'constant', 'text', 'fill_null', 'concat', 'dedupe'] as const) {
      const s = newStep(op); expect(s.op).toBe(op); expect(stepLabel(s)).toBeTruthy();
    }
  });
  it('describes steps in Korean', () => {
    expect(stepLabel({ op: 'rename', from: 'a', to: 'b' })).toBe('이름 변경 · a → b');
    expect(stepLabel({ op: 'cast', column: 'age', to: 'int' })).toBe('형 변환 · age → 정수');
    expect(stepLabel({ op: 'filter', column: 'age', cmp: 'ge', value: { t: 'Int', v: '18' } })).toBe('필터 · age ≥ 18');
    expect(stepLabel({ op: 'filter', column: 'age', cmp: 'is_null' })).toBe('필터 · age 값이 없음');
    expect(stepLabel({ op: 'drop', columns: ['a', 'b'] })).toBe('컬럼 제거 · a, b');
    expect(stepLabel({ op: 'drop', columns: [] })).toBe('컬럼 제거 · (미지정)');
  });
  it('builds typed values and rejects invalid numbers', () => {
    expect(makeValue('text', ' x ')).toEqual({ t: 'Text', v: ' x ' });
    expect(makeValue('int', '42')).toEqual({ t: 'Int', v: '42' });
    expect(makeValue('int', '4.2')).toBeNull();
    expect(makeValue('float', '4.5')).toEqual({ t: 'Float', v: 4.5 });
    expect(makeValue('float', 'abc')).toBeNull();
    expect(makeValue('bool', 'true')).toEqual({ t: 'Bool', v: true });
    expect(makeValue('bool', 'x')).toBeNull();
    expect(makeValue('null', '')).toEqual({ t: 'Null' });
  });
});

describe('capabilities and spec identity', () => {
  it('derives source/target support from capabilities only', () => {
    expect(canBeSource('query', { read_only_exec: true } as never)).toBe(true);
    expect(canBeSource('query', { paged_browse: true } as never)).toBe(false);
    expect(canBeSource('table', { paged_browse: true } as never)).toBe(true);
    expect(canBeSource('table', undefined)).toBe(false);
    expect(canBeTarget({ id: 'p' } as never, { import_rows: true } as never)).toBe(true);
    expect(canBeTarget({ id: 'p', access_mode: 'read_only' } as never, { import_rows: true } as never)).toBe(false);
    expect(canBeTarget({ id: 'p' } as never, {} as never)).toBe(false);
  });
  it('changes specKey on any edit', () => {
    const a = valid(), b = valid(); expect(specKey(a)).toBe(specKey(b));
    b.options.batch_size = 999; expect(specKey(a)).not.toBe(specKey(b));
  });
  it('registers the workbench page', () => {
    expect(operationLabels.etl).toBe('데이터 파이프라인');
    expect(supportsOperation('etl', { import_rows: true } as never)).toBe(true);
    expect(supportsOperation('etl', { paged_browse: true } as never)).toBe(true);
    expect(supportsOperation('etl', { read_only_exec: true } as never)).toBe(true);
    expect(supportsOperation('etl', {} as never)).toBe(false);
    expect(supportsOperation('etl', undefined)).toBe(false);
  });
});

describe('summarizeRun', () => {
  it('summarises a completed run', () => {
    expect(summarizeRun(report({ rows_read: 120, rows_written: 100, rows_filtered: 15, rows_rejected: 5, batches_committed: 2 }))).toBe('완료: 100행을 2개 배치로 반영했습니다. 필터로 제외 15행 · 오류로 건너뜀 5행');
    expect(summarizeRun(report({ rows_written: 3, batches_committed: 1 }))).toBe('완료: 3행을 1개 배치로 반영했습니다.');
  });
  it('mentions truncated query sources', () => {
    expect(summarizeRun(report({ rows_written: 1, batches_committed: 1, source_truncated: true }))).toContain('쿼리 결과가 행 수 상한에서 잘렸습니다');
  });
  it('tells how many batches were already applied when a run fails', () => {
    expect(summarizeRun(report({ status: 'failed', error: '연결 끊김', rows_written: 2000, batches_committed: 2 }))).toBe('실패: 연결 끊김. 2개 배치(2000행)가 이미 반영되었습니다.');
  });
  it('says nothing was applied when a run fails before the first commit', () => {
    expect(summarizeRun(report({ status: 'failed', error: '변환 오류' }))).toBe('실패: 변환 오류. 반영된 행이 없습니다.');
    expect(summarizeRun(report({ status: 'failed', error: null }))).toBe('실패: 알 수 없는 오류. 반영된 행이 없습니다.');
  });
  it('reports cancellation with committed batches', () => {
    expect(summarizeRun(report({ status: 'cancelled', rows_written: 500, batches_committed: 1 }))).toBe('취소됨. 1개 배치(500행)가 이미 반영되었습니다.');
    expect(summarizeRun(report({ status: 'cancelled' }))).toBe('취소됨. 반영된 행이 없습니다.');
  });
});

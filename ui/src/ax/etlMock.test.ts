import { describe, expect, it } from 'vitest';
import { axMock } from './mock';
import { emptyPipeline, type PreviewReport, type RunReport } from './etlContract';
const p = () => ({ ...emptyPipeline(), name: 't', steps: [{ op: 'filter', column: 'team', cmp: 'eq', value: { t: 'Text', v: '개발' } }, { op: 'text', column: 'name', func: 'trim' }] as never });
const ids = { sourceSessionId: 's', targetSessionId: 't', runId: 'r' };
describe('etl mock backend', () => {
  it('saves, lists and deletes pipelines', async () => {
    const saved = await axMock('etl_save', { pipeline: p() }) as { id: string };
    expect(saved.id).not.toBe('');
    expect((await axMock('etl_list', {}) as unknown[]).length).toBeGreaterThan(0);
    await axMock('etl_delete', { id: saved.id });
    expect((await axMock('etl_list', {}) as { id: string }[]).some(x => x.id === saved.id)).toBe(false);
    await expect(axMock('etl_save', { pipeline: { ...p(), name: ' ' } })).rejects.toThrow();
  });
  it('requires a matching preview before a single run', async () => {
    await expect(axMock('etl_run', { pipeline: p(), ...ids })).rejects.toThrow('먼저 미리보기');
    const preview = await axMock('etl_preview', { pipeline: p(), ...ids }) as PreviewReport;
    expect(preview.sample.length).toBeGreaterThan(0);
    expect(preview.sample.every(row => row[2].t === 'Text' && row[2].v === '개발')).toBe(true);
    const report = await axMock('etl_run', { pipeline: p(), ...ids }) as RunReport;
    expect(report.status).toBe('completed');
    await expect(axMock('etl_run', { pipeline: p(), ...ids })).rejects.toThrow();
    expect((await axMock('etl_runs', {}) as unknown[]).length).toBe(1);
  });
});

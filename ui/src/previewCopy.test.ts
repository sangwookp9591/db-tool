import { describe, expect, it } from 'vitest';
import { decisionLabel, errorKindLabel, statementKindLabel } from './previewCopy';
describe('enum display labels', () => {
  it('labels statement kinds, error kinds, and decisions; unknown values pass through', () => {
    expect(statementKindLabel('Update')).toBe('수정');
    expect(statementKindLabel('Mystery')).toBe('Mystery');
    expect(errorKindLabel('Timeout')).toBe('시간 초과');
    expect(decisionLabel('denied')).toBe('거부');
    expect(decisionLabel('other')).toBe('other');
  });
});

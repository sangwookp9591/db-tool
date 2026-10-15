import { describe, it, expect } from 'vitest';
import { selectRows, suggestedQuery } from './types';
import type { Table } from '../contract';
describe('assistant explicit actions', () => {
  const table: Table = { columns: [{ name: 'id', type_name: 'int' }], rows: Array.from({length:30}, (_,i) => [{t:'Int',v:String(i+1)}]) };
  it('sends only explicitly selected distinct rows and rejects over-limit indices', () => {
    expect(selectRows(table, '3,1,3')?.rows).toEqual([table.rows[2], table.rows[0]]);
    expect(selectRows(table, '')?.rows).toEqual([]);
    for (const input of ['0', '-1', '31', '1.5', 'foo', Array.from({length:21},(_,i)=>i+1).join(',')]) expect(() => selectRows(table,input)).toThrow();
  });
  it('extracts fenced suggestions without treating arbitrary prose as query text', () => {
    expect(suggestedQuery('Explanation only.')).toBeNull();
    for (const language of ['sql', 'cypher', 'json']) expect(suggestedQuery(`Suggestion:\n\`\`\`${language}\nSELECT 1\n\`\`\``)).toBe('SELECT 1');
  });
});

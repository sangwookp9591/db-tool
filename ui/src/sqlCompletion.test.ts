import { expect, it } from 'vitest';
import { CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { sql, schemaCompletionSource } from '@codemirror/lang-sql';
import { sqlCompletionConfig } from './sqlCompletion';

async function complete(text: string, driverId = 'postgres', explicit = true) {
  const config = sqlCompletionConfig(driverId, [
    { namespace: ['app'], name: 'users' },
    { namespace: ['audit'], name: 'users' },
    { namespace: ['app'], name: 'order.items' },
    { namespace: [], name: 'odd"table' },
  ]);
  const state = EditorState.create({ doc: text, extensions: [sql(config)] });
  return schemaCompletionSource(config)(new CompletionContext(state, text.length, explicit));
}

it('offers tables across schemas and inserts qualified names to disambiguate duplicates', async () => {
  const result = await complete('SELECT * FROM us');
  expect(result?.options).toEqual(expect.arrayContaining([
    expect.objectContaining({ label: 'app.users', displayLabel: 'users', apply: '"app"."users"' }),
    expect.objectContaining({ label: 'audit.users', displayLabel: 'users', apply: '"audit"."users"' }),
  ]));
});

it('completes names after a schema prefix and preserves dots inside quoted identifiers', async () => {
  const result = await complete('SELECT * FROM app.');
  expect(result?.options.map(option => option.label)).toEqual(['users', 'order.items']);
  expect(result?.options.find(option => option.label === 'order.items')?.apply).toBe('"order.items"');
});

it('uses each driver dialect and escapes identifier quotes', async () => {
  expect((await complete('SELECT * FROM ', 'mysql'))?.options).toContainEqual(expect.objectContaining({ label: 'app.users', apply: '`app`.`users`' }));
  expect((await complete('SELECT * FROM ', 'postgres'))?.options).toContainEqual(expect.objectContaining({ label: 'odd"table', apply: '"odd""table"' }));
});

it('does not automatically offer table names in SQL comments or strings', async () => {
  expect(await complete("SELECT 'us", 'postgres', false)).toBeNull();
  expect(await complete('-- users', 'postgres', false)).toBeNull();
});

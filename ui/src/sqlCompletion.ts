import { MariaSQL, MySQL, PostgreSQL, SQLite, StandardSQL, sql, type SQLConfig, type SQLNamespace } from '@codemirror/lang-sql';
import { autocompletion, acceptCompletion } from '@codemirror/autocomplete';
import { EditorView, keymap } from '@codemirror/view';
import type { SqlTableName } from './explorer';

export function sqlCompletionConfig(driverId: string, tables: SqlTableName[] = []): SQLConfig {
  const dialect = ({ postgres: PostgreSQL, mysql: MySQL, mariadb: MariaSQL, sqlite: SQLite })[driverId] ?? StandardSQL;
  const quote = dialect.spec.identifierQuotes?.[0] ?? '"';
  const identifier = (name: string) => quote + name.replaceAll(quote, quote + quote) + quote;
  const schema: Record<string, SQLNamespace> = Object.create(null);
  const counts = new Map<string, number>();
  for (const table of tables) {
    // SQLNamespace uses dots as separators; escaped dots belong to an identifier.
    const path = [...table.namespace, table.name];
    schema[path.map(name => name.replaceAll('.', '\\.')).join('.')] = [];
    counts.set(table.name, (counts.get(table.name) ?? 0) + 1);
  }
  return {
    dialect, schema, upperCaseKeywords: true,
    // Offer tables from every schema, including when public is empty. Qualifying
    // the inserted name avoids depending on a server's search_path/default DB.
    tables: tables.map(table => ({
      label: counts.get(table.name)! > 1 ? [...table.namespace, table.name].join('.') : table.name,
      displayLabel: table.name,
      detail: table.namespace.join('.'),
      type: 'type',
      boost: 1,
      apply: [...table.namespace, table.name].map(identifier).join('.'),
    })),
  };
}

export function sqlEditorExtensions(driverId: string, tables?: SqlTableName[]) {
  return [sql(sqlCompletionConfig(driverId, tables)), autocompletion(), keymap.of([{ key: 'Tab', run: acceptCompletion }]), EditorView.theme({
    '.cm-tooltip': { backgroundColor: 'var(--panel)', color: 'var(--text)', border: '1px solid var(--border)' },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: 'var(--selection)', color: 'var(--text)' },
    '.cm-completionDetail': { color: 'var(--muted)' },
  })];
}

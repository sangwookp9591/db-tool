import { useMemo, useState } from 'preact/hooks';
import type { EditTarget, Table, Value } from './contract';
import type { PendingEdits } from './edits';
import { emptyEdits } from './edits';
import { inputValue, valueClass, valueText } from './values';
import { Button } from './ui/Button';
import { isMock } from './api';
import { columnWidths } from './virtual';
import { Delete } from 'lucide-preact';

export function EditableTable({ table, target, edits, onChange }: { table: Table; target: EditTarget; edits: PendingEdits; onChange: (next: PendingEdits) => void }) {
  const [active, setActive] = useState<{ row: number | string; column: string; value: string } | null>(null);
  const columns = table.columns;
  const widths = useMemo(() => columnWidths(table), [table.columns, table.rows]);
  const keyNames = new Set(target.key_columns);
  function valueAt(row: Value[], index: number, rowKey: number | string) {
    const column = columns[index];
    if (typeof rowKey === 'number') return edits.updates[rowKey]?.[column.name] ?? row[index];
    return edits.added.find(item => item.id === rowKey)?.values[index] ?? row[index] ?? { t: 'Null' };
  }
  function editValue(rowKey: number | string, index: number, raw: string) {
    const current = valueAt(typeof rowKey === 'number' ? table.rows[rowKey] : edits.added.find(item => item.id === rowKey)!.values, index, rowKey);
    const value = inputValue(raw, columns[index], current);
    if (typeof rowKey === 'number') onChange({ ...edits, updates: { ...edits.updates, [rowKey]: { ...edits.updates[rowKey], [columns[index].name]: value } } });
    else onChange({ ...edits, added: edits.added.map(item => item.id === rowKey ? { ...item, values: item.values.map((v, i) => i === index ? value : v) } : item) });
  }
  function setNull(rowKey: number | string, index: number) {
    const value: Value = { t: 'Null' };
    if (typeof rowKey === 'number') onChange({ ...edits, updates: { ...edits.updates, [rowKey]: { ...edits.updates[rowKey], [columns[index].name]: value } } });
    else onChange({ ...edits, added: edits.added.map(item => item.id === rowKey ? { ...item, values: item.values.map((v, i) => i === index ? value : v) } : item) });
  }
  function rowView(row: Value[], rowKey: number | string, isAdded = false) {
    const deleted = typeof rowKey === 'number' && edits.deleted.includes(rowKey);
    const modified = typeof rowKey === 'number' && !!edits.updates[rowKey];
    return <tr key={rowKey} class={isAdded ? 'edit-added' : deleted ? 'edit-deleted' : modified ? 'edit-modified' : ''}>
      {columns.map((column, index) => { const value = valueAt(row, index, rowKey); const keyLocked = !isAdded && keyNames.has(column.name); const editing = active?.row === rowKey && active.column === column.name;
        return <td key={column.name} class={`${valueClass(value)} ${edits.updates[typeof rowKey === 'number' ? rowKey : -1]?.[column.name] ? 'edit-changed-cell' : ''}`} onDblClick={() => !deleted && !keyLocked && setActive({ row: rowKey, column: column.name, value: value.t === 'Null' ? '' : value.t === 'Bytes' ? `0x${value.v.hex}` : valueText(value) })} onKeyDown={event => { if ((event.key === 'Enter' || event.key === 'F2') && !deleted && !keyLocked) { event.preventDefault(); setActive({ row: rowKey, column: column.name, value: value.t === 'Null' ? '' : value.t === 'Bytes' ? `0x${value.v.hex}` : valueText(value) }); } }} tabIndex={keyLocked || deleted ? -1 : 0} aria-label={`${column.name}${keyLocked ? ', primary key locked' : ''}`}>
          {editing ? <form class="cell-edit-form" onSubmit={event => { event.preventDefault(); try { editValue(rowKey, index, active.value); setActive(null); } catch (error) { (event.currentTarget.querySelector('[role=alert]') as HTMLElement).textContent = (error as Error).message; } }} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); setActive(null); } }}><input autoFocus aria-label={`Edit ${column.name}`} value={active.value} onInput={event => setActive({ ...active, value: event.currentTarget.value })} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); event.currentTarget.form?.requestSubmit(); } else if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setActive(null); } }} /><Button type="submit" bare aria-label="Save cell">✓</Button><Button type="button" bare aria-label="Set NULL" onClick={() => { setNull(rowKey, index); setActive(null); }}>NULL</Button><span role="alert" /></form> : value.t === 'Null' ? <em>NULL</em> : value.t === 'Text' && value.v === '' ? '""' : valueText(value)}
        </td>;
      })}
      <td class="edit-row-action">{isAdded ? <Button bare aria-label="Remove added row" onClick={() => onChange({ ...edits, added: edits.added.filter(item => item.id !== rowKey) })}>×</Button> : <Button bare aria-label={deleted ? 'Restore row' : 'Mark row for deletion'} onClick={() => onChange({ ...edits, deleted: deleted ? edits.deleted.filter(i => i !== rowKey) : [...edits.deleted, rowKey as number], updates: deleted ? edits.updates : Object.fromEntries(Object.entries(edits.updates).filter(([i]) => Number(i) !== rowKey)) })}>{deleted ? '↶' : '×'}</Button>}</td>
    </tr>;
  }
  return <div class="table-area edit-table-area">{isMock && <div class="edit-demo-note">모의 모드: 편집은 예시이며 이 브라우저 모의 환경에만 적용됩니다.</div>}<div class="edit-table-scroll"><table class="edit-table" style={{tableLayout:'fixed',width:`${widths.reduce((a,b)=>a+b,0)+40}px`}}><colgroup>{widths.map((width,index)=><col key={index} style={{width:`${width}px`}} />)}<col class="edit-action-col" /></colgroup><thead><tr>{columns.map(column => <th class={/int|float|double|numeric|decimal|real|serial/i.test(column.type_name)?'value-number':''} key={column.name} title={column.name}>{column.name}<small>{column.type_name}</small></th>)}<th class="edit-row-action" aria-label="Row actions"><Delete aria-hidden="true" /></th></tr></thead><tbody>{table.rows.map((row, index) => rowView(row, index))}{edits.added.map(item => rowView(item.values, item.id, true))}</tbody></table></div></div>;
}

import type { Table, Value } from '../contract';
import { valueText } from '../values';
function decimalOrder(a: string, b: string): number {
  function parts(text: string) {
    const match = /^([+-]?)(\d+)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(text);
    if (!match) return null;
    const digits = (match[2] + (match[3] ?? '')).replace(/^0+/, '');
    return { digits, sign: !digits ? 0 : match[1] === '-' ? -1 : 1, power: digits.length - (match[3]?.length ?? 0) + Number(match[4] ?? 0) };
  }
  const x = parts(a), y = parts(b);
  if (!x || !y) return a.localeCompare(b);
  if (x.sign !== y.sign) return x.sign - y.sign;
  if (!x.sign) return 0;
  if (x.power !== y.power) return x.sign * (x.power < y.power ? -1 : 1);
  const length = Math.max(x.digits.length, y.digits.length);
  const left = x.digits.padEnd(length, '0'), right = y.digits.padEnd(length, '0');
  return x.sign * (left < right ? -1 : left > right ? 1 : 0);
}
function compare(a: Value, b: Value) {
  if (a.t === 'Null' || b.t === 'Null') return a.t === b.t ? 0 : a.t === 'Null' ? -1 : 1;
  if (a.t === 'Int' && b.t === 'Int') { const x = BigInt(a.v), y = BigInt(b.v); return x < y ? -1 : x > y ? 1 : 0; }
  if ((a.t === 'Decimal' || a.t === 'Int') && (b.t === 'Decimal' || b.t === 'Int')) return decimalOrder(a.v, b.v);
  if (a.t === 'Float' && b.t === 'Float' && typeof a.v === 'number' && typeof b.v === 'number') return a.v - b.v;
  return valueText(a).localeCompare(valueText(b));
}
export function tableIndex(table: Table, filter: string, column = -1, descending = false): Uint32Array {
  const query = filter.trim().toLocaleLowerCase();
  const indices = Array.from({ length: table.rows.length }, (_, i) => i).filter(i => !query || table.rows[i].some(v => valueText(v).toLocaleLowerCase().includes(query)));
  if (column >= 0 && column < table.columns.length) indices.sort((a, b) => (descending ? -1 : 1) * compare(table.rows[a][column], table.rows[b][column]) || a - b);
  return Uint32Array.from(indices);
}

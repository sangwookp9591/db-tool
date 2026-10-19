import { expect, it } from 'vitest';
import { decodeMessagePack, messagePackStats } from './msgpack';
const bytes = (hex: string) => Uint8Array.from(hex.match(/../g)!, n => parseInt(n, 16)).buffer;
it('decodes Rust-compatible named maps, UTF-8 and exact integer strings', () => {
  expect(decodeMessagePack(bytes('84a46b696e64a4726f7773a5696e64657800a66f666673657400a57461626c6583a7636f6c756d6e739182a46e616d65a6eca095ed9995a9747970655f6e616d65a4696e7438a5636f756e7402a4646174619182a3746167a3496e74a676616c75657392b039303037313939323534373430393933c0'))).toEqual({ kind: 'rows', index: 0, offset: 0, table: { columns: [{ name: '정확', type_name: 'int8' }], count: 2, data: [{ tag: 'Int', values: ['9007199254740993', null] }] } });
  expect(decodeMessagePack(bytes('96c2c3ffcd0100d1ff00cb3ff8000000000000'))).toEqual([false, true, -1, 256, -256, 1.5]);
});
it('rejects truncated, oversized, unsafe and ambiguous frames without prototype mutation', () => {
  for (const hex of ['a361', 'ddffffffff', 'df7fffffff', 'cf0020000000000000', '0000', 'c1', '82a16101a16102', 'a1ff']) expect(() => decodeMessagePack(bytes(hex))).toThrow();
  expect(() => decodeMessagePack(new ArrayBuffer(16 * 1024 * 1024 + 1))).toThrow();
  expect(() => decodeMessagePack(Uint8Array.from([...Array(130).fill(0x91), 0xc0]).buffer)).toThrow();
  const value = decodeMessagePack(bytes('81a95f5f70726f746f5f5f01')) as Record<string, unknown>;
  expect(Object.getPrototypeOf(value)).toBeNull(); expect(value.__proto__).toBe(1);
});

const str = (value: string) => { const data = new TextEncoder().encode(value); if (data.length > 31) throw new Error('fixture uses fixstr'); return [0xa0 + data.length, ...data]; };
const column = (tag: string, items: number[][]) => { const n = items.length; const header = n <= 65535 ? [0xdc, n >>> 8, n & 255] : [0xdd, n >>> 24, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]; return Uint8Array.from([0x83, ...str('tag'), ...str(tag), ...str('values'), ...header, ...items.flat(), ...str('after'), 0xc3]).buffer; };
it('retains scalar column bytes while preserving array access, Unicode, nulls and subsequent fields', () => {
  const values = Array.from({ length: 64 }, (_, i) => i === 0 ? '정확😀' : i === 1 ? null : `row ${i}`);
  const stringsBefore = messagePackStats.decodedStrings;
  const decoded = decodeMessagePack(column('Text', values.map(value => value === null ? [0xc0] : str(value)))) as { tag: string; values: unknown[]; after: boolean };
  expect(messagePackStats.decodedStrings - stringsBefore).toBeLessThan(10);
  expect(Array.isArray(decoded.values)).toBe(true); expect(decoded.values).toHaveLength(64);
  expect(decoded.values[63]).toBe('row 63'); expect(decoded.values[0]).toBe('정확😀'); expect(decoded.values[64]).toBeUndefined();
  expect(decoded.values.slice(0, 2)).toEqual(['정확😀', null]); expect(Object.keys(decoded.values)).toHaveLength(64);
  expect(JSON.parse(JSON.stringify(decoded))).toEqual({ tag: 'Text', values, after: true });
  expect(() => decoded.values.push('write')).toThrow();
  const scalar = new Uint8Array(9); scalar[0] = 0xcb; new DataView(scalar.buffer).setFloat64(1, 1.25);
  const floats = decodeMessagePack(column('Float', Array.from({ length: 64 }, () => [...scalar]))) as { values: number[] };
  expect(floats.values[63]).toBe(1.25);
});
it('validates every retained scalar before exposing cells, including unaccessed malformed UTF-8', () => {
  for (const invalid of [[0xa1, 0xff], [0xa2, 0xc0, 0xaf], [0xa3, 0xed, 0xa0, 0x80], [0xa4, 0xf4, 0x90, 0x80, 0x80], [0xcf, 0x00, 0x20, 0, 0, 0, 0, 0, 0], [0x91, 0xc0]]) {
    const items = Array.from({ length: 64 }, (_, i) => i === 63 ? invalid : [0xc0]);
    expect(() => decodeMessagePack(column('Text', items))).toThrow();
  }
  expect(() => decodeMessagePack(column('Text', Array.from({ length: 100001 }, () => [0xc0])))).toThrow('row limit');
});

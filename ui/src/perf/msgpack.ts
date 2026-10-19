import { arrayView } from './arrayView';
const scalarTags = new Set(['Null', 'Bool', 'Int', 'Float', 'Decimal', 'Text', 'Date', 'Time', 'DateTime', 'DateTimeTz', 'Duration', 'Uuid', 'Json']);
export const messagePackStats = { frames: 0, bytes: 0, retainedScalarCells: 0, decodedStrings: 0, uniformColumns: 0, mixedColumns: 0 };
// JSON-compatible MessagePack emitted by rmp-serde; DB integers/decimals remain tagged strings.
export function decodeMessagePack(buffer: ArrayBuffer): unknown {
  if (buffer.byteLength > 16 * 1024 * 1024) throw new Error('Result frame too large');
  messagePackStats.frames++; messagePackStats.bytes += buffer.byteLength;
  const bytes = new Uint8Array(buffer), view = new DataView(buffer), utf8 = new TextDecoder('utf-8', { fatal: true });
  let offset = 0;
  function take(size: number) { const start = offset; if (size > bytes.length - offset) throw new Error('Truncated result frame'); offset += size; return start; }
  function uint(size: number) { const start = take(size); return size === 1 ? view.getUint8(start) : size === 2 ? view.getUint16(start) : view.getUint32(start); }
  function string(size: number) { messagePackStats.decodedStrings++; const start = take(size); return utf8.decode(bytes.subarray(start, start + size)); }
  function array(size: number, depth: number, lazy: boolean): unknown[] {
    if (size > bytes.length - offset) throw new Error('Invalid result array length');
    if (!lazy || size < 64) return Array.from({ length: size }, () => read(depth + 1));
    if (size > 100000) throw new Error('Result column row limit exceeded');
    const positions = new Uint32Array(size);
    for (let row = 0; row < size; row++) { positions[row] = offset; skipScalar(); }
    messagePackStats.retainedScalarCells += size;
    // Retain encoded column bytes, not a million decoded strings and temporary typed-array views.
    return arrayView(size, row => { const saved = offset; offset = positions[row]; try { return read(depth + 1); } finally { offset = saved; } });
  }
  function map(size: number, depth: number) {
    if (size > (bytes.length - offset) / 2) throw new Error('Invalid result map length');
    const result: Record<string, unknown> = Object.create(null);
    for (let i = 0; i < size; i++) { const key = read(depth + 1); if (typeof key !== 'string' || Object.hasOwn(result, key)) throw new Error('Invalid result map key'); const lazy = key === 'values' && typeof result.tag === 'string' && scalarTags.has(result.tag); if (key === 'values' && Object.hasOwn(result, 'tag')) { if (lazy) messagePackStats.uniformColumns++; else messagePackStats.mixedColumns++; } result[key] = read(depth + 1, lazy); }
    return result;
  }
  function skipString(size: number) {
    let start = take(size); const end = start + size;
    while (start < end) {
      const first = bytes[start++]; if (first < 128) continue;
      let remaining: number, code: number, minimum: number;
      if (first >= 0xc2 && first <= 0xdf) { remaining = 1; code = first & 31; minimum = 0x80; }
      else if (first >= 0xe0 && first <= 0xef) { remaining = 2; code = first & 15; minimum = 0x800; }
      else if (first >= 0xf0 && first <= 0xf4) { remaining = 3; code = first & 7; minimum = 0x10000; }
      else throw new Error('Invalid result UTF-8');
      for (let i = 0; i < remaining; i++) {
        if (start >= end || (bytes[start] & 0xc0) !== 0x80) throw new Error('Invalid result UTF-8');
        code = (code << 6) | (bytes[start++] & 63);
      }
      if (code < minimum || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) throw new Error('Invalid result UTF-8');
    }
  }
  function skipScalar() {
    const tag = uint(1);
    if (tag <= 0x7f || tag >= 0xe0 || tag === 0xc0 || tag === 0xc2 || tag === 0xc3) return;
    if (tag >= 0xa0 && tag <= 0xbf) { skipString(tag & 31); return; }
    switch (tag) {
      case 0xcc: case 0xd0: take(1); return;
      case 0xcd: case 0xd1: take(2); return;
      case 0xce: case 0xd2: case 0xca: take(4); return;
      case 0xcb: take(8); return;
      case 0xcf: case 0xd3: {
        const start = take(8), value = Number(tag === 0xcf ? view.getBigUint64(start) : view.getBigInt64(start));
        if (!Number.isSafeInteger(value)) throw new Error('Unsafe result integer');
        return;
      }
      case 0xd9: skipString(uint(1)); return;
      case 0xda: skipString(uint(2)); return;
      case 0xdb: skipString(uint(4)); return;
      default: throw new Error('Invalid packed result scalar');
    }
  }
  function read(depth: number, lazy = false): unknown {
    if (depth > 128) throw new Error('Result nesting limit exceeded');
    const tag = uint(1);
    if (tag <= 0x7f) return tag;
    if (tag >= 0xe0) return tag - 256;
    if (tag >= 0xa0 && tag <= 0xbf) return string(tag & 31);
    if (tag >= 0x90 && tag <= 0x9f) return array(tag & 15, depth, lazy);
    if (tag >= 0x80 && tag <= 0x8f) return map(tag & 15, depth);
    switch (tag) {
      case 0xc0: return null;
      case 0xc2: return false;
      case 0xc3: return true;
      case 0xca: return view.getFloat32(take(4));
      case 0xcb: return view.getFloat64(take(8));
      case 0xcc: return uint(1);
      case 0xcd: return uint(2);
      case 0xce: return uint(4);
      case 0xcf: case 0xd3: {
        const start = take(8), value = Number(tag === 0xcf ? view.getBigUint64(start) : view.getBigInt64(start));
        if (!Number.isSafeInteger(value)) throw new Error('Unsafe result integer');
        return value;
      }
      case 0xd0: return view.getInt8(take(1));
      case 0xd1: return view.getInt16(take(2));
      case 0xd2: return view.getInt32(take(4));
      case 0xd9: return string(uint(1));
      case 0xda: return string(uint(2));
      case 0xdb: return string(uint(4));
      case 0xdc: return array(uint(2), depth, lazy);
      case 0xdd: return array(uint(4), depth, lazy);
      case 0xde: return map(uint(2), depth);
      case 0xdf: return map(uint(4), depth);
      default: throw new Error('Unsupported result MessagePack type');
    }
  }
  const result = read(0);
  if (offset !== bytes.length) throw new Error('Trailing result bytes');
  return result;
}

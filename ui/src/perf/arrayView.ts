export function arrayView<T>(length: number, read: (index: number) => T): T[] {
  const target: T[] = []; target.length = length;
  const index = (key: string | symbol) => typeof key === 'string' && /^(0|[1-9]\d*)$/.test(key) ? Number(key) : null;
  return new Proxy(target, {
    get(rows, key, receiver) { const row = index(key); return row === null ? Reflect.get(rows, key, receiver) : row < rows.length ? read(row) : undefined; },
    has(rows, key) { const row = index(key); return row === null ? Reflect.has(rows, key) : row < rows.length; },
    ownKeys(rows) { return [...Array.from({ length: rows.length }, (_, row) => String(row)), 'length']; },
    getOwnPropertyDescriptor(rows, key) {
      const row = index(key);
      return row !== null && row < rows.length ? { configurable: true, enumerable: true, writable: false, value: read(row) } : Reflect.getOwnPropertyDescriptor(rows, key);
    },
    set() { throw new Error('Streamed result values are read-only'); },
    deleteProperty() { throw new Error('Streamed result values are read-only'); }
  });
}

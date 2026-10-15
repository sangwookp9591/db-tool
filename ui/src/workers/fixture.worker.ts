import { fixtureTable, fixtureResult } from '../perf/fixtures';
import { packTable } from '../perf/columnar';
onmessage = event => {
  const { count, width } = event.data;
  const columns = fixtureTable(0, width).columns;
  postMessage(JSON.stringify({ kind: 'start', index: 0, result: fixtureResult({ columns, rows: [] }) }));
  for (let offset = 0; offset < count;) {
    const size = Math.min(count - offset, offset === 0 ? 200 : 2000);
    const table = packTable(fixtureTable(size, width, offset));
    postMessage(JSON.stringify({ kind: 'rows', index: 0, offset, table })); offset += size;
  }
  postMessage(JSON.stringify({ kind: 'done' }));
};

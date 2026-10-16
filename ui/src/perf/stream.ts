import { paintQueue } from './paintQueue';
import { Channel, invoke } from '@tauri-apps/api/core';
import { api, isMock, type Commands } from '../api';
import type { ResultSet } from '../contract';
import { packTable } from './columnar';
import { finish, painted, start } from './index';
import { ResultStream, type StreamEvent } from './streamState';
export async function streamResults(results: ResultSet[], receive: (event: StreamEvent) => void) {
  for (const [index, result] of results.entries()) {
    const rows = result.table?.rows ?? [];
    receive({ kind: 'start', index, result: { ...result, row_count: 0, table: result.table ? { columns: result.table.columns, rows: [] } : null } });
    for (let offset = 0; offset < rows.length;) {
      const size = offset === 0 ? 200 : 2000;
      const table = packTable({ columns: result.table!.columns, rows: rows.slice(offset, offset + size) });
      receive(JSON.parse(JSON.stringify({ kind: 'rows', index, offset, table })));
      offset += table.count;
      await new Promise(resolve => setTimeout(resolve, 0));
    }
    const { table: _table, graph: _graph, documents: _documents, ...meta } = result;
    receive({ kind: 'end', index, result: meta });
  }
  receive({ kind: 'done' });
}
export async function executeQuery(args: Commands['execute']['args'], update: (results: ResultSet[]) => void) {
  const since = start('execute-click'); const stream = new ResultStream();
  let frameId = 0, first = true, failed = false;
  let resolveDone!: () => void, rejectDone!: (error: unknown) => void;
  const done = new Promise<void>((resolve, reject) => { resolveDone = resolve; rejectDone = reject; });
  // Install a handler immediately so a malformed channel event never becomes an unhandled rejection.
  void done.catch(() => {});
  const flush = () => { frameId = 0; if (failed) return; update([...stream.results]); if (first && stream.results.some(r => !r.graph && r.table?.rows.length)) { first = false; void painted('execute-first-row', since); } };
  const receiver = paintQueue<StreamEvent>(event => {
    if (failed) return false;
    stream.accept(event);
    const firstRows = first && stream.results.some(result => !result.graph && !!result.table?.rows.length);
    if (firstRows) { cancelAnimationFrame(frameId); flush(); }
    else if (!frameId) frameId = requestAnimationFrame(flush);
    if (event.kind === 'done') { finish('last-chunk', since); resolveDone(); }
    return firstRows;
  }, error => { failed = true; rejectDone(error); });
  const receive = receiver.push;
  try {
    if (isMock) await streamResults(await api('execute', args), receive);
    else { const onEvent = new Channel<StreamEvent>(); onEvent.onmessage = receive; const direct = await invoke<ResultSet[]>('execute_stream', { ...args, onEvent }); if (direct.length) { direct.forEach((result, index) => receive({ kind: 'start', index, result })); receive({ kind: 'done' }); } }
    await done; receiver.dispose(); cancelAnimationFrame(frameId); flush(); return stream.results;
  } catch (error) { failed = true; receiver.dispose(); cancelAnimationFrame(frameId); throw error; }
}

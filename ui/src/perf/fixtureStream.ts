import { paintQueue } from './paintQueue';
import type { ResultSet } from '../contract';
import { ResultStream } from './streamState';
export function fixtureStream(count: number, width: number, update: (results: ResultSet[]) => void) {
  return new Promise<void>((resolve, reject) => {
    const worker = new Worker(new URL('../workers/fixture.worker.ts', import.meta.url), { type: 'module' });
    const state = new ResultStream();
    const receiver = paintQueue<string>(data => { state.accept(JSON.parse(data)); update([...state.results]); if (state.done) { receiver.dispose(); worker.terminate(); resolve(); } return state.results.some(result => !!result.table?.rows.length); }, error => { worker.terminate(); reject(error); });
    worker.onerror = event => { receiver.dispose(); worker.terminate(); reject(new Error(event.message)); };
    worker.onmessage = event => receiver.push(event.data);
    worker.postMessage({ count, width });
  });
}

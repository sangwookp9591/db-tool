import type { ResultSet } from '../contract';
import { ResultStream } from './streamState';
export function fixtureStream(count: number, width: number, update: (results: ResultSet[]) => void) {
  return new Promise<void>((resolve, reject) => {
    const worker = new Worker(new URL('../workers/fixture.worker.ts', import.meta.url), { type: 'module' });
    const state = new ResultStream();
    worker.onerror = event => { worker.terminate(); reject(new Error(event.message)); };
    worker.onmessage = event => {
      try { state.accept(JSON.parse(event.data)); update([...state.results]); if (state.done) { worker.terminate(); resolve(); } }
      catch (error) { worker.terminate(); reject(error); }
    };
    worker.postMessage({ count, width });
  });
}

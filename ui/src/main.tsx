import { render } from 'preact';
import { App } from './App';
import './style.css';
render(<App />, document.getElementById('app')!);

if ('__TAURI_INTERNALS__' in window) void import('@tauri-apps/api/core').then(async ({ invoke }) => { if (await invoke<boolean>('viewer_bench_enabled')) await import('./perf/app'); });

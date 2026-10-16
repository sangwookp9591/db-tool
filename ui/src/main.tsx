import { fontsReady } from './theme';
import { render } from 'preact';
import { App } from './App';
import './style.css';
if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window) && new URLSearchParams(location.search).has('gallery')) void import('./ui/gallery').then(({ mountGallery }) => mountGallery()); else void fontsReady.then(() => render(<App />, document.getElementById('app')!));

if ('__TAURI_INTERNALS__' in window) void import('@tauri-apps/api/core').then(async ({ invoke }) => { if (await invoke<boolean>('viewer_bench_enabled')) await import('./perf/app'); });

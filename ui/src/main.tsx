import { render } from 'preact';
import { App } from './App';
import './style.css';
render(<App />, document.getElementById('app')!);

if ('__TAURI_INTERNALS__' in window) void import('./perf/app');

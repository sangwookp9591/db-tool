import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import type { DriverManifest } from '../contract';
import type { BenchConfig, BenchConnection } from './benchConfig';
import { frame, painted, perf, stats, type Metric } from './index';
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(test: () => boolean) { const end = performance.now() + 60000; while (!test()) { if (performance.now() > end) throw new Error('Connection benchmark UI timeout'); await pause(10); } }
function input(element: HTMLInputElement | HTMLSelectElement, value: string) { if (!element) throw new Error('Missing connection field'); element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })); element.dispatchEvent(new Event('change', { bubbles: true })); }
interface Phase { driver: string; paradigm: string; trial: number; cold: boolean; phase: string; startTime: number; duration: number; ipc: Metric[] }
export async function connectionBench() {
  const app = document.getElementById('app')!;
  const output: { complete?: boolean; error?: string; checkpoint: number; stage: string; iteration: number; graphWorkers: number; viewport: object; renderer_time_origin: number; phases: Phase[]; registry: number[]; summary?: unknown } = { checkpoint: 0, stage: 'startup', iteration: 0, graphWorkers: 0, viewport: {}, renderer_time_origin: performance.timeOrigin, phases: [], registry: [] };
  async function checkpoint(stage: string, iteration = 0) { Object.assign(output, { checkpoint: output.checkpoint + 1, stage, iteration, viewport: { width: innerWidth, height: innerHeight, scale: devicePixelRatio } }); await invoke('viewer_bench_write', { report: output }); await invoke('viewer_bench_checkpoint', { checkpoint: output.checkpoint }); }
  perf.captureCommands = true;
  try {
    await checkpoint('startup');
    await waitFor(() => !!app.querySelector('.empty-workspace h2'));
    const manifests = await api('drivers_list', {}), config = await invoke<BenchConfig>('viewer_bench_config');
    await checkpoint('prepare-registry');
    for (let i = 0; i < 20; i++) { const since = performance.now(); await api('drivers_list', {}); output.registry.push(performance.now() - since); }
    const table = manifests.find(d => d.paradigm === 'Relational' && d.connection_fields.some(f => f.key === 'host')), graph = manifests.find(d => d.paradigm === 'Graph');
    if (!table || !graph) throw new Error('Required test drivers missing');
    let trial = 0;
    for (const driver of [table, graph]) for (let repeat = 0; repeat < 20; repeat++) {
      trial++; await checkpoint('prepare-connection-flow', trial);
      await flow(driver, driver.paradigm === 'Graph' ? config.graph : config.table, trial, repeat === 0);
      await checkpoint('connection-flow', trial);
    }
    const grouped = new Map<string, number[]>();
    for (const p of output.phases) { const key = `${p.driver}:${p.phase}`; if (!grouped.has(key)) grouped.set(key, []); grouped.get(key)!.push(p.duration); }
    output.summary = { phases: Object.fromEntries([...grouped].map(([key, values]) => [key, stats(values)])), warm_registry_ipc: stats(output.registry) };
    output.complete = true;
    async function flow(driver: DriverManifest, settings: BenchConnection, trial: number, cold: boolean) {
      const name = `P1 flow ${driver.paradigm} ${trial}`;
      const connection = () => [...app.querySelectorAll<HTMLElement>('.connection')].find(c => c.querySelector('strong')?.textContent === name);
      async function phase(label: string, action: () => Promise<void>) { perf.clear(); const since = performance.now(); await action(); const duration = await painted(`connection-${label}`, since); output.phases.push({ driver: driver.id, paradigm: driver.paradigm, trial, cold, phase: label, startTime: since, duration, ipc: perf.marks().filter(m => m.name.startsWith('ipc:') && m.startTime >= since) }); }
      let form: HTMLFormElement;
      const field = (label: string) => [...form.querySelectorAll('label')].find(l => l.firstChild?.textContent?.trim().replace(/\s*\*$/, '') === label)?.querySelector<HTMLInputElement | HTMLSelectElement>('input,select');
      try {
        await phase('open', async () => { app.querySelector<HTMLButtonElement>('[aria-label="Add connection"]')!.click(); await waitFor(() => !!app.querySelector('dialog[open] .profile-form form')); form = app.querySelector<HTMLFormElement>('.profile-form form')!; });
        const other = manifests.find(d => d.id !== driver.id); if (!other) throw new Error('Driver switching requires two manifests');
        input(field('Driver')!, other.id); await frame(); await frame();
        await phase('select', async () => { input(field('Driver')!, driver.id); await waitFor(() => driver.connection_fields.every(f => !!field(f.label))); });
        input(field('Name')!, name);
        for (const f of driver.connection_fields) if (f.secret || settings.fields[f.key] !== undefined) input(field(f.label)!, f.secret ? settings.secret : settings.fields[f.key]);
        await frame(); await frame();
        await phase('test', async () => { const button = form.querySelector<HTMLButtonElement>('[data-testid=test-connection-form]')!; button.click(); await waitFor(() => !!form.querySelector('[role=status]')?.textContent && !button.disabled); });
        await phase('save', async () => { form.querySelector<HTMLButtonElement>('[type=submit]')!.click(); await waitFor(() => !!connection() && !app.querySelector('.profile-form form')); });
        await phase('connect', async () => {
          connection()!.querySelector<HTMLButtonElement>('.connection-actions button')!.click();
          await waitFor(() => !!app.querySelector('dialog[open] input[name=credential]'));
          const credential = app.querySelector<HTMLInputElement>('input[name=credential]')!; input(credential, settings.secret); await frame();
          credential.closest('form')!.querySelector<HTMLButtonElement>('[type=submit]')!.click();
          await waitFor(() => !!connection()!.querySelector('.tree-node'));
        });
      } finally {
        const element = connection();
        if (element?.querySelector('.new-query')) { element.querySelector<HTMLButtonElement>('.connection-actions button')!.click(); await waitFor(() => !element.querySelector('.new-query')); }
        if (element) {
          const original = window.confirm; window.confirm = message => message === `Delete connection “${name}”?` || original.call(window, message);
          try { element.querySelector<HTMLButtonElement>('[aria-label^="Delete "]')!.click(); await waitFor(() => !connection()); }
          finally { window.confirm = original; }
        }
        const close = app.querySelector<HTMLButtonElement>('[aria-label="Close connection form"]'); if (close) close.click();
      }
    }
  } catch (error) { output.complete = false; output.error = error instanceof Error ? error.message : JSON.stringify(error); }
  finally { perf.captureCommands = false; await invoke('viewer_bench_write', { report: output }); }
}

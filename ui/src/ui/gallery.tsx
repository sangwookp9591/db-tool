import { Button } from './Button';
// React Bits interaction gallery (MIT + Commons Clause), original author David Haz.
import { render } from 'preact';
import { ExtraGallery } from './ExtraGallery';
import { useRef, useState } from 'preact/hooks';
import { JellyRadio } from './JellyRadio';
import { SwipeRow } from './SwipeRow';
import { CometDial } from './CometDial';
import { WakeSlider } from './WakeSlider';
import { CodeSlots, NumberSlots } from './CodeSlots';
import { LatticeLoader } from './LatticeLoader';
import { BorderGlow } from './BorderGlow';
import { SpringCheck } from './SpringCheck';
import { FuseButton } from './FuseButton';
import { CommandPalette } from './CommandPalette';
import { ToastStack, type ToastItem } from './ToastStack';
import { Tooltip } from './Tooltip';
import { SlidingIndicator } from './SlidingIndicator';
import { GlideSelect } from './GlideSelect';
import { SpotlightCard } from './SpotlightCard';
import { Skeleton } from './Skeleton';
import { IntegrationBeam, type IntegrationBeamHandle } from './IntegrationBeam';

function Gallery() {
  const [view, setView] = useState('Table');
  const [open, setOpen] = useState(false);
  const [timeout, setTimeoutValue] = useState(30);
  const [limit, setLimit] = useState(1000);
  const [code, setCode] = useState('120');
  const [metric, setMetric] = useState(128);
  const [running, setRunning] = useState(false);
  const [checked, setChecked] = useState(true);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState('No action committed');
  const [theme, setTheme] = useState('dark');
  const [palette, setPalette] = useState(false);
  const [tooltip, setTooltip] = useState(false);
  const [segment, setSegment] = useState('Connections');
  const [glide, setGlide] = useState('Two');
  const [loading, setLoading] = useState(false);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const beam = useRef<IntegrationBeamHandle>(null);
  return <main className="micro-gallery micro" data-theme={theme}>
    <header><h1>AxDB · Micro interactions</h1><JellyRadio items={['dark', 'light']} value={theme} onChange={next => { setTheme(next); document.documentElement.dataset.theme = next === 'light' ? 'daylight' : 'midnight'; }} ariaLabel="Gallery theme" /><p>Motion on input. Quiet at rest.</p></header>
    <div className="micro-gallery__grid">
      <ExtraGallery />
      <section><h2>01 · JellyRadio</h2><JellyRadio role="tablist" items={['Table', 'Graph', 'JSON', 'ERD'].map(label => ({ value: label, label, ariaLabel: label, testId: `gallery-view-${label}` }))} value={view} onChange={setView} aria-label="Gallery result view" testId="gallery-views" /><p>Arrow keys select a view.</p></section>
      <section><h2>02 · SwipeRow</h2><SwipeRow label="Gallery history" open={open} onOpenChange={setOpen} actions={[
        { id: 'open', label: 'Open', onSelect: () => setMessage('Opened in new tab') },
        { id: 'copy', label: 'Copy', onSelect: () => setMessage('Copied history') },
        { id: 'delete', label: 'Delete', onSelect: () => setPending(true) },
      ]}>Query history</SwipeRow><p>Drag left, scroll horizontally, or use the arrow button.</p></section>
      <section><h2>03 · CometDial</h2><div className="micro-gallery__dial"><CometDial value={timeout} onChange={setTimeoutValue} ariaLabel="Gallery timeout" /><p>1–600 seconds<br />Logarithmic dial<br />Drag · wheel · arrow keys</p></div></section>
      <section><h2>04 · WakeSlider</h2><WakeSlider value={limit} onChange={setLimit} ariaLabel="Gallery row limit" /><p>100–100,000 rows · wake follows drag speed.</p></section>
      <section><h2>05 · CodeSlots / NumberSlots</h2><CodeSlots value={code} onChange={setCode} ariaLabel="Gallery code" /><div className="micro-gallery__controls"><NumberSlots value={metric} ariaLabel="Gallery elapsed" suffix="ms" /><Button bare type="button" onClick={() => setMetric(metric + 137)}>Update metric</Button></div></section>
      <section><h2>06 · LatticeLoader</h2><SpringCheck checked={running} onChange={setRunning} label="Run loader" /><div className="micro-gallery__controls"><LatticeLoader running={running} label="Query running" />{!running && <p>Stopped · no loader DOM or timer</p>}</div></section>
      <section><h2>07 · BorderGlow</h2><BorderGlow ariaLabel="Gallery focus panel"><label>Editor panel<input aria-label="Gallery panel input" placeholder="Focus or move pointer" /></label></BorderGlow><p>One animation frame per pointer burst.</p></section>
      <section><h2>08 · SpringCheck</h2><SpringCheck checked={checked} onChange={setChecked} label="Read only" /><p>Native checkbox · Space to toggle.</p></section>
      <section><h2>09 · FuseButton</h2><FuseButton pending={pending} onPendingChange={setPending} onCommit={() => setMessage('Delete committed')} onUndo={() => setMessage('Delete undone')} label="Delete sample" /><p role="status">{message}</p></section>
      <section><h2>10 · Command palette</h2><Button bare type="button" onClick={() => setPalette(true)}>Search workspace ⌘K</Button><p>Connections, tables and recent queries. Arrow keys and Enter open the selected item.</p></section>
      <section><h2>11 · Toast stack</h2><Button bare type="button" onClick={() => setToasts(items => [...items, { id: String(Date.now()), message: 'Query completed · 128 rows', tone: 'success' }])}>Show notification</Button><p>Dismiss explicitly; messages remain available to keyboard and screen readers.</p></section>
      <section><h2>12 · Tooltip</h2><Tooltip open={tooltip} onOpenChange={setTooltip} content="Execute the current query · ⌘Enter"><Button bare type="button" aria-label="Gallery run query">Run query</Button></Tooltip><p>Hover or focus for help. Escape dismisses.</p></section>
      <section><h2>13 · Sliding indicator</h2><SlidingIndicator value={segment}><JellyRadio items={['Connections', 'History']} value={segment} onChange={setSegment} ariaLabel="Gallery workspace section" /></SlidingIndicator><p>The active section stays visible while selection moves.</p></section>
      <section><h2>14 · GlideSelect</h2><GlideSelect ariaLabel="Gallery glide select" value={glide} options={[{ value: 'One', label: 'One', tag: 'First' }, { value: 'Two', label: 'Two', tag: 'Selected' }, { value: 'Three', label: 'Three' }]} onChange={setGlide} /><p>Keyboard, typeahead, pointer scrubbing, and viewport flip.</p></section>
      <section><h2>15 · Spotlight card</h2><SpotlightCard ariaLabel="Gallery connection card"><strong>Analytics workspace</strong><p>Read-only connection</p><Button bare type="button" onClick={() => setMessage('Connection selected')}>Select connection</Button></SpotlightCard></section>
      <section><h2>15 · Loading skeleton</h2><SpringCheck checked={loading} onChange={setLoading} label="Show loading skeleton" /><Skeleton running={loading} /><p>One entrance, then still. Outside result grids and canvases.</p></section>
      <section className="micro-gallery__beam"><h2>16 · Integration Beam</h2><IntegrationBeam controllerRef={beam} nodes={[
        { id: 'analytics', label: 'Analytics', side: 'source', state: 'connected' },
        { id: 'archive', label: 'Archive', side: 'source', state: 'disconnected' },
        { id: 'local', label: 'Local agent', side: 'target', state: 'connected' },
        { id: 'approval', label: 'Approval', side: 'target', state: 'pending' },
      ]} onSelect={id => setMessage(`Details: ${id}`)} /><div className="micro-gallery__controls">
        <Button bare type="button" onClick={() => beam.current?.pulse('analytics', 'axdb', 'primary')}>Simulate query event</Button>
        <Button bare type="button" onClick={() => beam.current?.pulse('axdb', 'local', 'success')}>Simulate response event</Button>
        <Button bare type="button" onClick={() => beam.current?.pulse('approval', 'axdb', 'warning')}>Simulate approval event</Button>
      </div><p>Event-driven only. Select a tile to open details; no hidden permission toggles.</p></section>
    </div>
    <CommandPalette open={palette} onOpenChange={setPalette} items={[
      { id: 'connection', label: 'Analytics workspace', group: 'Connections', onSelect: () => setMessage('Connection selected') },
      { id: 'table', label: 'public.orders', group: 'Tables', onSelect: () => setMessage('Table selected') },
      { id: 'query', label: 'Recent order query', group: 'Recent queries', keywords: 'select orders', onSelect: () => setMessage('Recent query selected') },
    ]} />
    <ToastStack items={toasts} onDismiss={id => setToasts(items => items.filter(item => item.id !== id))} />
  </main>;
}

export function mountGallery() {
  document.documentElement.dataset.theme = 'midnight';
  render(<Gallery />, document.getElementById('app')!);
}

// 사용자 제공 레퍼런스 재구현
import type { ComponentChildren } from 'preact';
import { useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Check, ChevronDown, Copy, LoaderCircle, Wrench } from 'lucide-preact';
import type { MicroAccessibility } from './accessibility';
import { Button } from './Button';
import { decisionLabel } from '../previewCopy';
import './micro-extra.css';

export interface ToolCallProps extends MicroAccessibility {
  name: string;
  summary?: string;
  status: 'running' | 'success' | 'error';
  icon?: ComponentChildren;
  arguments?: unknown;
  result?: unknown;
  error?: string;
  durationMs?: number;
  startedAt?: number;
  decision?: 'allowed' | 'denied' | 'approved';
  retrySafe?: boolean;
  onRetry?: () => void;
}

/** Values must already be redacted; rendering and copy share the same bounded preview. */
export function toolPreview(value: unknown) {
  let remaining = 200;
  const seen = new WeakSet<object>();
  const bound = (item: unknown, depth: number): unknown => {
    if (--remaining < 0 || depth > 6) return '[preview limit]';
    if (typeof item === 'string') return item.length > 1000 ? `${item.slice(0, 1000)}…` : item;
    if (typeof item === 'bigint') return String(item);
    if (item === null || typeof item !== 'object') return item;
    if (seen.has(item)) return '[circular]';
    seen.add(item);
    if (Array.isArray(item)) return item.slice(0, 20).map(child => bound(child, depth + 1));
    return Object.fromEntries(Object.keys(item).slice(0, 20).map(key => [key.slice(0, 200), bound((item as Record<string, unknown>)[key], depth + 1)]));
  };
  try {
    const text = JSON.stringify(bound(value, 0), null, 2) ?? 'null';
    const lines = text.slice(0, 8000).split('\n');
    return lines.slice(0, 20).join('\n').slice(0, 8000);
  } catch { return '[Preview unavailable]'; }
}

function JsonBlock({ label, value }: { label: string; value: unknown }) {
  const text = toolPreview(value);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const timeout = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(false);
  useLayoutEffect(() => { mounted.current = true; return () => { mounted.current = false; clearTimeout(timeout.current); }; }, []);
  useLayoutEffect(() => { setCopied(false); setExpanded(false); clearTimeout(timeout.current); }, [text]);
  const tokens = text.split(/("(?:\\.|[^"\\])*"\s*:|"(?:\\.|[^"\\])*"|\b-?\d+(?:\.\d+)?(?:e[+-]?\d+)?\b|\btrue\b|\bfalse\b|\bnull\b)/gi);
  return <div className="tool-json">
    <header><strong>{label}</strong><Button variant="ghost" aria-label={`${label} 복사`} icon={copied ? <Check size={14} /> : <Copy size={14} />} onClick={async () => {
      try {
        await navigator.clipboard.writeText(text);
        if (!mounted.current) return;
        setCopied(true); setCopyError(false); clearTimeout(timeout.current);
        timeout.current = setTimeout(() => setCopied(false), 1400);
      } catch { if (mounted.current) setCopyError(true); }
    }} /></header>
    <pre className="tool-json__preview" data-expanded={expanded}>{tokens.map((token, index) => <span key={index} className={/^".*:\s*$/.test(token) ? 'tool-json__key' : token.startsWith('"') ? 'tool-json__string' : /^-?\d/.test(token) ? 'tool-json__number' : /^(true|false|null)$/.test(token) ? 'tool-json__literal' : undefined}>{token}</span>)}</pre>
    {text.split('\n').length > 10 && <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? '접기' : '미리보기 전체 표시'}</Button>}
    <small>제한된 미리보기 · 최대 20줄 / 8,000자 · 배열 최대 20행</small>
    {copied && <span role="status">복사됨</span>}{copyError && <span role="alert">복사하지 못했습니다. 미리보기를 선택해 직접 복사하세요.</span>}
  </div>;
}

function Elapsed({ running, startedAt, durationMs }: { running: boolean; startedAt?: number; durationMs?: number }) {
  const [elapsed, setElapsed] = useState(durationMs ?? 0);
  useLayoutEffect(() => {
    if (!running) { setElapsed(durationMs ?? 0); return; }
    const start = startedAt ?? Date.now();
    const tick = () => setElapsed(Math.max(0, Date.now() - start));
    tick();
    const timer = setInterval(tick, 100);
    return () => clearInterval(timer);
  }, [running, startedAt, durationMs]);
  if (!running && durationMs === undefined) return null;
  return <span className="tool-call__elapsed" aria-hidden="true">{(elapsed / 1000).toFixed(1)}s</span>;
}

export function ToolCall({ name, summary, status, icon, arguments: args, result, error, durationMs, startedAt, decision, retrySafe = false, onRetry, ...accessibility }: ToolCallProps) {
  const failed = status === 'error' || decision === 'denied';
  const running = status === 'running' && !failed;
  const [manualOpen, setManualOpen] = useState<boolean>();
  const open = manualOpen ?? failed;
  const id = useId();
  return <article className="tool-call" data-status={failed ? 'error' : status} aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? name} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <Button bare type="button" className="tool-call__header" aria-expanded={open} aria-controls={id} onClick={() => setManualOpen(!open)}>
      <span className="tool-call__icon" aria-hidden="true">{icon ?? <Wrench size={16} />}</span><code className="tool-call__name">{name}</code><span className="tool-call__summary">{summary}</span>
      <span className="tool-call__state">{running ? <LoaderCircle size={14} className="ax-spinner" aria-hidden="true" /> : !failed && <Check size={14} aria-hidden="true" />}{failed ? decision === 'denied' ? '권한 거부' : '실패' : running ? '실행 중' : '완료'}{!failed && <Elapsed running={running} startedAt={startedAt} durationMs={durationMs} />}</span><ChevronDown size={14} className="ax-chevron" data-open={open} aria-hidden="true" />
    </Button>
    <div id={id} hidden={!open}>
      {open && <div className="tool-call__body ax-reveal">
        {decision && <p>권한: {decisionLabel(decision)}</p>}
        {args !== undefined && <JsonBlock label="인자" value={args} />}
        {result !== undefined && <JsonBlock label="결과" value={result} />}
        {failed && <div className="tool-call__error" role="alert"><p>{(error ?? (decision === 'denied' ? '권한 거부' : '도구 호출에 실패했습니다.')).slice(0, 2000)}</p>{retrySafe && decision !== 'denied' && onRetry && <Button onClick={onRetry}>재시도</Button>}</div>}
      </div>}
    </div>
  </article>;
}

export interface ToolCallGroupProps extends MicroAccessibility {
  calls: (ToolCallProps & { id: string })[];
  defaultOpen?: boolean;
}
export function ToolCallGroup({ calls, defaultOpen = false, ...accessibility }: ToolCallGroupProps) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  const running = calls.some(call => call.status === 'running' && call.decision !== 'denied');
  const failed = calls.filter(call => call.status === 'error' || call.decision === 'denied').length;
  return <section className="tool-group" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Tool activity'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <Button bare type="button" className="tool-group__header" aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}>
      <span className="tool-group__icons" aria-hidden="true">{calls.slice(-3).map(call => <span key={call.id}>{call.icon ?? <Wrench size={14} />}</span>)}</span>
      <span className="ax-button__label">{running ? '도구 사용 중…' : `도구 ${calls.length}개 사용${calls.length > 0 && calls.every(call => call.durationMs !== undefined) ? ` · ${(calls.reduce((sum, call) => sum + call.durationMs!, 0) / 1000).toFixed(1)}s` : ''}`}{running && <span className="ax-button__shine" aria-hidden="true" />}</span>
      {failed > 0 && <span className="tool-group__failed">실패 {failed}개</span>}<ChevronDown className="ax-chevron" data-open={open} size={14} aria-hidden="true" />
    </Button>
    <div id={id} hidden={!open}>{open && <div className="tool-group__body ax-reveal">{calls.map(call => <ToolCall key={call.id} {...call} />)}</div>}</div>
  </section>;
}

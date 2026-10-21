import { memo } from 'preact/compat';
import { highlightSegments } from './highlight';

/** Renders host-marked matches as text nodes; never as HTML. */
export const Marked = memo(function Marked({ highlight, text }: { highlight?: string; text: string }) {
  return <>{highlightSegments(highlight, text).map((part, i) => part.mark ? <mark key={i}>{part.text}</mark> : part.text)}</>;
});
const dateTime = new Intl.DateTimeFormat('ko-KR', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
export function when(value: string | number) { const d = new Date(value); return Number.isNaN(d.getTime()) ? '' : dateTime.format(d); }
export function fullWhen(value: string | number) { const d = new Date(value); return Number.isNaN(d.getTime()) ? '' : d.toLocaleString('ko-KR'); }
export function describeError(e: unknown) { return e && typeof e === 'object' && 'message' in e ? String((e as { message: unknown }).message) : String(e); }

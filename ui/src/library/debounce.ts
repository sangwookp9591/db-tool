/** Trailing debounce that only owns a timer while an input is pending; nothing runs when idle. */
export interface Debounced<A extends unknown[]> { (...args: A): void; cancel(): void; flush(): void; pending(): boolean }
export function debounce<A extends unknown[]>(fn: (...args: A) => void, delay = 250): Debounced<A> {
  let timer: ReturnType<typeof setTimeout> | undefined, last: A | undefined;
  const run = () => { timer = undefined; const args = last!; last = undefined; fn(...args); };
  const call = ((...args: A) => { last = args; if (timer !== undefined) clearTimeout(timer); timer = setTimeout(run, delay); }) as Debounced<A>;
  call.cancel = () => { if (timer !== undefined) clearTimeout(timer); timer = undefined; last = undefined; };
  call.flush = () => { if (timer !== undefined) { clearTimeout(timer); run(); } };
  call.pending = () => timer !== undefined;
  return call;
}

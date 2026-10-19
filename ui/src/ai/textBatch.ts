export function textBatch(update: (text: string) => void) {
  let text = '', timer: ReturnType<typeof setTimeout> | undefined, disposed = false;
  function flush() { clearTimeout(timer); timer = undefined; if (!disposed) update(text); }
  return {
    push(delta: string) { if (disposed) return; text += delta; if (timer === undefined) timer = setTimeout(flush, 50); },
    flush,
    dispose() { disposed = true; clearTimeout(timer); timer = undefined; text = ''; }
  };
}

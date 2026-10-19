export const CHECK_INTERVAL = 24 * 60 * 60 * 1000;
export function scheduleChecks(check: () => void) {
  let idle: number | undefined;
  const run = () => {
    if ('requestIdleCallback' in window) idle = window.requestIdleCallback(check, { timeout: 30_000 });
    else check();
  };
  let daily: ReturnType<typeof setInterval> | undefined;
  const first = setTimeout(() => { run(); daily = setInterval(run, CHECK_INTERVAL); }, 10_000);
  return () => { clearTimeout(first); clearInterval(daily); if (idle !== undefined) window.cancelIdleCallback(idle); };
}

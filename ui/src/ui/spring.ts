// React Bits interaction port (MIT + Commons Clause), original author David Haz.
export function reducedMotion() {
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

// Closed-form critical damping from rest, with a finite settling deadline.
export function springValue(from: number, to: number, elapsed: number, duration = 220) {
  const t = Math.max(0, elapsed) / 1000;
  const omega = 9.23 / (duration / 1000);
  return to + (from - to) * (1 + omega * t) * Math.exp(-omega * t);
}

export function spring(from: number, to: number, paint: (value: number) => void, duration = 220) {
  let frame = 0;
  if (from === to || reducedMotion()) {
    paint(to);
    return () => {};
  }
  const started = performance.now();
  const tick = (now: number) => {
    if (reducedMotion() || now - started >= duration) {
      paint(to);
      return;
    }
    paint(springValue(from, to, now - started, duration));
    frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
  return () => cancelAnimationFrame(frame);
}

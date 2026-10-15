import { frame, readGraphProbes, sampleFrames } from './index';
export async function graphInteraction(canvas: HTMLCanvasElement) {
  const rect = canvas.getBoundingClientRect();
  const frames = await sampleFrames(i => canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: i % 2 ? -20 : 20, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2, bubbles: true, cancelable: true })));
  await new Promise(resolve => setTimeout(resolve, 180)); await frame(); await frame();
  const before = readGraphProbes()[0];
  await new Promise(resolve => setTimeout(resolve, 500));
  const after = readGraphProbes()[0]; let movement = 0;
  for (let i = 0; i < before.positions.length; i += 2) movement += Math.hypot(before.positions[i] - after.positions[i], before.positions[i + 1] - after.positions[i + 1]);
  return { frames, idleRenderFrames: after.frames - before.frames, averageMovementPx: movement / Math.max(1, before.positions.length / 2), input: '60 alternating synthetic wheel events; rAF intervals (same W4 method)' };
}

import { forceSimulation, forceLink, forceManyBody, forceCenter, forceCollide, type SimulationNodeDatum } from 'd3-force';
interface Node extends SimulationNodeDatum { id: string; x: number; y: number; fixed?: boolean }
let simulation: ReturnType<typeof forceSimulation<Node>> | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let revision = 0, ticks = 0, workMs = 0, paused = false;
function positions() {
  if (!simulation) return;
  const values = new Float32Array(simulation.nodes().length * 2);
  simulation.nodes().forEach((node, i) => { values[i * 2] = node.x!; values[i * 2 + 1] = node.y!; });
  postMessage({ revision, positions: values, stable: true, ticks, workMs }, { transfer: [values.buffer] });
}
function step() {
  if (!simulation || paused) return;
  const start = performance.now();
  do { simulation.tick(); ticks++; } while (ticks < 300 && simulation.alpha() >= .01 && performance.now() - start < 8);
  workMs += performance.now() - start;
  if (ticks >= 300 || simulation.alpha() < .01) { simulation.stop(); positions(); }
  else timer = setTimeout(step, 0);
}
onmessage = event => {
  const message = event.data;
  clearTimeout(timer);
  if (message.kind === 'pause') { paused = true; return; }
  if (message.kind === 'resume') { paused = false; if (simulation && ticks < 300 && simulation.alpha() >= .01) step(); return; }
  if (message.kind === 'pin') { const node = simulation?.nodes().find(n => n.id === message.id); if (node) { node.fx = node.x = message.x; node.fy = node.y = message.y; node.vx = node.vy = 0; } positions(); return; }
  revision = message.revision; ticks = 0; workMs = 0; paused = false;
  const nodes: Node[] = message.nodes.map((n: Node) => ({ ...n, ...(n.fixed ? { fx: n.x, fy: n.y } : {}) }));
  simulation?.stop();
  simulation = forceSimulation(nodes).stop().alpha(message.incremental ? .12 : 1).alphaMin(.01).alphaDecay(.08).velocityDecay(.7).force('link', forceLink<Node, { source: string; target: string }>(message.links).id(n => n.id).distance((message.radius ?? 40)*3.5)).force('charge', forceManyBody().strength(-250)).force('center', forceCenter(0, 0)).force('collide', forceCollide(message.radius ?? 40));
  step();
};

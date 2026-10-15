import { forceSimulation, forceLink, forceManyBody, forceCenter, forceCollide, type SimulationNodeDatum } from 'd3-force';
interface Node extends SimulationNodeDatum { id: string; x: number; y: number; fixed?: boolean }
let simulation: ReturnType<typeof forceSimulation<Node>> | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let revision = 0;
function step() {
  if (!simulation) return;
  simulation.tick(5);
  const positions = new Float32Array(simulation.nodes().length * 2);
  simulation.nodes().forEach((node, i) => { positions[i * 2] = node.x!; positions[i * 2 + 1] = node.y!; });
  const stable = simulation.alpha() < simulation.alphaMin();
  postMessage({ revision, positions, stable }, { transfer: [positions.buffer] });
  if (!stable) timer = setTimeout(step, 0);
}
onmessage = event => {
  const message = event.data;
  clearTimeout(timer);
  if (message.kind === 'pause') return;
  if (message.kind === 'resume') { step(); return; }
  if (message.kind === 'pin') { const node = simulation?.nodes().find(n => n.id === message.id); if (node) { node.fx = node.x = message.x; node.fy = node.y = message.y; } step(); return; }
  revision = message.revision;
  const nodes: Node[] = message.nodes.map((n: Node) => ({ ...n, ...(n.fixed ? { fx: n.x, fy: n.y } : {}) }));
  simulation?.stop();
  simulation = forceSimulation(nodes).stop().alpha(message.incremental ? .12 : 1).force('link', forceLink<Node, { source: string; target: string }>(message.links).id(n => n.id).distance(140)).force('charge', forceManyBody().strength(-250)).force('center', forceCenter(0, 0)).force('collide', forceCollide(40));
  step();
};

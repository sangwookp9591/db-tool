import type { SchemaGraph } from './contract';
import { curvePoint, type EdgeCurve } from './graph';

interface Node { id: string; x: number; y: number; color: string; visible: boolean }
interface Link { curve: EdgeCurve; label: string }
interface Palette { surface: string; text: string; edge: string; selected: string; font: string }
export const xml = (text: string) => text.replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[char]!);
export function schemaSvg(schema: SchemaGraph, nodes: Node[], links: Link[], camera: { x: number; y: number; scale: number }, width: number, height: number, palette: Palette, background?: string): string {
  const text = (x: number, y: number, value: string, color = palette.text) => `<text x="${x}" y="${y}" fill="${xml(color)}">${xml(value)}</text>`;
  const body = links.map(({ curve: g, label }) => {
    const d = `M ${g.a.x} ${g.a.y} ${g.d ? `C ${g.c.x} ${g.c.y} ${g.d.x} ${g.d.y}` : `Q ${g.c.x} ${g.c.y}`} ${g.b.x} ${g.b.y}`;
    const p = curvePoint(g, .5);
    return `<path d="${d}" fill="none" stroke="${xml(palette.edge)}" stroke-width="${1.2 / camera.scale}" marker-end="url(#arrow)"/>${text(p.x, p.y - 5, label)}`;
  }).join('') + nodes.filter(n => n.visible).map(n => {
    const entity = schema.entities.find(e => e.id === n.id);
    if (!entity) return '';
    const h = 36 + entity.fields.length * 18, left = n.x - 120, top = n.y - h / 2;
    return `<g text-anchor="start"><rect x="${left}" y="${top}" width="240" height="${h}" rx="8" fill="${xml(palette.surface)}" stroke="${xml(n.color)}" stroke-width="${2 / camera.scale}"/>${text(left + 8, top + 20, entity.name.slice(0, 32))}${entity.fields.map((f, i) => {
      const y = top + 40 + i * 18;
      return (f.pk ? text(left + 8, y, 'PK', palette.selected) : '') + text(left + (f.pk ? 34 : 8), y, f.name.slice(0, 18)) + text(left + 150, y, `${f.type_name}${f.nullable ? ' ?' : ''}`.slice(0, 16), palette.edge);
    }).join('')}</g>`;
  }).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M 0 0 L 8 4 L 0 8 Z" fill="${xml(palette.edge)}"/></marker></defs>${background ? `<rect width="100%" height="100%" fill="${xml(background)}"/>` : ''}<g transform="translate(${width / 2 + camera.x} ${height / 2 + camera.y}) scale(${camera.scale})" font-family="${xml(palette.font)}" font-size="11" text-anchor="middle">${body}</g></svg>`;
}
export async function canvasPng(source: HTMLCanvasElement, background?: string): Promise<Uint8Array> {
  const canvas = document.createElement('canvas'); canvas.width = source.width; canvas.height = source.height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('PNG 캔버스를 만들지 못했습니다.');
  if (background) { context.fillStyle = background; context.fillRect(0, 0, canvas.width, canvas.height); }
  context.drawImage(source, 0, 0);
  const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('PNG 변환에 실패했습니다.')), 'image/png'));
  return new Uint8Array(await blob.arrayBuffer());
}

// Status bar label "<Product> <version>"; the driver's full string stays available as the tooltip.
export function shortServerVersion(info: string | null | undefined, fallback: string): string {
  const line = info?.split('\n')[0]?.trim();
  if (!line) return fallback;
  const product = line.match(/^[A-Za-z][A-Za-z0-9_-]*/)?.[0];
  const version = line.match(/\d+\.\d+(?:\.\d+)*/)?.[0];
  return product && version ? `${product} ${version}` : line.length > 40 ? `${line.slice(0, 39)}…` : line;
}
export function serverVersionTitle(info: string | null | undefined): string | undefined {
  return info?.split('\n')[0]?.trim() || undefined;
}

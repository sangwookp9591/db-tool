import { describe, expect, it } from 'vitest';
import { serverVersionTitle, shortServerVersion } from './serverVersion';

describe('shortServerVersion', () => {
  it('keeps product and version only', () => {
    expect(shortServerVersion('PostgreSQL 16.10 on aarch64-unknown-linux-musl, compiled by gcc (Alpine 14.2.0) 14.2.0, 64-bit', 'PostgreSQL')).toBe('PostgreSQL 16.10');
    expect(shortServerVersion('Neo4j [[Text("Neo4j Kernel"), List([[Text("5.26.30")]])]]', 'Neo4j')).toBe('Neo4j 5.26.30');
  });
  it('falls back to the driver name or a clipped line', () => {
    expect(shortServerVersion(undefined, 'Redis')).toBe('Redis');
    expect(shortServerVersion('custom build without numbers that is rather long indeed', 'X')).toHaveLength(40);
  });
  it('titles with the full first line', () => {
    expect(serverVersionTitle('PostgreSQL 16.10 on x\nsecond')).toBe('PostgreSQL 16.10 on x');
    expect(serverVersionTitle(null)).toBeUndefined();
  });
});

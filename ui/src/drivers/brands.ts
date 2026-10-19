import postgres from '../assets/drivers/postgres.svg?raw';
import neo4j from '../assets/drivers/neo4j.svg?raw';
import mongodb from '../assets/drivers/mongodb.svg?raw';
import redis from '../assets/drivers/redis.svg?raw';
import sqlite from '../assets/drivers/sqlite.svg?raw';
import database from '../assets/drivers/database.svg?raw';
const images: Record<string, string> = Object.fromEntries(Object.entries({ postgres, neo4j, mongodb, redis, sqlite, database }).map(([id, svg]) => [id, `data:image/svg+xml,${encodeURIComponent(svg)}`]));
const colors: Record<string, string> = { postgres: 'blue', neo4j: 'teal', mongodb: 'green', redis: 'red', sqlite: 'indigo' };
export function driverBrand(id: string) { return { image: images[id] ?? images.database, color: colors[id] ?? 'graphite' }; }

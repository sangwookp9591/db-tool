import postgres from '../assets/drivers/postgres.webp';
import neo4j from '../assets/drivers/neo4j.webp';
import mongodb from '../assets/drivers/mongodb.webp';
import redis from '../assets/drivers/redis.webp';
import sqlite from '../assets/drivers/sqlite.webp';
import mariadb from '../assets/drivers/mariadb.webp';
import mysql from '../assets/drivers/mysql.webp';
const brands: Record<string, { image: string; color: string; accent?: string }> = {
  postgres: { image: postgres, color: '#336791' },
  mysql: { image: mysql, color: '#00758F', accent: '#F29111' },
  mariadb: { image: mariadb, color: '#003545' },
  neo4j: { image: neo4j, color: '#018BFF' },
  mongodb: { image: mongodb, color: '#00684A' },
  redis: { image: redis, color: '#C6302B' },
  sqlite: { image: sqlite, color: '#044A64' },
};
export function driverBrand(iconKey?: string) { return Object.hasOwn(brands, iconKey ?? '') ? brands[iconKey!] : undefined; }

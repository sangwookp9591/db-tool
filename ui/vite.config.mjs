import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

const root = fileURLToPath(new URL('..', import.meta.url));
function git(...args) {
  try { return execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim(); }
  catch { return ''; }
}
const notesDir = new URL('../docs/release-notes/', import.meta.url);
const releaseDates = Object.fromEntries(readdirSync(notesDir).filter(name => /^v.*\.md$/.test(name)).map(name => {
  const version = name.slice(1, -3);
  const source = readFileSync(new URL(name, notesDir), 'utf8');
  const date = /^<!--\s*date:\s*(\d{4}-\d{2}-\d{2})\s*-->$/m.exec(source)?.[1]
    ?? git('for-each-ref', '--format=%(creatordate:short)', `refs/tags/v${version}`);
  if (!date || Number.isNaN(Date.parse(date)) || new Date(date).toISOString().slice(0, 10) !== date) throw new Error(`${name}: add <!-- date: YYYY-MM-DD --> or a release tag with a valid date`);
  return [version, date];
}));
export default defineConfig({
  build: { target: 'es2022', assetsInlineLimit: 0 }, clearScreen: false,
  server: { fs: { allow: [fileURLToPath(new URL('.', import.meta.url)), fileURLToPath(notesDir)] } },
  define: {
    'import.meta.env.AXDB_BUILD_COMMIT': JSON.stringify(git('rev-parse', '--short=7', 'HEAD') || '확인 불가'),
    'import.meta.env.AXDB_RELEASE_DATES': JSON.stringify(releaseDates),
  },
});

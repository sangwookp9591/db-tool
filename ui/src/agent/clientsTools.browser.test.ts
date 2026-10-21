import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, webkit, type Browser } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
import { clientCommand, clientConfig, shellQuote } from './ClientConnectGuide';

declare const process: { env: Record<string, string | undefined> };
declare global { interface Window { __copy?: { value: string; message: string } } }
let server: ViteDevServer, browser: Browser, base: string;

beforeAll(async () => {
  server = await createServer({ configFile: false, plugins: [{
    name: 'client-tools-probe',
    configureServer(vite) { vite.middlewares.use('/client-tools-probe', (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end('<html><body><main id="root"></main></body></html>'); }); },
    resolveId(id) { if (id === 'virtual:client-tools-probe') return id; },
    load(id) {
      if (id !== 'virtual:client-tools-probe') return;
      return `import {h,render} from 'preact';import {useState} from 'preact/hooks';import {ToolAccessList} from '/src/agent/ToolAccessList.tsx';import {ClientConnectGuide} from '/src/agent/ClientConnectGuide.tsx';import {SpringCheck} from '/src/ui/SpringCheck.tsx';import '/src/theme.css';import '/src/style.css';
        function Probe(){const [allowed,setAllowed]=useState(['list_connections','run_query']);return h('div',{class:'probe'},h('div',{class:'agent-clients'},h(ToolAccessList,{allowed,onChange:setAllowed}),h('output',{id:'allowed'},allowed.join(',')),h(SpringCheck,{checked:true,disabled:true,label:'잠긴 설정'}),h(ClientConnectGuide,{client:'QA Codex',url:'http://127.0.0.1:47831/mcp',token:'secret-test-token',onCopy:(value,message)=>{window.__copy={value,message}}})));}render(h(Probe),document.querySelector('#root'));`;
    },
  }], optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-client-tools-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen(); base = server.resolvedUrls!.local[0];
  browser = await (process.env.AXDB_QA_BROWSER==='webkit'?webkit:chromium).launch({ headless: true,...(process.env.AXDB_QA_WEBKIT_PATH?{executablePath:process.env.AXDB_QA_WEBKIT_PATH}:{}) });
}, 30000);

afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

it('keeps check indicators clipped and aligned, and toggles by label, keyboard, and disabled state', async () => {
  const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce' });
  try {
    await page.goto(base + 'client-tools-probe');
    await page.addScriptTag({ type: 'module', url: '/@id/virtual:client-tools-probe' });
    const checkbox = page.getByRole('checkbox', { name: '연결 목록 (list_connections)' });
    expect(await checkbox.isChecked()).toBe(true);
    await page.locator('.agent-tools__label').filter({ hasText: '연결 목록' }).click();
    expect(await checkbox.isChecked()).toBe(false);
    expect(await page.locator('#allowed').innerText()).not.toContain('list_connections');
    await checkbox.focus(); await page.keyboard.press('Space');
    expect(await checkbox.isChecked()).toBe(true);
    const locked = page.getByRole('checkbox', { name: '잠긴 설정' });
    expect(await locked.isDisabled()).toBe(true);
    expect(await locked.isChecked()).toBe(true);
    await locked.click({ force: true });
    expect(await locked.isChecked()).toBe(true);

    const alignment = await page.locator('.agent-tools__option .spring-check').evaluateAll(labels => labels.map(label => {
      const input = label.querySelector('input')!.getBoundingClientRect();
      const box = label.querySelector('.spring-check__box')!.getBoundingClientRect();
      const text = label.querySelector('.agent-tools__label')!.getBoundingClientRect();
      return { delta: input.left + input.width / 2 - (box.left + box.width / 2), text: text.left - box.right };
    }));
    expect(alignment.every(item => Math.abs(item.delta) < 1 && item.text >= 0), JSON.stringify(alignment.slice(0, 3))).toBe(true);
    const metrics = await page.locator('.agent-tools').evaluate(el => {
      const style = getComputedStyle(el), options = [...el.querySelectorAll<HTMLElement>('.agent-tools__option')];
      const bounds = el.getBoundingClientRect();
      return { columns: style.gridTemplateColumns.split(' ').length, heights: options.map(option => option.getBoundingClientRect().height), count: options.length, overflow: options.some(option => { const rect = option.getBoundingClientRect(); return rect.left < bounds.left || rect.right > bounds.right; }) };
    });
    expect(metrics.columns).toBeGreaterThanOrEqual(3);
    expect(metrics.heights.every(height => height <= 48)).toBe(true);
    expect(metrics.count).toBe(19);
    expect(metrics.overflow).toBe(false);
  } finally { await page.close(); }
}, 30000);

it('copies a client setup guide with a token placeholder and offers the issued token only on explicit settings copy', async () => {
  const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce' });
  try {
    await page.goto(base + 'client-tools-probe');
    await page.addScriptTag({ type: 'module', url: '/@id/virtual:client-tools-probe' });
    const visible = await page.locator('.client-connect-guide__code').innerText();
    expect(visible).toContain('--bearer-token-env-var AXDB_MCP_TOKEN');
    expect(visible).not.toContain('secret-test-token');
    await page.getByRole('button', { name: 'Claude Code' }).click();
    expect(await page.locator('.client-connect-guide__code').innerText()).toContain('claude mcp add --transport http');
    await page.getByRole('button', { name: '연결 방법 복사' }).click();
    const copiedGuide = await page.evaluate(() => window.__copy?.value ?? '');
    expect(copiedGuide).toContain('claude mcp add');
    expect(copiedGuide).not.toContain('secret-test-token');
    await page.getByRole('button', { name: '설정 복사' }).click();
    const copiedConfig = JSON.parse(await page.evaluate(() => window.__copy?.value ?? ''));
    expect(copiedConfig.mcpServers.axdb.headers.Authorization).toBe('Bearer secret-test-token');
    await page.getByRole('button', { name: 'Codex', exact: true }).click();
    await page.getByRole('button', { name: '설정 복사', exact: true }).click();
    expect(await page.evaluate(() => window.__copy?.value)).toBe('[mcp_servers.axdb]\nurl = \"http://127.0.0.1:47831/mcp\"\nbearer_token_env_var = \"AXDB_MCP_TOKEN\"');
    if(process.env.AXDB_QA_SCREENSHOTS)await page.screenshot({path:`${process.env.AXDB_QA_SCREENSHOTS}/mcp-tools-connect.png`});
  } finally { await page.close(); }
}, 30000);

it('escapes shell arguments in provider commands', () => {
  expect(shellQuote("http://localhost/path?q=one'two")).toBe("'http://localhost/path?q=one'\\''two'");
  expect(clientCommand('codex', 'http://127.0.0.1:1/mcp')).toContain("--url 'http://127.0.0.1:1/mcp'");
});

it('provides reusable settings without an issued token',()=>{expect(clientConfig('codex','http://localhost/mcp')).toContain('bearer_token_env_var');expect(JSON.parse(clientConfig('json','http://localhost/mcp')).mcpServers.axdb.headers.Authorization).toBe('Bearer <발급된 토큰>');});

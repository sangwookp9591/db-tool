import { afterAll, afterEach, beforeAll, expect, it } from 'vitest';
import { chromium, webkit, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';

declare const process: { platform: string; env: Record<string, string | undefined>; getBuiltinModule(name: string): { existsSync(path: string): boolean; readdirSync(path: string): string[]; homedir(): string; join(...parts: string[]): string } };

let server: ViteDevServer;
let browser: Browser;
let page: Page;
const fs = process.getBuiltinModule('fs'), os = process.getBuiltinModule('os'), path = process.getBuiltinModule('path');
const browserCache = process.env.PLAYWRIGHT_BROWSERS_PATH || path.join(os.homedir(), process.platform === 'darwin' ? 'Library/Caches/ms-playwright' : '.cache/ms-playwright');
const cachedWebKit = fs.existsSync(browserCache) ? fs.readdirSync(browserCache).filter(name => name.startsWith('webkit-')).map(name => path.join(browserCache, name, 'pw_run.sh')).find(fs.existsSync) : undefined;

beforeAll(async () => {
  server = await createServer({ configFile: false, plugins: [{
    name: 'profile-form-probe',
    configureServer(server) { server.middlewares.use('/profile-form-probe', (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end('<html><body><main id="root"></main></body></html>'); }); },
    resolveId(id, importer) { if (id === 'virtual:profile-form-probe' || id === 'virtual:profile-form-api') return id; if ((id === './api' && importer?.endsWith('/ProfileForm.tsx')) || (id === '../api' && importer?.endsWith('/ConnectionNetwork.tsx'))) return 'virtual:profile-form-api'; },
    load(id) {
      if (id === 'virtual:profile-form-api') return `
        export const isMock=true;
        const capabilities={lint:false,import_rows:false,schema_snapshot:false,ddl_render:false,roles:false,backup:false,explain:false,schema_graph:false,preview_changes:'None',read_only_exec:false,cancel:'None',timeout:false,params:false,transactions:false,paged_browse:false,graph_expand:false,multi_statement:false,edit_rows:false};
        const fields=[{key:'host',label:'Host',kind:'Text',required:true,default:'localhost',secret:false},{key:'port',label:'Port',kind:'Number',required:true,default:'5432',secret:false},{key:'password',label:'Password',kind:'Password',required:true,default:null,secret:true}];
        export const drivers=[{id:'postgres',icon_key:'postgres',display_name:'PostgreSQL · 데모',paradigm:'Relational',query_language:{id:'sql',display:'SQL',editor_mode:'sql',supports_params:false},connection_fields:fields,capabilities,default_query:''},{id:'neo4j',icon_key:'neo4j',display_name:'그래프 · 데모',paradigm:'Graph',query_language:{id:'cypher',display:'Cypher',editor_mode:'cypher',supports_params:false},connection_fields:fields.map(f=>f.key==='port'?{...f,default:'7687'}:f),capabilities,default_query:''}];
        export async function api(command,args={}) { (window.__apiCalls??=[]).push({command,args});if(command==='drivers_list')return drivers;if(command==='iam_profiles')return {aws:[],gcp:[]};if(command==='connection_test_steps'){window.__testedNetworkSecrets=args.networkSecrets;for(const stage of ['dns','ssh','tunnel','tls','db_auth','permissions'])args.onEvent?.onmessage({event:'stage',stage,status:'passed',elapsed_ms:1,code:null,help_key:null});return 'Mock connection';}if(command==='profile_save'){window.__savedProfile=args.profile;window.__submittedSecret=args.secret;return args.profile;}if(command==='connection_credentials_save')return;if(command==='connection_parse')return {driver_id:'postgres',fields:{host:'db.example',port:'5432'},network:null,has_secret:true,sanitized_input:'db.example:5432',secret:'pasted-secret'};throw Error('Unexpected API command '+command); }
      `;
      if (id === 'virtual:profile-form-probe') return `import {h,render} from 'preact';import {ProfileForm} from '/src/ProfileForm.tsx';import {drivers} from 'virtual:profile-form-api';window.__apiCommands=[];window.__testSecretKeys=[];window.addEventListener('ax:mock-command',event=>window.__apiCommands.push(event.detail.command));window.addEventListener('ax:mock-test-secrets',event=>window.__testSecretKeys=event.detail.keys);const profile=window.__editingProfile?{id:'edit-fixture',name:'Existing profile',driver_id:'postgres',fields:{host:'db.example',port:'5432'},save_secret:true,access_mode:'read_write'}:undefined;render(h(ProfileForm,{profile,drivers,onClose:()=>{},onSaved:async()=>{window.__saveComplete=true;if(window.__failRefresh)throw new Error('profiles refresh failed')},onError:()=>{}}),document.querySelector('#root'));`;
    },
  }], optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-profile-form-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen();
  browser = await chromium.launch({ headless: true });
}, 30000);

afterEach(async () => { await page?.close(); });
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

async function openForm(editExisting = false) {
  page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 1000, height: 760 } });
  page.setDefaultTimeout(4000);
  await page.goto(server.resolvedUrls!.local[0] + 'profile-form-probe');
  if (editExisting) await page.evaluate(() => { (window as any).__editingProfile = true; });
  await page.addScriptTag({ type: 'module', url: '/@id/virtual:profile-form-probe' });
  await page.getByRole('dialog', { name: editExisting ? 'Edit connection' : 'Add connection' }).waitFor();
}

async function checksClosedNetworkValidation(target: Page) {
  page = target;
  await page.goto(server.resolvedUrls!.local[0] + 'profile-form-probe');
  await page.addScriptTag({ type: 'module', url: '/@id/virtual:profile-form-probe' });
  await page.getByRole('dialog', { name: 'Add connection' }).waitFor();
  await page.getByLabel('Name', { exact: true }).fill('network validation');
  await page.getByRole('button', { name: '2. 네트워크' }).click();
  await page.getByLabel('SSH 터널', { exact: true }).selectOption('ssh');
  await page.getByRole('button', { name: '1. 기본' }).click();
  await page.getByRole('button', { name: 'Test connection' }).click();
  await page.getByLabel('SSH 호스트').waitFor({ state: 'visible' });
  expect(await page.locator('.ax-steps [aria-current="step"]').getAttribute('aria-label')).toBe('2. 네트워크');
  expect(await page.locator('[data-status="success"]').count()).toBe(0);
}

it('validates required network fields even when the user has left that step closed', async () => {
  page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 1000, height: 760 } });
  page.setDefaultTimeout(4000);
  await checksClosedNetworkValidation(page);
}, 30000);

it.skipIf(!cachedWebKit)('reveals required fields after native validation in WebKit', async () => {
  const webkitBrowser = await webkit.launch({ headless: true, ...(cachedWebKit ? { executablePath: cachedWebKit } : {}) });
  try {
    await checksClosedNetworkValidation(await webkitBrowser.newPage({ reducedMotion: 'reduce', viewport: { width: 1000, height: 760 } }));
  } finally { await webkitBrowser.close(); }
}, 30000);

it('clears a successful test when the driver changes and applies that driver defaults', async () => {
  await openForm();
  await page.getByLabel('Name', { exact: true }).fill('driver reset');
  await page.getByRole('button', { name: '3. 인증' }).click();
  await page.getByLabel('Password', { exact: true }).fill('temporary');
  await page.getByRole('button', { name: 'Test connection' }).click();
  await page.locator('.ax-test-status[data-status="success"]').waitFor();
  await page.getByRole('button', { name: '1. 기본' }).click();
  await page.getByRole('combobox', { name: 'Driver' }).click();
  await page.getByRole('option', { name: '그래프 · 데모' }).click();
  expect(await page.locator('.ax-test-status').count()).toBe(0);
  expect(await page.getByLabel('Port', { exact: true }).inputValue()).toBe('7687');
}, 30000);

it('saves a new profile through the form submit action', async () => {
  await openForm();
  await page.getByLabel('Name', { exact: true }).fill('saved connection');
  await page.getByRole('button', { name: '3. 인증' }).click();
  await page.getByLabel('Password', { exact: true }).fill('new-secret');
  await page.getByRole('button', { name: '저장', exact: true }).click();
  await page.waitForFunction(() => (window as any).__saveComplete === true);
  expect(await page.locator('.connection-dialog').count()).toBe(1);
});

it('preserves the stored password when editing without entering a replacement', async () => {
  await openForm(true);
  await page.getByRole('button', { name: '3. 인증' }).click();
  expect(await page.getByLabel('Password', { exact: true }).getAttribute('placeholder')).toContain('비워 두면 저장된 값을 유지합니다');
  await page.getByRole('button', { name: '저장', exact: true }).click();
  await page.waitForFunction(() => (window as any).__saveComplete === true);
  expect(await page.locator('.connection-dialog').count()).toBe(1);
});

it('keeps pasted and network credentials in the draft through test and persists them only after save', async () => {
  await openForm();
  await page.getByLabel('Name', { exact: true }).fill('draft credentials');
  await page.getByLabel('스마트 붙여넣기').fill('postgres://alice:pasted-secret@db.example:5432/app');
  await page.getByRole('button', { name: '안전하게 채우기' }).click();
  await page.waitForFunction(() => (document.querySelector('[aria-label="Password"]') as HTMLInputElement | null)?.value === 'pasted-secret');
  await page.getByRole('button', { name: '3. 인증' }).click();
  expect(await page.getByLabel('Password', { exact: true }).inputValue()).toBe('pasted-secret');
  await page.getByRole('button', { name: '2. 네트워크' }).click();
  await page.getByLabel('프록시 종류').selectOption('socks5');
  await page.getByLabel('프록시 호스트').fill('proxy.example');
  await page.getByLabel('프록시 비밀번호').fill('proxy-draft-secret');
  await page.getByRole('button', { name: '3. 인증' }).click();
  const invalid = await page.locator('form :invalid').evaluateAll(elements => elements.map(element => ({ label: element.getAttribute('aria-label') || element.parentElement?.textContent, value: (element as HTMLInputElement).value })));
  expect(invalid).toEqual([]);
  await page.getByRole('button', { name: 'Test connection' }).click();
  await page.locator('.ax-test-status').waitFor();
  expect(await page.locator('.ax-test-status').getAttribute('data-status')).toBe('success');
  expect(await page.evaluate(() => (window as any).__testSecretKeys)).toContain('proxy_password');
  expect(await page.evaluate(() => (window as any).__apiCommands)).not.toContain('connection_credentials_save');
  expect(await page.evaluate(() => (window as any).__apiCommands)).not.toContain('profile_save');
  await page.getByRole('button', { name: '저장', exact: true }).click();
  await page.waitForFunction(() => (window as any).__saveComplete === true);
  const calls = await page.evaluate(() => (window as any).__apiCommands);
  expect(calls.indexOf('connection_credentials_save')).toBeGreaterThan(calls.indexOf('profile_save'));
  expect(calls).toContain('connection_test_steps');
}, 30000);

it('does not persist draft network credentials when the form is canceled', async () => {
  await openForm();
  await page.getByLabel('Name', { exact: true }).fill('canceled credentials');
  await page.getByRole('button', { name: '2. 네트워크' }).click();
  await page.getByLabel('프록시 종류').selectOption('socks5');
  await page.getByLabel('프록시 호스트').fill('proxy.example');
  await page.getByLabel('프록시 비밀번호').fill('proxy-draft-secret');
  await page.getByRole('button', { name: '취소' }).click();
  expect(await page.evaluate(() => (window as any).__apiCommands)).not.toContain('connection_credentials_save');
}, 30000);

it('does not persist a pasted replacement when editing an existing profile and canceling', async () => {
  await openForm(true);
  await page.getByLabel('스마트 붙여넣기').fill('postgres://alice:replacement-secret@db.example:5432/app');
  await page.getByRole('button', { name: '안전하게 채우기' }).click();
  await page.getByRole('button', { name: '3. 인증' }).click();
  await page.waitForFunction(() => (document.querySelector('[aria-label="Password"]') as HTMLInputElement | null)?.value === 'replacement-secret');
  await page.getByRole('button', { name: '취소' }).click();
  const commands = await page.evaluate(() => (window as any).__apiCommands);
  expect(commands).not.toContain('profile_save');
  expect(commands).not.toContain('connection_credentials_save');
}, 30000);

it('shows save callback failures inside the open form', async () => {
  await openForm();
  await page.getByLabel('Name', { exact: true }).fill('refresh failure');
  await page.getByRole('button', { name: '3. 인증' }).click();
  await page.getByLabel('Password', { exact: true }).fill('temporary');
  await page.evaluate(() => { (window as any).__failRefresh = true; });
  await page.getByRole('button', { name: '저장', exact: true }).click();
  const error = page.locator('.connection-dialog .db-error[role="alert"]');
  await error.waitFor();
  expect(await error.textContent()).toContain('profiles refresh failed');
});

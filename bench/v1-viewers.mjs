import {chromium} from '../ui/node_modules/playwright/index.mjs';
import {writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const browser=await chromium.launch({headless:true});
const output={complete:false,errors:[],checks:[]};
try {
 const page=await browser.newPage({viewport:{width:1440,height:1000}});page.on('pageerror',e=>output.errors.push(e.message));
 await page.goto('http://127.0.0.1:1426/viewer-bench.html');await page.waitForFunction(()=>window.axdb?.perf);
 output.graph=await page.evaluate(()=>window.axdb.perf.run('graph-panzoom-300'));
 assert.equal(output.graph.idleRenderFrames,0);assert.equal(output.graph.averageMovementPx,0);assert.ok(output.graph.frames.p95<=16.8,`pan/zoom p95 ${output.graph.frames.p95}`);
 output.checks.push('300 nodes: pan/zoom p95 <=16.8ms, no idle renders or movement');
 output.documents=await page.evaluate(()=>window.axdb.perf.run('documents-5000'));
 assert.ok(output.documents.rendered<60);output.checks.push('5000 document roots keep fewer than 60 DOM rows');
 await page.getByLabel('Expand Document 1',{exact:true}).click();await page.getByLabel('Expand a',{exact:true}).click();await page.getByLabel('Expand a.b',{exact:true}).click();
 assert.match(await page.getByLabel('Document tree').innerText(),/9223372036854775807/);assert.match(await page.getByLabel('Document tree').innerText(),/ObjectId/);
 await page.evaluate(()=>{window.__copiedPath='';Object.defineProperty(navigator,'clipboard',{value:{writeText:async p=>{window.__copiedPath=p;}},configurable:true});});await page.getByLabel('Copy path a.b[1]',{exact:true}).click();assert.equal(await page.evaluate(()=>window.__copiedPath),'a.b[1]');
 output.checks.push('typed precision, expansion, nested field-path copying');await page.screenshot({path:new URL('./v1-screenshots/documents-5000.png',import.meta.url).pathname});
 await page.evaluate(()=>window.axdb.perf.run('linked-viewers'));
 await page.getByText('Nodes & relationships (keyboard access)',{exact:true}).click();await page.getByRole('button',{name:'Node 0',exact:true}).click();await page.getByLabel('Show Table results',{exact:true}).click();
 assert.match(await page.locator('.table-row.selected').innerText(),/Node 0/);
 await page.getByRole('gridcell').nth(1).click();await page.getByLabel('Show Graph results',{exact:true}).click();await page.getByLabel('Graph properties',{exact:true}).waitFor();assert.match(await page.getByLabel('Graph properties',{exact:true}).innerText(),/bench:n1/);
 await page.getByLabel('Show JSON results',{exact:true}).click();await page.locator('.documents').getByLabel('Expand Document 1',{exact:true}).click();await page.locator('.documents').getByRole('button',{name:'node',exact:true}).click();await page.getByLabel('Show Table results',{exact:true}).click();assert.equal(await page.locator('.column-header.selected').count(),1);
 output.checks.push('Graph→Table row selection, Table→Graph properties, JSON field→table column highlight');
 assert.deepEqual(output.errors,[]);output.complete=true;console.log({complete:output.complete,documents:output.documents,graph:output.graph.frames,checks:output.checks});
} catch(e){output.errors.push(String(e));throw e;}
finally{await writeFile(new URL('./results-v1-viewers.json',import.meta.url),JSON.stringify(output,null,2));await browser.close();}

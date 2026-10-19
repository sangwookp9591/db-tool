import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch({headless:true});
const base=process.env.AX_UI_URL??'http://127.0.0.1:51977';
const output=new URL('../docs/design-shots/',import.meta.url).pathname;
const errors=[];
try{for(const theme of ['midnight','daylight']){
 const context=await browser.newContext({viewport:{width:1280,height:900},reducedMotion:'reduce'});
 await context.addInitScript(theme=>localStorage.setItem('axdb-theme',theme),theme);
 const page=await context.newPage();page.on('pageerror',error=>errors.push(error.message));await page.goto(base);
 await page.getByRole('button',{name:'Workspace settings',exact:true}).waitFor();
 await page.evaluate(()=>window.dispatchEvent(new Event('ax:accounts')));
 for(const [provider,name] of [['claude','Claude'],['chatgpt','ChatGPT']]){
   await page.getByRole('button',{name:`${name}로 로그인`,exact:true}).click();
   const modal=page.getByRole('dialog',{name:`${name} 연결`,exact:true});await modal.waitFor();
   await modal.getByRole('button',{name:'연결',exact:true}).click();
   await modal.getByRole('status').waitFor();
   if(provider==='claude')await modal.getByLabel('Claude 인증 코드',{exact:true}).waitFor();
   await page.screenshot({path:`${output}w7-connect-${provider}-${theme}.png`});
   if(provider==='claude')await modal.getByLabel('Claude 인증 코드',{exact:true}).evaluate(input=>{const data=new DataTransfer();data.setData('text/plain','mock-code-not-a-real-token');input.dispatchEvent(new ClipboardEvent('paste',{bubbles:true,clipboardData:data}));});
   await modal.getByText(`${name} 계정이 연결되었습니다. 모델 선택기에서 사용할 모델을 고르세요.`,{exact:true}).waitFor();
   await modal.getByRole('button',{name:'닫기',exact:true}).click();
 }
 await context.close();
}assert.deepEqual(errors,[]);console.log('W7 connect modal mock E2E passed: Claude code paste, ChatGPT polling, four theme captures.');}finally{await browser.close();}

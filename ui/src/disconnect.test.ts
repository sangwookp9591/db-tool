import {it,expect} from 'vitest';
import {disconnectedTabs} from './disconnect';
it('keeps open tab identity and edited text, clears runs only for the disconnected connection',()=>{
 const tabs=[{id:'a',profileId:'p',text:'old',runId:'r',dirty:true},{id:'b',profileId:'q',text:'other',runId:'s',dirty:false}];
 const next=disconnectedTabs(tabs,'p',id=>id==='a'?'SELECT 42':undefined);
 expect(next).toEqual([{...tabs[0],text:'SELECT 42',runId:null},tabs[1]]);expect(next[1]).toBe(tabs[1]);expect(tabs[0].text).toBe('old');
});

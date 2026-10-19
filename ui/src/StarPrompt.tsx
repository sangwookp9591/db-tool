import { useEffect, useRef, useState } from 'preact/hooks';
import { lazy, Suspense } from 'preact/compat';
import { api, isMock } from './api';
const StarPromptCard = lazy(() => import('./StarPromptCard').then(m => ({ default: m.StarPromptCard })));
export const GITHUB_URL = 'https://github.com/sangwookp9591/db-tool';
export async function openStar(){if(isMock)return;await (await import('@tauri-apps/plugin-opener')).openUrl(GITHUB_URL);await api('star_prompt',{action:'star'});}
declare module './api' { interface Commands { star_count:{args:Record<string,never>;result:number|null}; star_prompt: {args:{action:'status'|'shown'|'later'|'dismiss'|'star'};result:{eligible:boolean}} } }
export function StarPrompt({blocked, pulse}:{blocked:boolean;pulse:number}) {
  const [visible,setVisible]=useState(false);
  const busy=useRef(false);
  useEffect(()=>{
    if(isMock || blocked || visible || busy.current)return;
    let alive=true;
    const timer=setTimeout(()=>{busy.current=true;void api('star_prompt',{action:'status'}).then(async result=>{
      if(!alive || !result.eligible)return;
      const shown=await api('star_prompt',{action:'shown'});if(alive&&shown.eligible)setVisible(true);
    }).catch(()=>{}).finally(()=>{busy.current=false;});},60_000);
    return()=>{alive=false;clearTimeout(timer);};
  },[blocked,pulse,visible]);
  useEffect(()=>{if(!isMock)return; const preview=()=>setVisible(true);window.addEventListener('ax:preview-star',preview);return()=>window.removeEventListener('ax:preview-star',preview);},[]);
  return <Suspense fallback={null}>{visible && !blocked && <StarPromptCard onClose={()=>setVisible(false)}/>}</Suspense>;
}

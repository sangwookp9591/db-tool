import { openUrl } from '@tauri-apps/plugin-opener';
import { useEffect, useRef, useState } from 'preact/hooks';
import { api, isMock } from './api';
import { Button } from './ui/Button';
import './w7.css';
import {PulseHeart} from './ui/PulseHeart';
export const GITHUB_URL = 'https://github.com/sangwookp9591/db-tool';
export async function openStar(){if(isMock)return;await openUrl(GITHUB_URL);await api('star_prompt',{action:'star'});}
declare module './api' { interface Commands { star_count:{args:Record<string,never>;result:number|null}; star_prompt: {args:{action:'status'|'shown'|'later'|'dismiss'|'star'};result:{eligible:boolean}} } }
export function StarPrompt({blocked, pulse}:{blocked:boolean;pulse:number}) {
  const [visible,setVisible]=useState(false),[error,setError]=useState(''),[clicked,setClicked]=useState(false),[count,setCount]=useState<number|null>(null);
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
  useEffect(()=>{if(!visible||isMock)return;let alive=true;void api('star_count',{}).then(count=>{if(alive)setCount(count);}).catch(()=>{if(alive)setCount(null);});return()=>{alive=false;};},[visible]);
  async function choose(action:'star'|'later'|'dismiss') {
    try {if(action==='star'){await openStar();setClicked(true);}else{if(!isMock)await api('star_prompt',{action});setVisible(false);}}catch(e){setError(String(e));}
  }
  return visible && !blocked ? <aside class="star-prompt" aria-label="GitHub Star"><p>{clicked?'고마워요 ⭐':'AxDB 잘 쓰고 계신가요? GitHub에 ⭐ 를 눌러 주시면 큰 힘이 됩니다'}</p><div class="toolbar"><PulseHeart icon="star" liked={clicked} count={count??0} showCount={count!==null} label="⭐ Star 하러 가기" onClick={()=>choose('star')}/><Button morph={false} onClick={()=>void choose('later')}>나중에</Button><Button morph={false} onClick={()=>void choose('dismiss')}>다시 보지 않기</Button></div>{error&&<p role="alert">{error}</p>}</aside>:null;
}

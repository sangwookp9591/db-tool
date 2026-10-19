import { useEffect, useState } from 'preact/hooks';
import { api, isMock } from './api';
import { openStar } from './StarPrompt';
import { Button } from './ui/Button';
import { PulseHeart } from './ui/PulseHeart';
import './w7.css';
export function StarPromptCard({onClose}: {onClose: () => void}) {
  const [error,setError]=useState(''),[clicked,setClicked]=useState(false),[count,setCount]=useState<number|null>(null);
  useEffect(()=>{if(isMock)return;let alive=true;void api('star_count',{}).then(count=>{if(alive)setCount(count);}).catch(()=>{if(alive)setCount(null);});return()=>{alive=false;};},[]);
  async function choose(action:'star'|'later'|'dismiss') {
    try {if(action==='star'){await openStar();setClicked(true);}else{if(!isMock)await api('star_prompt',{action});onClose();}}catch(e){setError(String(e));}
  }
  return <aside class="star-prompt" aria-label="GitHub Star"><p>{clicked?'고마워요 ⭐':'AxDB 잘 쓰고 계신가요? GitHub에 ⭐ 를 눌러 주시면 큰 힘이 됩니다'}</p><div class="toolbar"><PulseHeart icon="star" liked={clicked} count={count??0} showCount={count!==null} label="⭐ Star 하러 가기" onClick={()=>choose('star')}/><Button morph={false} onClick={()=>void choose('later')}>나중에</Button><Button morph={false} onClick={()=>void choose('dismiss')}>다시 보지 않기</Button></div>{error&&<p role="alert">{error}</p>}</aside>;
}

// Preact port of React Bits PulseHeart (MIT + Commons Clause), https://reactbits.dev/micro/pulse-heart.
import {useEffect,useLayoutEffect,useRef,useState} from 'preact/hooks';
import {pulseSwell} from './pulseMath';
import './PulseHeart.css';
interface Props {icon?:'star';liked?:boolean;count?:number;showCount?:boolean;size?:number;likedColor?:string;pillColor?:string;textColor?:string;label:string;disabled?:boolean;onClick:()=>void|Promise<void>}
export function PulseHeart({icon='star',liked=false,count=0,showCount=true,size=22,likedColor='#f5b301',pillColor='var(--surface-secondary)',textColor='var(--text-primary)',label,disabled,onClick}:Props){
 const root=useRef<HTMLButtonElement>(null),pill=useRef<HTMLSpanElement>(null),glyph=useRef<SVGGElement>(null),roller=useRef<HTMLSpanElement>(null),frame=useRef(0),timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
 const [shown,setShown]=useState(liked),[rolling,setRolling]=useState(false),[busy,setBusy]=useState(false);
 const latest=useRef(liked);latest.current=liked;
 useEffect(()=>{if(!frame.current)setShown(liked);},[liked]);
 useEffect(()=>()=>{cancelAnimationFrame(frame.current);clearTimeout(timer.current);},[]);
 useLayoutEffect(()=>{if(!rolling||!roller.current)return;const el=roller.current;el.style.transition='none';el.style.transform='translateY(0)';el.getBoundingClientRect();el.style.transition='';el.style.transform='translateY(-1em)';},[rolling]);
 async function click(){if(disabled||busy||frame.current)return;setBusy(true);
  const reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
  if(!reduced)await new Promise<void>(resolve=>{const start=performance.now();let swapped=false;
   if(root.current)root.current.dataset.running='';
   const tick=(now:number)=>{const t=Math.min(1,(now-start)/560),s=pulseSwell(t),k=1-.7*s;
    glyph.current?.setAttribute('transform',`translate(12 12) scale(${k}) translate(-12 -12)`);
    if(pill.current)pill.current.style.transform=`scale(${1-.03*s})`;
    if(t>=.4&&!swapped){swapped=true;setShown(true);setRolling(true);clearTimeout(timer.current);timer.current=setTimeout(()=>setRolling(false),350);}
    if(t<1){frame.current=requestAnimationFrame(tick);return;}
    frame.current=0;glyph.current?.removeAttribute('transform');if(pill.current)pill.current.style.transform='';if(root.current)delete root.current.dataset.running;resolve();
   };frame.current=requestAnimationFrame(tick);
  });else setShown(true);
  try{await onClick();}finally{setBusy(false);setShown(latest.current);}
 }
 const formatted=new Intl.NumberFormat().format(count);
 return <button ref={root} type="button" class="pulse-heart" aria-label={label} aria-pressed={liked} disabled={disabled||busy} data-liked={String(shown)} data-no-count={!showCount?'':undefined} data-icon={icon} style={{'--ph-size':`${size}px`,'--ph-liked':likedColor,'--ph-pill':pillColor,'--ph-text':textColor}} onClick={()=>void click()}><span ref={pill} class="pulse-heart__pill"><span class="pulse-heart__heart" aria-hidden="true"><svg viewBox="0 0 24 24"><g ref={glyph}><path d="m12 2.5 2.94 5.96 6.58.96-4.76 4.64 1.12 6.55L12 17.52l-5.88 3.09 1.12-6.55L2.48 9.42l6.58-.96Z" vectorEffect="non-scaling-stroke"/></g></svg></span>{showCount&&<span class="pulse-heart__count" aria-label={`${formatted} GitHub stars`}>{rolling?<span class="pulse-heart__slot"><span ref={roller} class="pulse-heart__roll"><span>{formatted}</span><span>{formatted}</span></span></span>:formatted}</span>}<span class="pulse-heart__label">{label}</span></span></button>;
}

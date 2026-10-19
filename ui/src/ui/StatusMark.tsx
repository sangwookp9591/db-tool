// Preact port of React Bits StatusMark (MIT + Commons Clause), https://reactbits.dev/micro/status-mark.
import type {ComponentChildren} from 'preact';
import {useLayoutEffect,useRef} from 'preact/hooks';
import {markGeometry,type MarkStatus} from './statusMath';
import './StatusMark.css';
interface Props{status?:MarkStatus;progress?:number;label?:ComponentChildren;color?:string;doneColor?:string;errorColor?:string;size?:number;strokeWidth?:number;dashes?:number;fontSize?:number;spinDuration?:number;arcLength?:number;drawDuration?:number;strike?:boolean;className?:string}
export function StatusMark({status='pending',progress,label,color='currentColor',doneColor='var(--accent-success)',errorColor='var(--accent-danger,var(--accent-error))',size=20,strokeWidth=2,dashes=8,fontSize=14,spinDuration=1100,arcLength=.68,drawDuration=240,strike=true,className=''}:Props){
 const ring=useRef<SVGCircleElement>(null),frame=useRef(0),values=useRef({mode:status==='pending'||status==='cancelled'?0:1,arc:1});
 const circumference=2*Math.PI*(10-strokeWidth/2),determinate=status==='running'&&Number.isFinite(progress),indeterminate=status==='running'&&!determinate;
 const targetArc=indeterminate?arcLength:determinate?Math.max(0,Math.min(1,progress!)):1,targetMode=['running','done','failed'].includes(status)?1:0;
 useLayoutEffect(()=>{
  const from={...values.current},start=performance.now(),reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
  const write=(mode:number,arc:number)=>{values.current={mode,arc};const {dash,gap}=markGeometry(mode,arc,circumference,dashes);ring.current?.setAttribute('stroke-dasharray',`${dash} ${gap}`);};
  if(reduced){write(targetMode,targetArc);return;}
  const tick=(now:number)=>{const t=Math.min(1,(now-start)/300),eased=t*t*(3-2*t);write(from.mode+(targetMode-from.mode)*eased,from.arc+(targetArc-from.arc)*eased);if(t<1)frame.current=requestAnimationFrame(tick);else frame.current=0;};
  frame.current=requestAnimationFrame(tick);return()=>cancelAnimationFrame(frame.current);
 },[status,targetArc,targetMode,circumference,dashes]);
 const spoken={pending:'대기',running:'진행 중',done:'완료',failed:'실패',cancelled:'취소'}[status];
 return <span class={`status-mark ${className}`} data-status={status} data-indeterminate={indeterminate?'':undefined} data-strike={strike?'':undefined} style={{'--sm-size':`${size}px`,'--sm-stroke':strokeWidth,'--sm-color':color,'--sm-done':doneColor,'--sm-error':errorColor,'--sm-font':`${fontSize}px`,'--sm-draw':`${drawDuration}ms`,'--sm-spin':`${spinDuration}ms`,'--sm-travel':`${-circumference}`}}><svg class="status-mark__glyph" viewBox="0 0 24 24" width={size} height={size} role={label==null?'img':undefined} aria-label={label==null?spoken:undefined} aria-hidden={label!=null||undefined}><circle class="status-mark__track" cx="12" cy="12" r={10-strokeWidth/2} transform="rotate(-90 12 12)"/><circle ref={ring} class="status-mark__ring" cx="12" cy="12" r={10-strokeWidth/2} transform="rotate(-90 12 12)"/><path class="status-mark__check" d="M7.5 12.25 10.5 15.25 16.75 8.75" pathLength="1"/><path class="status-mark__cross" d="M8.5 8.5 15.5 15.5M15.5 8.5 8.5 15.5" pathLength="1"/></svg>{label!=null&&<><span class="status-mark__sr">{spoken}: </span><span class="status-mark__label">{label}<span class="status-mark__strike" aria-hidden="true"/></span></>}</span>;
}

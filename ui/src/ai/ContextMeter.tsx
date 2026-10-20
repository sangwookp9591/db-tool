// Kokonut UI (MIT) 동작·디자인 참고. 모델 한도가 아닌 앱의 대화 전송 예산을 표시한다.
import { Button } from '../ui/Button';
import { conversationContext, type ChatMessage } from './chat';
export function ContextMeter({messages,query,schema,busy,usage,onCompact}:{messages:ChatMessage[];query:string;schema:string;busy:boolean;usage?:{input_tokens:number;output_tokens:number};onCompact:()=>void}) {
 const bytes=new TextEncoder().encode(conversationContext(messages).map(m=>m.content).join('')).length;
 const percent=Math.min(100,Math.round(bytes/32000*100));
 return <details class="ai-context-meter"><summary aria-label={`대화 맥락 사용량 ${percent}%`} title="전송 맥락 보기"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="var(--border-default)" stroke-width="2"/><circle cx="12" cy="12" r="9" fill="none" stroke="var(--accent-primary)" stroke-width="2" stroke-dasharray={`${percent*.5655} 56.55`} transform="rotate(-90 12 12)"/></svg></summary><div><strong>전송 맥락</strong><p>시스템 · 호스트의 제안 전용 정책</p><p>스키마 · 약 {Math.ceil(schema.length/3).toLocaleString()} 토큰 (추정)</p><p>첨부 쿼리 · 약 {Math.ceil(query.length/3).toLocaleString()} 토큰 (추정)</p><p>메시지 · {bytes.toLocaleString()} / 32,000 bytes</p><p>남은 대화 예산 · {(32000-bytes).toLocaleString()} bytes</p>{usage&&<p>공급자 사용량 · 입력 {usage.input_tokens} / 출력 {usage.output_tokens} 토큰</p>}<Button disabled={busy||messages.length<4} onClick={onCompact}>오래된 대화 발췌 요약</Button><small>민감·실패 응답 제외 · 최근 20개만 전송</small></div></details>;
}

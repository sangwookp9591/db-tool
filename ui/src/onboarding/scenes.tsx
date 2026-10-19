import { useEffect, useRef, useState } from 'preact/hooks';
import { ArrowRight, Bot, Database, FileJson, FileText, LockKeyhole, Network, Play, ShieldCheck, Sparkles } from 'lucide-preact';
import { IntegrationBeam, type IntegrationBeamHandle } from '../ui/IntegrationBeam';
import { JellyRadio } from '../ui/JellyRadio';
import { DriverPicker } from '../drivers/DriverPicker';
import mark from '../assets/axdb-mark.png';

export const scenes = [
  { label: '환영', title: '데이터를 이해하는\n새로운 작업공간.', description: '서로 다른 DB를 한 곳에서 탐색하고, AI와 안전하게 함께 일하세요.', hint: '연결부터 내보내기까지, 7개의 짧은 장면으로 시작합니다.', action: '지금 해 보기' },
  { label: 'DB 드라이버', title: '필요한 DB만,\n골라서 설치.', description: '앱은 드라이버 없이 시작합니다. PostgreSQL · Neo4j · MongoDB · Redis · SQLite 중 필요한 드라이버를 설치하세요.', hint: '오른쪽에서 설치한 뒤 연결을 추가하세요. 새로운 DB 드라이버 제작은 docs/PLUGIN_GUIDE.md를 참고하세요.', action: '연결 추가' },
  { label: '연결과 탐색', title: '연결하고, 펼치고,\n바로 물어보세요.', description: '연결을 추가하고 탐색기에서 테이블을 여세요. 쿼리 결과는 아래 결과 창에 나타납니다.', hint: '저장을 선택한 비밀번호는 OS 키체인에만 보관합니다. ⌘ Enter로 쿼리를 실행하세요.', action: '샘플 쿼리 열기' },
  { label: 'ERD와 뷰어', title: '데이터의 관계가\n한눈에 보입니다.', description: '탐색기의 다이어그램에서 관계를 확인하세요. 결과에 맞춰 그래프 · 문서 트리 · 표로 전환할 수 있습니다.', hint: 'ERD와 미리보기 지원 범위는 드라이버마다 다릅니다. 오른쪽에서 뷰어를 바꿔 보세요.', action: '탐색기 열기' },
  { label: '권한과 안전', title: '변경하기 전에,\n확인할 수 있도록.', description: '연결별 읽기 전용과 에이전트별 권한으로 접근 범위를 정하세요. 지원 DB의 변경 미리보기는 실행 후 롤백합니다.', hint: 'write-with-approval도 매번 앱 안에서 승인해야 합니다. 미리보기는 변경을 저장하지 않습니다.', action: '에이전트 클라이언트 열기' },
  { label: 'MCP 연결', title: '당신의 AI에게\n안전한 연결을.', description: 'MCP 브리지를 켜고 클라이언트 설정을 복사하세요. Claude 등 HTTP MCP 클라이언트에 붙여 넣으면 됩니다.', hint: '복사한 JSON에는 토큰이 포함됩니다. 브리지 재시작 시 URL·토큰을 다시 복사하고, 에이전트마다 범위 토큰을 발급하세요.', action: '권한 범위 토큰 발급' },
  { label: '파일과 AI', title: '발견한 내용을\n다음 작업으로.', description: '결과는 CSV · JSON · Markdown, ERD는 SVG · PNG로 저장하세요. AI 도우미는 쿼리를 새 탭에 제안합니다.', hint: 'AI 제안은 자동 실행되지 않습니다. 결과 툴바의 내보내기와 사이드 레일의 AI 도우미에서 이어 가세요.', action: 'AI 도우미 열기' },
] as const;

function MiniTable() {
  return <div class="ob-mini-table"><div><b>id</b><b>name</b><b>team</b></div>{[['01','서연','Product'],['02','민준','Data'],['03','지우','Design']].map(row => <div key={row[0]}>{row.map(value => <span key={value}>{value}</span>)}</div>)}</div>;
}
function Welcome() {
  return <div class="ob-orbit" aria-label="AxDB 로고와 데이터 궤도"><span class="ob-ring ob-ring-one"/><span class="ob-ring ob-ring-two"/><span class="ob-orbit-dot ob-dot-one"/><span class="ob-orbit-dot ob-dot-two"/><div class="ob-logo ob-enter"><span class="ob-logo-mark" role="img" aria-label="AxDB" style={{maskImage:`url(${mark})`,WebkitMaskImage:`url(${mark})`}}/><strong>AxDB</strong><small>모든 데이터, 하나의 흐름</small></div><span class="ob-orbit-tag ob-tag-top">DATABASES</span><span class="ob-orbit-tag ob-tag-bottom">HUMAN + AI</span></div>;
}
function Explore() {
  return <div class="ob-flow"><div class="ob-flow-label ob-enter"><Database size={18}/><b>나의 데이터베이스</b><span class="ob-connected">연결됨</span></div><div class="ob-explorer ob-enter" style={{'--delay':'120ms'}}><span>⌄ public</span><span>　⌄ 테이블</span><strong>　　▦ people</strong></div><div class="ob-query ob-enter" style={{'--delay':'240ms'}}><code><span>SELECT</span> * <span>FROM</span> people;</code><Play size={14}/></div><div class="ob-enter" style={{'--delay':'380ms'}}><MiniTable/></div><span class="ob-caption"><LockKeyhole size={14}/> 비밀번호는 OS 키체인으로</span></div>;
}
function Erd() {
  return <div class="ob-erd"><svg viewBox="0 0 400 200" aria-hidden="true"><path class="ob-fk" d="M150 65 H210 V140 H250"/><circle cx="150" cy="65" r="4"/><circle cx="250" cy="140" r="4"/></svg><div class="ob-entity ob-enter"><strong>teams</strong><span>◇ id <small>PK</small></span><span>name <small>text</small></span></div><div class="ob-entity ob-enter" style={{'--delay':'160ms'}}><strong>people</strong><span>◇ id <small>PK</small></span><span>team_id <small>FK</small></span><span>name <small>text</small></span></div><span class="ob-erd-label">1 ─── N</span></div>;
}
function Viewers() {
  const [view, setView] = useState('ERD');
  return <div class="ob-viewers"><JellyRadio aria-label="온보딩 뷰어" items={['ERD','그래프','문서','표']} value={view} onChange={setView}/><div class="ob-view-content ob-enter" key={view}>{view === 'ERD' ? <Erd/> : view === '표' ? <MiniTable/> : view === '문서' ? <div class="ob-document"><FileJson/><code>⌄ people<br/>　⌄ 0<br/>　　id: <em>1</em><br/>　　name: <em>"서연"</em><br/>　　team: <em>"Product"</em></code></div> : <div class="ob-graph"><svg viewBox="0 0 400 210" aria-label="사람과 팀의 관계"><path d="M90 100 L220 50 L310 150 M90 100 L310 150"/><circle cx="90" cy="100" r="28"/><circle cx="220" cy="50" r="22"/><circle cx="310" cy="150" r="32"/><text x="90" y="104">사람</text><text x="220" y="54">팀</text><text x="310" y="154">프로젝트</text></svg></div>}</div><p class="ob-caption">데이터의 모양에 맞는 뷰어</p></div>;
}
function Safety() {
  return <div class="ob-safety"><div class="ob-shield ob-enter"><ShieldCheck size={64}/><LockKeyhole size={20}/></div><div class="ob-permissions">{[['none','접근 없음'],['read','읽기 전용'],['preview','실행 후 롤백'],['write-with-approval','매번 승인 후 적용']].map(([level,label],i) => <div class="ob-enter" style={{'--delay':`${i*100}ms`}} key={level}><code>{level}</code><span>{label}</span></div>)}</div><div class="ob-safe-flow"><span>편집</span><ArrowRight size={14}/><span>미리보기</span><ArrowRight size={14}/><span><ShieldCheck size={14}/> 승인·적용</span></div></div>;
}
function Mcp({ onClients }: { onClients: () => void }) {
  const beam = useRef<IntegrationBeamHandle>(null);
  useEffect(() => {
    const first = setTimeout(() => beam.current?.pulse('database','axdb','success'), 350);
    const second = setTimeout(() => beam.current?.pulse('axdb','agent'), 1000);
    return () => { clearTimeout(first); clearTimeout(second); };
  }, []);
  return <div class="ob-mcp"><IntegrationBeam controllerRef={beam} nodes={[{id:'database',label:'데이터베이스',side:'source',state:'connected',icon:<Database/>},{id:'agent',label:'Claude · AI 에이전트',side:'target',state:'connected',icon:<Bot/>}]} status="MCP 연결 흐름 예시" onSelect={onClients} aria-label="MCP 연결 흐름 예시"/><div class="ob-masked ob-enter"><LockKeyhole size={16}/><code>Authorization: Bearer ••••••••</code></div><p class="ob-caption">127.0.0.1 · 이 기기의 클라이언트만</p></div>;
}
function Export() {
  return <div class="ob-export"><div class="ob-enter"><MiniTable/></div><div class="ob-file-row">{['CSV','JSON','Markdown'].map((format,i)=><div class="ob-file ob-enter" style={{'--delay':`${180+i*100}ms`}} key={format}><FileText/><b>{format}</b></div>)}</div><div class="ob-export-note"><Network size={16}/> ERD <ArrowRight size={14}/> SVG · PNG</div><div class="ob-ai-card ob-enter" style={{'--delay':'500ms'}}><Sparkles size={20}/><div><strong>이 데이터를 더 깊이 이해해 볼까요?</strong><small>AI 제안 → 새 쿼리 탭 → 직접 확인·실행</small></div></div></div>;
}
export function Scene({ step, onClients, onDriverBusy }: { step:number; onClients:()=>void; onDriverBusy:(busy:boolean)=>void }) {
  return <div class="ob-illustration" data-scene={step}><span class="ob-demo-label">{step === 0 ? 'AXDB / GET STARTED' : step === 1 ? 'DB 드라이버 설치' : '사용 흐름 예시'}</span>{[<Welcome/>,<DriverPicker compact onBusyChange={onDriverBusy}/>,<Explore/>,<Viewers/>,<Safety/>,<Mcp onClients={onClients}/>,<Export/>][step]}</div>;
}

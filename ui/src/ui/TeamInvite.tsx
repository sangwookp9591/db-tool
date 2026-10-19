// 사용자 제공 UI 예제 기반: TeamInvite (600 × 400 composition), adapted for AxDB.
import { Bot, MousePointer2, PlugZap } from 'lucide-preact';
import { useIllustration } from './useIllustration';
import './TeamInvite.css';
export function TeamInvite({ connectedName, clientName = '새 에이전트', joined = false }: { connectedName?: string; clientName?: string; joined?: boolean }) {
  const { root, playing } = useIllustration(2800);
  return <div ref={root} className="ui-ti" data-playing={playing} role="img" aria-label={joined ? `${clientName} 클라이언트가 생성되었습니다. 설정을 복사해 연결하세요.` : `MCP 연결 예시: AxDB와 ${connectedName || 'MCP 브리지'}에 새 에이전트를 연결합니다.`}>
    <svg className="ui-ti-lines" viewBox="0 0 600 340" aria-hidden="true"><path className="ui-ti-line" d="M162 159C214 159 222 105 258 105" /><path className="ui-ti-line ui-ti-line--dashed" d="M162 159C214 165 214 245 248 245" /><path className="ui-ti-line ui-ti-line--joined" d="M162 159C214 165 214 245 248 245" /><g className="ui-ti-slot"><circle cx="290" cy="245" r="40" /><path d="M290 234v22M279 245h22" /></g></svg>
    <div className="ui-ti-node ui-ti-node--a" aria-hidden="true"><span className="ui-ti-face"><span className="axdb-mark" /></span><span className="ui-ti-dot" /><small>AxDB</small></div>
    <div className="ui-ti-node ui-ti-node--b" aria-hidden="true"><span className="ui-ti-face">{connectedName ? <Bot /> : <PlugZap />}</span><span className="ui-ti-dot" /><small>{connectedName || 'MCP 브리지'}</small></div>
    <div className="ui-ti-drag" aria-hidden="true"><div className="ui-ti-node ui-ti-node--c"><span className="ui-ti-face"><Bot /></span><span className="ui-ti-dot" /><small>{clientName}</small></div><MousePointer2 className="ui-ti-cursor" /></div>
  </div>;
}

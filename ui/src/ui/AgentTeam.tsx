// 사용자 제공 UI 예제 기반: AgentTeam, with AxDB permission/tool marks and one finite entrance.
import { ArrowUp, Bot, Search, ShieldCheck, Terminal } from 'lucide-preact';
import { useIllustration } from './useIllustration';
import './AgentTeam.css';
const roles = [{ label: 'read', icon: <Search />, tool: 'explore · schema' }, { label: 'preview', icon: <Terminal />, tool: 'preview_changes' }, { label: 'write-with-approval', icon: <ShieldCheck />, tool: 'run_query · 앱 승인' }];
export function AgentTeam() {
  const { root, playing } = useIllustration(3400);
  return <div ref={root} className="ui-at" data-playing={playing} role="img" aria-label="권한 등급 안내 예시: read는 읽기, preview는 미리보기, write-with-approval은 앱 승인 후 쓰기입니다. 아래에서 실제 권한을 선택하세요.">
    <svg className="ui-at-lines" viewBox="0 0 600 300" aria-hidden="true">{['M300 80C300 132 102 122 102 166', 'M300 80V166', 'M300 80C300 132 498 122 498 166'].map(d => <path key={d} d={d} />)}</svg>
    <div className="ui-at-prompt" aria-hidden="true"><span className="ui-at-text">분석 에이전트에게 public 스키마 읽기 권한</span><span className="ui-at-send"><ArrowUp /></span></div>
    {roles.map((role, i) => <div className="ui-at-agent" style={{ left: `${17 + i * 33}%`, '--i': i }} key={role.label} aria-hidden="true"><div className="ui-at-node"><Bot /><span className="ui-at-dot" /></div><div className="ui-at-role">{role.icon}<span>{role.label}</span></div><small>{role.tool}</small></div>)}
  </div>;
}

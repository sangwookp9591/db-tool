import { useState } from 'preact/hooks';
import { Button } from '../ui/Button';
import './client-connect-guide.css';

export type AgentClientKind = 'codex' | 'claude' | 'json';
export function shellQuote(value: string) { return `'${value.replace(/'/g, `'\\''`)}'`; }
export function clientCommand(kind: AgentClientKind, url: string) {
  const endpoint = shellQuote(url || '<AXDB_MCP_URL>');
  if (kind === 'codex') return `# 발급된 토큰을 붙여 넣고 Enter를 누르세요 (입력은 숨겨집니다).
read -s AXDB_MCP_TOKEN
export AXDB_MCP_TOKEN
codex mcp add axdb --url ${endpoint} --bearer-token-env-var AXDB_MCP_TOKEN
# 같은 터미널에서 Codex를 시작하세요.`;
  if (kind === 'claude') return `# 발급된 토큰을 붙여 넣고 Enter를 누르세요 (입력은 숨겨집니다).
read -s AXDB_MCP_TOKEN
claude mcp add --transport http axdb ${endpoint} --header "Authorization: Bearer \${AXDB_MCP_TOKEN}"
unset AXDB_MCP_TOKEN`;
  return JSON.stringify({ mcpServers: { axdb: { type: 'http', url: url || '<AXDB_MCP_URL>', headers: { Authorization: 'Bearer <발급된 토큰>' } } } }, null, 2);
}

export function clientConfig(kind: AgentClientKind, url: string, token?: string) {
  if (kind === 'codex') return `[mcp_servers.axdb]\nurl = ${JSON.stringify(url || '<AXDB_MCP_URL>')}\nbearer_token_env_var = "AXDB_MCP_TOKEN"`;
  return JSON.stringify({ mcpServers: { axdb: { type: 'http', url: url || '<AXDB_MCP_URL>', headers: { Authorization: `Bearer ${token || '<발급된 토큰>'}` } } } }, null, 2);
}

const kinds: { id: AgentClientKind; label: string }[] = [
  { id: 'codex', label: 'Codex' },
  { id: 'claude', label: 'Claude Code' },
  { id: 'json', label: '기타 MCP 앱' },
];

export function ClientConnectGuide({ client, url, token, onCopy }: { client: string; url: string; token?: string; onCopy: (value: string, message: string) => void }) {
  const [kind, setKind] = useState<AgentClientKind>('codex');
  const instructions = clientCommand(kind, url);
  return <section class="client-connect-guide" aria-label={`${client} 연결 방법`}>
    <div class="client-connect-guide__heading"><div><h3>{client} 연결 방법</h3><p>브리지를 켠 뒤, 아래 설정을 해당 에이전트에 등록하세요.</p></div><span class="client-connect-guide__status" data-ready={!!url}>{url ? '브리지 주소 확인됨' : '브리지를 켜면 주소가 표시됩니다'}</span></div>
    <div class="client-connect-guide__tabs" role="group" aria-label="에이전트 종류">
      {kinds.map(item => <Button key={item.id} bare aria-pressed={kind === item.id} onClick={() => setKind(item.id)}>{item.label}</Button>)}
    </div>
    <ol class="client-connect-guide__steps"><li>MCP 브리지를 켜고 클라이언트의 DB 접근 권한을 저장하세요.</li><li>{kind==='json'?'발급된 토큰을 복사해 JSON의 <발급된 토큰> 위치에 넣으세요.':'발급된 토큰을 복사하세요. 명령 실행 후 토큰을 붙여 넣고 Enter를 누르면 됩니다.'}</li><li>설정을 등록한 뒤 에이전트를 시작하고 “DB 연결 목록을 보여 줘”라고 요청하세요.</li></ol>
    <pre class="client-connect-guide__code"><code>{instructions}</code></pre>
    <p class="client-connect-guide__note">{kind === 'codex' ? 'Codex는 같은 터미널에서 시작하세요. 설정 복사는 ~/.codex/config.toml에 붙여 넣을 TOML을 제공합니다.' : kind === 'claude' ? 'Claude Code는 토큰을 숨겨 입력해 등록합니다. 설정 복사는 MCP JSON을 제공합니다.' : '사용하는 앱의 MCP 설정에 JSON을 붙여 넣고 발급된 토큰을 입력하세요.'} 연결 방법 복사에는 비밀 토큰이 포함되지 않습니다.</p>
    {!token && <p class="client-connect-guide__note">기존 토큰을 보관했다면 그대로 사용하세요. 분실했다면 클라이언트 카드에서 토큰을 회전한 뒤 에이전트 설정도 갱신하세요.</p>}
    <p class="client-connect-guide__note">AxDB를 재시작하거나 브리지를 다시 켜면 연결 주소가 바뀝니다. 연결되지 않으면 여기에서 새 주소로 설정을 다시 복사하세요.</p>
    <div class="client-connect-guide__actions">
      <Button morph={false} variant="brand" onClick={() => onCopy(instructions, '연결 방법을 복사했습니다.')}>연결 방법 복사</Button>
      <Button morph={false} disabled={!url} onClick={() => onCopy(clientConfig(kind, url, token), kind === 'codex' ? 'Codex TOML 설정을 복사했습니다.' : 'MCP JSON 설정을 복사했습니다.')}>설정 복사</Button>
    </div>
  </section>;
}

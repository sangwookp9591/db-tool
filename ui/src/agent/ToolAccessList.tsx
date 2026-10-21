import { SpringCheck } from '../ui/SpringCheck';
import './tool-access.css';

export const AGENT_TOOLS = ['list_drivers', 'list_connections', 'open_connection', 'close_connection', 'explore', 'run_query', 'get_result_page', 'preview_changes', 'explain_query', 'lint_query', 'browse', 'expand_node', 'get_history', 'get_perf_metrics', 'get_ui_state', 'open_query_tab', 'get_schema_context', 'get_work_context', 'explain_error'];

const toolLabels: Record<string, string> = {
  list_drivers: '드라이버 목록',
  list_connections: '연결 목록',
  open_connection: '연결 열기',
  close_connection: '연결 닫기',
  explore: '스키마 탐색',
  run_query: '쿼리 실행',
  get_result_page: '결과 이어보기',
  preview_changes: '변경 미리보기',
  explain_query: '실행 계획 보기',
  lint_query: '쿼리 진단',
  browse: '표 데이터 보기',
  expand_node: '그래프 확장',
  get_history: '실행 이력 보기',
  get_perf_metrics: '성능 지표 보기',
  get_ui_state: '작업공간 상태 읽기',
  open_query_tab: '쿼리 탭 열기',
  get_schema_context: '스키마 요약 읽기',
  get_work_context: '작업 맥락 읽기',
  explain_error: '오류 원인 분석',
};
export const toolLabel = (tool: string) => toolLabels[tool] ?? tool;

export function ToolAccessList({ allowed, onChange }: { allowed: string[]; onChange: (tools: string[]) => void }) {
  return <fieldset class="agent-tools" aria-label="허용 도구">
    <legend>허용 도구</legend>
    {AGENT_TOOLS.map(tool => {
      const title = toolLabel(tool);
      const checked = allowed.includes(tool);
      return <div class="agent-tools__option" key={tool}>
        <SpringCheck
          aria-label={`${title} (${tool})`}
          checked={checked}
          onChange={enabled => onChange(enabled ? [...allowed, tool] : allowed.filter(item => item !== tool))}
          label={<span class="agent-tools__label"><span>{title}</span><code>{tool}</code></span>}
        />
      </div>;
    })}
  </fieldset>;
}

import type { Table } from '../contract';
export interface Scope { focus?: string; text?: string; run?: string; include_rows?: boolean; selected_rows?: Table; masked_columns?: string[] }
export interface WorkInput { session_id: string; scope: Scope }
export interface AiSettings { enabled: boolean; model: string; effort: string; auth: 'api_key' | 'claude_login' }
export interface AiReply { context?: Record<string, unknown>; context_elapsed_ms?: number; text: string; refused: boolean; stop_reason: string }
export function suggestedQuery(text: string): string | null {
  return /```[^\n]*\n([\s\S]*?)```/.exec(text)?.[1]?.trim() || null;
}
export function selectRows(table: Table | undefined, selection: string): Table | undefined {
  if (!table) return undefined;
  const indices = [...new Set(selection.split(',').map(s => s.trim()).filter(Boolean).map(Number))];
  if (indices.length > 20 || indices.some(i => !Number.isInteger(i) || i < 1 || i > table.rows.length)) throw new Error('결과 행 번호를 최대 20개 입력하세요. 예: 1,3,5');
  return { columns: table.columns, rows: indices.map(i => table.rows[i - 1]) };
}

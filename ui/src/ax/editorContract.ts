export interface PlanTree {
  op: string;
  target: string | null;
  est_rows: number | null;
  actual_rows: number | null;
  cost: number | null;
  children: PlanTree[];
  warnings: string[];
}
/** UTF-16 offsets, matching CodeMirror and JavaScript string indices. */
export interface TextEdit { start: number; end: number; text: string }
export interface Diagnostic {
  rule_id: string; start: number; end: number;
  severity: 'error' | 'warning'; message: string;
  quick_fix: { title: string; edits: TextEdit[] } | null;
}
export type RefactorAction = { kind: 'format' | 'expand_star' } | { kind: 'rename_alias'; from: string; to: string } | { kind: 'extract_cte'; name: string };
export interface EditorCommands {
  explain_query: { args: { sessionId: string; runId: string; query: string; analyze: boolean }; result: PlanTree };
  lint_query: { args: { sessionId: string; query: string }; result: Diagnostic[] };
  refactor_query: { args: { sessionId: string; query: string; action: RefactorAction }; result: TextEdit[] };
}

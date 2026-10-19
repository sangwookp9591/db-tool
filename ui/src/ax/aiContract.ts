import type { Risk } from './PlanCard';
export type Provider = 'anthropic' | 'openai';
export type OperationKind='explain'|'analyze'|'refactor'|'import'|'schema_diff'|'migrate'|'backup'|'restore'|'grant'|'connect_test';
export interface Operation { id: string; kind: OperationKind; connection: string; params: Record<string, unknown>; plan: { reason: string; evidence: string[]; statements: string[] }; risk: Risk; needs_approval: boolean }
export interface Usage { input_tokens: number; output_tokens: number }
declare module '../api' { interface Commands {
  ai_provider_login_start: { args: { provider: Provider; accountId?: string | null }; result: { url: string | null; waiting_for_code?: boolean } };
  ai_provider_login_poll: { args: { provider: Provider }; result: { state: 'idle' | 'waiting' | 'done' | 'failed'; account?: string } };
  ai_provider_login_cancel: { args: { provider: Provider }; result: void };
  ai_provider_logout: { args: { provider: Provider; accountId?:string }; result: {logged_in: boolean; remote_revoked?: boolean} };
  ai_account_select: { args: { accountId: string }; result: void };
  ai_diagnose_connection: { args: { steps: {stage: string; status: string; elapsed_ms: number; error_code: string | null}[] }; result: { text: string } };
  ai_models: { args: {provider?:Provider}; result: { id: string; name: string; efforts: string[]; auth?: "api_key"|"claude_login"|"chatgpt_login"; status: "connected"|"expired"|"disconnected"; cached: boolean }[] };
  ai_operation: { args: { sessionId: string; intent: string; purpose: 'operation' | 'explain' | 'recommend_indexes'; details?: Record<string, unknown> | null }; result: { operations: Operation[]; text: string; usage: Usage } };
} }

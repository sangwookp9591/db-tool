import type { Schema, Tool, createRegistry } from './registry';
type Registry = ReturnType<typeof createRegistry>;
export interface ModelContext { registerTool: (tool: Tool, options: { signal: AbortSignal }) => void }
export interface AgentAPI { listTools: () => Omit<Tool, 'execute'>[]; callTool: (name: string, input: unknown, context?: { signal?: AbortSignal }) => Promise<unknown> }
declare global {
  interface DBTool { agent?: AgentAPI }
  interface Document { modelContext?: ModelContext }
  interface Navigator { modelContext?: ModelContext }
}
export function validate(schema: Schema, input: unknown, path = 'input'): void {
  const object = input !== null && typeof input === 'object' && !Array.isArray(input);
  if (schema.type === 'object') {
    if (!object) throw new Error(`${path} must be an object`);
    const a = input as Record<string, unknown>;
    for (const key of schema.required ?? []) if (!Object.hasOwn(a, key)) throw new Error(`${path}.${key} is required`);
    for (const [key, value] of Object.entries(a)) {
      const child = schema.properties?.[key];
      if (child) validate(child, value, `${path}.${key}`);
      else if (schema.additionalProperties === false) throw new Error(`${path}.${key} is not allowed`);
    }
  } else if (schema.type === 'integer') {
    if (typeof input !== 'number' || !Number.isSafeInteger(input) || input < (schema.minimum ?? -Infinity) || input > (schema.maximum ?? Infinity)) throw new Error(`${path} is outside its integer range`);
  } else if (typeof input !== schema.type) throw new Error(`${path} must be ${schema.type}`);
}
export function expose(registry: Registry, target: { dbtool?: object } = window) {
  const api: AgentAPI = {
    listTools: () => registry.listTools().map(({ execute: _, ...tool }) => tool),
    callTool: async (name, input, context) => {
      const tool = registry.listTools().find(t => t.name === name);
      if (!tool) return { error: { kind: 'Unsupported', message: 'Unknown or unavailable tool' } };
      try {
        validate(tool.inputSchema, input);
        // Reject cyclic, BigInt and undefined-containing input at the automation boundary.
        const json = JSON.stringify(input, (_key, value) => {
          if (value === undefined || typeof value === 'bigint' || typeof value === 'function' || typeof value === 'symbol' || (typeof value === 'number' && !Number.isFinite(value))) throw new Error('Input must contain only JSON values');
          return value;
        });
        if (!json) throw new Error('Input must be JSON serializable');
        return JSON.parse(JSON.stringify(await registry.call(tool, JSON.parse(json), context)));
      } catch (e) { return { error: { kind: 'InvalidInput', message: (e as Error).message } }; }
    }
  };
  target.dbtool = { ...target.dbtool, agent: api }; return api;
}
export function register(api: AgentAPI, context = document.modelContext ?? navigator.modelContext): () => void {
  const controller = new AbortController();
  if (context) for (const tool of api.listTools()) context.registerTool({ ...tool, execute: (input, options) => api.callTool(tool.name, input, options) }, { signal: controller.signal });
  return () => controller.abort();
}

export interface BenchConnection { fields: Record<string, string>; secret: string }
export interface BenchConfig { table: BenchConnection; graph: BenchConnection }

export interface ContractResponse {
  contract: Record<string, unknown>;
  contract_hash: string;
  ambiguity_score: number;
  compile_warnings: string[];
}

export interface PlanResponse {
  plan: Record<string, unknown>;
  plan_hash: string;
}

export interface ProposeResponse {
  receipt: {
    commit_index: number;
    new_version: number;
    key: string;
    committed_at_ms: number;
  };
}

export interface DemoResponse {
  track: string;
  contract_hash: string;
  plan_hash: string;
  traceparent: string;
  ambiguity_score: number;
  warnings: string[];
  entries_created: string[];
  elapsed_ms: number;
}

export interface HealthResponse {
  status: string;
  version: string;
}

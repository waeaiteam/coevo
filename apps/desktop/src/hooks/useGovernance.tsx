import { createContext, useContext, useState } from "react";

export type GovPhase = "idle" | "review" | "executing" | "done";

export interface GovernanceState {
  phase: GovPhase;
  track: string;
  contractHash: string;
  planHash: string;
  contract: Record<string, unknown> | null;
  agents: string[];
  riskDecision: string;
  approvalMode: string;
  actionModes: string[];
  approvalRequired: boolean;
  traceparent: string;
}

const defaults: GovernanceState = {
  phase: "idle",
  track: "",
  contractHash: "",
  planHash: "",
  contract: null,
  agents: [],
  riskDecision: "",
  approvalMode: "",
  actionModes: [],
  approvalRequired: false,
  traceparent: "",
};

const GovernanceCtx = createContext<{
  state: GovernanceState;
  set: (s: GovernanceState) => void;
  reset: () => void;
}>({ state: defaults, set: () => {}, reset: () => {} });

export function GovernanceProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<GovernanceState>(defaults);
  const reset = () => setState(defaults);
  return <GovernanceCtx.Provider value={{ state, set: setState, reset }}>{children}</GovernanceCtx.Provider>;
}

export function useGovernance() {
  return useContext(GovernanceCtx);
}

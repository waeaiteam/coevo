import { createContext, useContext, useState } from "react";

export interface GovernanceState {
  track: string;
  contractHash: string;
  planHash: string;
  agents: string[];
  riskDecision: string;
  approvalRequired: boolean;
  traceparent: string;
}

const GovernanceCtx = createContext<{
  state: GovernanceState | null;
  set: (s: GovernanceState) => void;
}>({ state: null, set: () => {} });

export function GovernanceProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<GovernanceState | null>(null);
  return <GovernanceCtx.Provider value={{ state, set: setState }}>{children}</GovernanceCtx.Provider>;
}

export function useGovernance() {
  return useContext(GovernanceCtx);
}

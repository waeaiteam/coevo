import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { immer } from 'zustand/middleware/immer';

export interface WorkerRun {
  run_id: string;
  worker_id: string;
  status: 'queued' | 'running' | 'completed' | 'failed';
  steps: WorkerStep[];
  created_at: string;
  updated_at: string;
}

export interface WorkerStep {
  step_id: string;
  step_type: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  output?: string;
  error?: string;
}

interface WorkerState {
  runs: Record<string, WorkerRun>;
  activeRunId: string | null;

  addRun: (run: WorkerRun) => void;
  updateRun: (runId: string, updates: Partial<WorkerRun>) => void;
  setActiveRun: (runId: string | null) => void;
  addStep: (runId: string, step: WorkerStep) => void;
  updateStep: (runId: string, stepId: string, updates: Partial<WorkerStep>) => void;
  clearRuns: () => void;
}

export const useWorkerStore = create<WorkerState>()(
  persist(
    immer((set) => ({
      runs: {},
      activeRunId: null,

      addRun: (run: WorkerRun) =>
        set((state: WorkerState) => {
          state.runs[run.run_id] = run;
        }),

      updateRun: (runId: string, updates: Partial<WorkerRun>) =>
        set((state: WorkerState) => {
          if (state.runs[runId]) {
            Object.assign(state.runs[runId], updates);
          }
        }),

      setActiveRun: (runId: string | null) =>
        set((state: WorkerState) => {
          state.activeRunId = runId;
        }),

      addStep: (runId: string, step: WorkerStep) =>
        set((state: WorkerState) => {
          if (state.runs[runId]) {
            state.runs[runId].steps.push(step);
          }
        }),

      updateStep: (runId: string, stepId: string, updates: Partial<WorkerStep>) =>
        set((state: WorkerState) => {
          const run = state.runs[runId];
          if (run) {
            const step = run.steps.find((s: WorkerStep) => s.step_id === stepId);
            if (step) {
              Object.assign(step, updates);
            }
          }
        }),

      clearRuns: () =>
        set((state: WorkerState) => {
          state.runs = {};
          state.activeRunId = null;
        }),
    })),
    {
      name: 'coevo-worker-storage',
      partialize: (state: WorkerState) => ({
        runs: Object.fromEntries(
          Object.entries(state.runs).slice(-50)
        ),
        activeRunId: state.activeRunId,
      }),
    }
  )
);

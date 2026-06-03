import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { immer } from 'zustand/middleware/immer';
import type { Evaluator, EvaluationJob } from '../evaluation/evaluators';
import { evaluationManager } from '../evaluation/evaluators';

interface EvaluationState {
  // 评估器列表
  evaluators: Evaluator[];

  // 评估任务
  jobs: Record<string, EvaluationJob>;

  // 活跃任务ID
  activeJobId: string | null;

  // Actions
  loadEvaluators: () => void;
  runEvaluation: (evaluatorId: string, targetId: string, data: any) => Promise<EvaluationJob>;
  getJob: (jobId: string) => EvaluationJob | undefined;
  refreshJob: (jobId: string) => void;
  setActiveJob: (jobId: string | null) => void;
  clearJobs: () => void;
}

export const useEvaluationStore = create<EvaluationState>()(
  persist(
    immer((set, get) => ({
      evaluators: [],
      jobs: {},
      activeJobId: null,

      loadEvaluators: () => {
        const evaluators = evaluationManager.getEvaluators();
        set(state => {
          state.evaluators = evaluators;
        });
      },

      runEvaluation: async (evaluatorId: string, targetId: string, data: any) => {
        const job = await evaluationManager.runEvaluation(evaluatorId, targetId, data);

        set(state => {
          state.jobs[job.job_id] = job;
          state.activeJobId = job.job_id;
        });

        // 轮询任务状态更新
        const pollInterval = setInterval(() => {
          const updatedJob = evaluationManager.getJob(job.job_id);
          if (updatedJob) {
            set(state => {
              state.jobs[job.job_id] = updatedJob;
            });

            if (updatedJob.status === 'completed' || updatedJob.status === 'failed') {
              clearInterval(pollInterval);
            }
          }
        }, 500);

        return job;
      },

      getJob: (jobId: string) => {
        return get().jobs[jobId];
      },

      refreshJob: (jobId: string) => {
        const updatedJob = evaluationManager.getJob(jobId);
        if (updatedJob) {
          set(state => {
            state.jobs[jobId] = updatedJob;
          });
        }
      },

      setActiveJob: (jobId: string | null) => {
        set(state => {
          state.activeJobId = jobId;
        });
      },

      clearJobs: () => {
        set(state => {
          state.jobs = {};
          state.activeJobId = null;
        });
      },
    })),
    {
      name: 'coevo-evaluation-storage',
      partialize: (state) => ({
        jobs: state.jobs,
        activeJobId: state.activeJobId,
      }),
    }
  )
);

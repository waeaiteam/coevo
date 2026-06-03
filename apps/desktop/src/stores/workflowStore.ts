import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { immer } from 'zustand/middleware/immer';
import type { Workflow, WorkflowNode, WorkflowEdge } from '../engine/dag';

export interface WorkflowExecution {
  execution_id: string;
  workflow_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  current_node?: string;
  results: Record<string, any>;
  errors: Record<string, string>;
  started_at: string;
  completed_at?: string;
}

interface WorkflowState {
  workflows: Record<string, Workflow>;
  executions: Record<string, WorkflowExecution>;
  activeWorkflowId: string | null;

  addWorkflow: (workflow: Workflow) => void;
  updateWorkflow: (id: string, updates: Partial<Workflow>) => void;
  deleteWorkflow: (id: string) => void;
  setActiveWorkflow: (id: string | null) => void;

  addNode: (workflowId: string, node: WorkflowNode) => void;
  updateNode: (workflowId: string, nodeId: string, updates: Partial<WorkflowNode>) => void;
  deleteNode: (workflowId: string, nodeId: string) => void;

  addEdge: (workflowId: string, edge: WorkflowEdge) => void;
  deleteEdge: (workflowId: string, edgeId: string) => void;

  startExecution: (execution: WorkflowExecution) => void;
  updateExecution: (executionId: string, updates: Partial<WorkflowExecution>) => void;
}

export const useWorkflowStore = create<WorkflowState>()(
  persist(
    immer((set) => ({
      workflows: {},
      executions: {},
      activeWorkflowId: null,

      addWorkflow: (workflow: Workflow) =>
        set((state: WorkflowState) => {
          state.workflows[workflow.id] = workflow;
        }),

      updateWorkflow: (id: string, updates: Partial<Workflow>) =>
        set((state: WorkflowState) => {
          if (state.workflows[id]) {
            Object.assign(state.workflows[id], updates);
          }
        }),

      deleteWorkflow: (id: string) =>
        set((state: WorkflowState) => {
          delete state.workflows[id];
          if (state.activeWorkflowId === id) {
            state.activeWorkflowId = null;
          }
        }),

      setActiveWorkflow: (id: string | null) =>
        set((state: WorkflowState) => {
          state.activeWorkflowId = id;
        }),

      addNode: (workflowId: string, node: WorkflowNode) =>
        set((state: WorkflowState) => {
          const workflow = state.workflows[workflowId];
          if (workflow) {
            workflow.nodes.push(node);
          }
        }),

      updateNode: (workflowId: string, nodeId: string, updates: Partial<WorkflowNode>) =>
        set((state: WorkflowState) => {
          const workflow = state.workflows[workflowId];
          if (workflow) {
            const node = workflow.nodes.find((n: WorkflowNode) => n.id === nodeId);
            if (node) {
              Object.assign(node, updates);
            }
          }
        }),

      deleteNode: (workflowId: string, nodeId: string) =>
        set((state: WorkflowState) => {
          const workflow = state.workflows[workflowId];
          if (workflow) {
            workflow.nodes = workflow.nodes.filter((n: WorkflowNode) => n.id !== nodeId);
            workflow.edges = workflow.edges.filter(
              (e: WorkflowEdge) => e.source !== nodeId && e.target !== nodeId
            );
          }
        }),

      addEdge: (workflowId: string, edge: WorkflowEdge) =>
        set((state: WorkflowState) => {
          const workflow = state.workflows[workflowId];
          if (workflow) {
            workflow.edges.push(edge);
          }
        }),

      deleteEdge: (workflowId: string, edgeId: string) =>
        set((state: WorkflowState) => {
          const workflow = state.workflows[workflowId];
          if (workflow) {
            workflow.edges = workflow.edges.filter((e: WorkflowEdge) => e.id !== edgeId);
          }
        }),

      startExecution: (execution: WorkflowExecution) =>
        set((state: WorkflowState) => {
          state.executions[execution.execution_id] = execution;
        }),

      updateExecution: (executionId: string, updates: Partial<WorkflowExecution>) =>
        set((state: WorkflowState) => {
          if (state.executions[executionId]) {
            Object.assign(state.executions[executionId], updates);
          }
        }),
    })),
    {
      name: 'coevo-workflow-storage',
      partialize: (state: WorkflowState) => ({
        workflows: state.workflows,
        activeWorkflowId: state.activeWorkflowId,
        executions: Object.fromEntries(
          Object.entries(state.executions).slice(-20)
        ),
      }),
    }
  )
);

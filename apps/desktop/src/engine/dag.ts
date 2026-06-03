export interface WorkflowNode {
  id: string;
  type: string;
  data: Record<string, any>;
  inputs: string[];
  outputs: string[];
}

export interface WorkflowEdge {
  id: string;
  source: string;
  target: string;
  sourceOutput?: string;
  targetInput?: string;
}

export interface Workflow {
  id: string;
  name: string;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export interface ExecutionResult {
  status: 'completed' | 'failed' | 'partial';
  results: Map<string, any>;
  errors: Map<string, Error>;
}

export class DAGExecutor {
  private nodeExecutors: Map<string, (node: WorkflowNode, inputs: Record<string, any>) => Promise<any>>;

  constructor() {
    this.nodeExecutors = new Map();
  }

  registerNodeType(type: string, executor: (node: WorkflowNode, inputs: Record<string, any>) => Promise<any>) {
    this.nodeExecutors.set(type, executor);
  }

  async execute(workflow: Workflow): Promise<ExecutionResult> {
    const results = new Map<string, any>();
    const errors = new Map<string, Error>();

    // Detect cycles
    if (this.hasCycle(workflow)) {
      throw new Error('Workflow contains cycles');
    }

    // Topological sort
    const sortedNodes = this.topologicalSort(workflow);

    // Execute nodes in order
    for (const node of sortedNodes) {
      try {
        const inputs = this.resolveInputs(node, workflow.edges, results);
        const executor = this.nodeExecutors.get(node.type);

        if (!executor) {
          throw new Error(`No executor found for node type: ${node.type}`);
        }

        const output = await executor(node, inputs);
        results.set(node.id, output);
      } catch (error) {
        errors.set(node.id, error as Error);
        // Continue execution for independent branches
      }
    }

    return {
      status: errors.size === 0 ? 'completed' : errors.size === sortedNodes.length ? 'failed' : 'partial',
      results,
      errors,
    };
  }

  private topologicalSort(workflow: Workflow): WorkflowNode[] {
    const { nodes, edges } = workflow;
    const inDegree = new Map<string, number>();
    const adjList = new Map<string, string[]>();

    // Initialize
    nodes.forEach((node) => {
      inDegree.set(node.id, 0);
      adjList.set(node.id, []);
    });

    // Build adjacency list and in-degree
    edges.forEach((edge) => {
      adjList.get(edge.source)?.push(edge.target);
      inDegree.set(edge.target, (inDegree.get(edge.target) || 0) + 1);
    });

    // Kahn's algorithm
    const queue: string[] = [];
    const result: WorkflowNode[] = [];

    inDegree.forEach((degree, nodeId) => {
      if (degree === 0) {
        queue.push(nodeId);
      }
    });

    while (queue.length > 0) {
      const nodeId = queue.shift()!;
      const node = nodes.find((n) => n.id === nodeId);
      if (node) {
        result.push(node);
      }

      adjList.get(nodeId)?.forEach((neighbor) => {
        const newDegree = (inDegree.get(neighbor) || 0) - 1;
        inDegree.set(neighbor, newDegree);
        if (newDegree === 0) {
          queue.push(neighbor);
        }
      });
    }

    if (result.length !== nodes.length) {
      throw new Error('Topological sort failed - graph has cycles');
    }

    return result;
  }

  private hasCycle(workflow: Workflow): boolean {
    const { nodes, edges } = workflow;
    const visited = new Set<string>();
    const recStack = new Set<string>();

    const adjList = new Map<string, string[]>();
    nodes.forEach((node) => adjList.set(node.id, []));
    edges.forEach((edge) => {
      adjList.get(edge.source)?.push(edge.target);
    });

    const dfs = (nodeId: string): boolean => {
      visited.add(nodeId);
      recStack.add(nodeId);

      for (const neighbor of adjList.get(nodeId) || []) {
        if (!visited.has(neighbor)) {
          if (dfs(neighbor)) return true;
        } else if (recStack.has(neighbor)) {
          return true;
        }
      }

      recStack.delete(nodeId);
      return false;
    };

    for (const node of nodes) {
      if (!visited.has(node.id)) {
        if (dfs(node.id)) {
          return true;
        }
      }
    }

    return false;
  }

  private resolveInputs(
    node: WorkflowNode,
    edges: WorkflowEdge[],
    results: Map<string, any>
  ): Record<string, any> {
    const inputs: Record<string, any> = {};

    const incomingEdges = edges.filter((e) => e.target === node.id);

    incomingEdges.forEach((edge) => {
      const sourceResult = results.get(edge.source);
      if (sourceResult !== undefined) {
        const inputKey = edge.targetInput || 'input';
        inputs[inputKey] = sourceResult;
      }
    });

    return inputs;
  }
}

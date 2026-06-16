import { getAgentGrowth, type AgentGrowth } from '../api/client';

export type EvaluatorType = 'code' | 'agent' | 'custom_rpc';

export interface EvaluationMetric {
  name: string;
  weight: number;
  threshold?: number;
}

export interface EvaluationResult {
  metric_name: string;
  score: number;
  passed: boolean;
  details?: string;
  error?: string;
}

export interface Evaluator {
  id: string;
  name: string;
  type: EvaluatorType;
  description: string;
  metrics: EvaluationMetric[];
  config: Record<string, any>;
}

export interface CustomRpcEvaluatorConfig {
  endpoint: string;
  auth_token: string;
}

export interface EvaluationJob {
  job_id: string;
  evaluator_id: string;
  target_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  created_at: number;
  started_at?: number;
  completed_at?: number;
  results: EvaluationResult[];
  error?: string;
  duration_ms?: number;
}

export class CodeEvaluator implements Evaluator {
  id = 'code-evaluator-' + Date.now();
  name = 'Code Quality Evaluator';
  type: EvaluatorType = 'code';
  description = 'Evaluates code quality using static analysis';
  metrics: EvaluationMetric[] = [
    { name: 'correctness', weight: 0.4, threshold: 90 },
    { name: 'readability', weight: 0.3, threshold: 70 },
    { name: 'performance', weight: 0.3, threshold: 70 },
  ];
  config = {
    language: 'javascript',
    strict: true,
  };

  async evaluate(code: string): Promise<EvaluationResult[]> {
    const results: EvaluationResult[] = [];

    const hasTests = code.includes('test(') || code.includes('describe(');
    const hasComments = code.includes('//') || code.includes('/*');
    const linesCount = code.split('\n').length;

    results.push({
      metric_name: 'correctness',
      score: hasTests ? 95 : 70,
      passed: hasTests,
      details: hasTests ? 'Tests found' : 'No tests found',
    });

    results.push({
      metric_name: 'readability',
      score: hasComments ? 85 : 60,
      passed: hasComments,
      details: hasComments ? 'Comments present' : 'Needs more comments',
    });

    results.push({
      metric_name: 'performance',
      score: linesCount < 200 ? 90 : 70,
      passed: linesCount < 200,
      details: `Lines: ${linesCount}`,
    });

    return results;
  }
}

export class AgentEvaluator implements Evaluator {
  id = 'agent-evaluator-' + Date.now();
  name = 'Agent Performance Evaluator';
  type: EvaluatorType = 'agent';
  description = 'Evaluates agent behavior and outputs';
  metrics: EvaluationMetric[] = [
    { name: 'accuracy', weight: 0.4, threshold: 80 },
    { name: 'latency', weight: 0.3, threshold: 70 },
    { name: 'cost', weight: 0.3, threshold: 70 },
  ];
  config = {
    test_cases: 10,
    timeout_ms: 30000,
  };

  async evaluate(agentId: string, _testCases: any[]): Promise<EvaluationResult[]> {
    const results: EvaluationResult[] = [];

    let growth: AgentGrowth | null = null;
    try {
      growth = await getAgentGrowth(agentId);
    } catch {
      growth = null;
    }

    if (!growth || growth.total_tasks === 0) {
      return [
        {
          metric_name: 'accuracy',
          score: 0,
          passed: false,
          details: 'No execution history yet - assign tasks to this employee first.',
        },
      ];
    }

    const accuracy = growth.success_rate;
    results.push({
      metric_name: 'accuracy',
      score: accuracy,
      passed: accuracy >= 80,
      details: `${growth.completed_tasks}/${growth.total_tasks} tasks completed`,
    });

    const avgLatency = growth.avg_latency_ms;
    const latencyScore = avgLatency > 0 ? (avgLatency < 2000 ? 90 : avgLatency < 5000 ? 70 : 50) : 0;
    results.push({
      metric_name: 'latency',
      score: latencyScore,
      passed: avgLatency > 0 && avgLatency < 2000,
      details: `Avg: ${Math.round(avgLatency)}ms`,
    });

    const avgCost = growth.total_tasks > 0 ? growth.total_cost_usd / growth.total_tasks : 0;
    const costScore = avgCost < 0.05 ? 90 : avgCost < 0.2 ? 70 : 50;
    results.push({
      metric_name: 'cost',
      score: costScore,
      passed: avgCost < 0.2,
      details: `Avg per task: $${avgCost.toFixed(4)}`,
    });

    return results;
  }
}

export class CustomRPCEvaluator implements Evaluator {
  id = 'custom-rpc-evaluator';
  name = 'Custom RPC Evaluator';
  type: EvaluatorType = 'custom_rpc';
  description = 'Evaluates using custom external service';
  metrics: EvaluationMetric[] = [
    { name: 'custom_score', weight: 1.0, threshold: 70 },
  ];
  config: CustomRpcEvaluatorConfig = loadCustomRpcEvaluatorConfig();

  getConfig(): CustomRpcEvaluatorConfig {
    return { ...this.config };
  }

  setConfig(next: CustomRpcEvaluatorConfig): void {
    this.config = {
      endpoint: next.endpoint.trim(),
      auth_token: next.auth_token.trim(),
    };
    saveCustomRpcEvaluatorConfig(this.config);
  }

  async evaluate(data: any): Promise<EvaluationResult[]> {
    if (!this.config.endpoint.trim()) {
      return [{
        metric_name: 'custom_score',
        score: 0,
        passed: false,
        error: 'Custom RPC endpoint is not configured.',
      }];
    }

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (this.config.auth_token.trim()) {
      headers.Authorization = `Bearer ${this.config.auth_token.trim()}`;
    }

    try {
      const response = await fetch(this.config.endpoint, {
        method: 'POST',
        headers,
        body: JSON.stringify(data),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const result = await response.json();
      return [{
        metric_name: 'custom_score',
        score: result.score || 0,
        passed: result.passed || false,
        details: result.details || '',
      }];
    } catch (error) {
      return [{
        metric_name: 'custom_score',
        score: 0,
        passed: false,
        error: error instanceof Error ? error.message : String(error),
      }];
    }
  }
}

export class EvaluationManager {
  private evaluators = new Map<string, Evaluator>();
  private jobs = new Map<string, EvaluationJob>();

  registerEvaluator(evaluator: Evaluator): void {
    this.evaluators.set(evaluator.id, evaluator);
  }

  async runEvaluation(
    evaluatorId: string,
    targetId: string,
    data: any,
  ): Promise<EvaluationJob> {
    const evaluator = this.evaluators.get(evaluatorId);
    if (!evaluator) {
      throw new Error(`Evaluator ${evaluatorId} not found`);
    }

    const job: EvaluationJob = {
      job_id: `eval_${Date.now()}_${crypto.randomUUID().slice(0, 9)}`,
      evaluator_id: evaluatorId,
      target_id: targetId,
      status: 'pending',
      created_at: Date.now(),
      results: [],
    };

    const mutableJob = cloneJob(job);
    this.jobs.set(job.job_id, mutableJob);

    this.executeJob(mutableJob, evaluator, data).catch((error) => {
      mutableJob.status = 'failed';
      mutableJob.error = error.message;
    });

    return cloneJob(mutableJob);
  }

  private async executeJob(
    job: EvaluationJob,
    evaluator: Evaluator,
    data: any,
  ): Promise<void> {
    job.status = 'running';
    job.started_at = Date.now();

    try {
      let results: EvaluationResult[];

      if (evaluator instanceof CodeEvaluator) {
        results = await evaluator.evaluate(data);
      } else if (evaluator instanceof AgentEvaluator) {
        results = await evaluator.evaluate(data.agentId, data.testCases);
      } else if (evaluator instanceof CustomRPCEvaluator) {
        results = await evaluator.evaluate(data);
      } else {
        throw new Error('Unknown evaluator type');
      }

      job.results = results;
      job.status = 'completed';
      job.completed_at = Date.now();
      job.duration_ms = job.completed_at - job.started_at;
    } catch (error) {
      job.status = 'failed';
      job.error = error instanceof Error ? error.message : String(error);
      job.completed_at = Date.now();
      job.duration_ms = job.completed_at - (job.started_at || job.created_at);
    }
  }

  getJob(jobId: string): EvaluationJob | undefined {
    const job = this.jobs.get(jobId);
    return job ? cloneJob(job) : undefined;
  }

  listJobs(): EvaluationJob[] {
    return Array.from(this.jobs.values()).map(cloneJob);
  }

  getEvaluators(): Evaluator[] {
    return Array.from(this.evaluators.values());
  }

  getCustomRpcConfig(): CustomRpcEvaluatorConfig {
    const evaluator = this.getCustomRpcEvaluator();
    return evaluator ? evaluator.getConfig() : { endpoint: '', auth_token: '' };
  }

  setCustomRpcConfig(next: CustomRpcEvaluatorConfig): void {
    const evaluator = this.getCustomRpcEvaluator();
    if (evaluator) {
      evaluator.setConfig(next);
    }
  }

  private getCustomRpcEvaluator(): CustomRPCEvaluator | undefined {
    return Array.from(this.evaluators.values()).find(
      (evaluator): evaluator is CustomRPCEvaluator => evaluator instanceof CustomRPCEvaluator,
    );
  }
}

export const evaluationManager = new EvaluationManager();

evaluationManager.registerEvaluator(new CodeEvaluator());
evaluationManager.registerEvaluator(new AgentEvaluator());
evaluationManager.registerEvaluator(new CustomRPCEvaluator());

const CUSTOM_RPC_STORAGE_KEY = 'coevo-custom-rpc-evaluator-config';

function loadCustomRpcEvaluatorConfig(): CustomRpcEvaluatorConfig {
  try {
    const raw = localStorage.getItem(CUSTOM_RPC_STORAGE_KEY);
    if (!raw) return { endpoint: '', auth_token: '' };
    const parsed = JSON.parse(raw) as Partial<CustomRpcEvaluatorConfig>;
    return {
      endpoint: String(parsed.endpoint || '').trim(),
      auth_token: String(parsed.auth_token || '').trim(),
    };
  } catch {
    return { endpoint: '', auth_token: '' };
  }
}

function saveCustomRpcEvaluatorConfig(config: CustomRpcEvaluatorConfig): void {
  localStorage.setItem(
    CUSTOM_RPC_STORAGE_KEY,
    JSON.stringify({
      endpoint: config.endpoint.trim(),
      auth_token: config.auth_token.trim(),
    }),
  );
}

function cloneJob(job: EvaluationJob): EvaluationJob {
  return {
    ...job,
    results: job.results.map((result) => ({ ...result })),
  };
}

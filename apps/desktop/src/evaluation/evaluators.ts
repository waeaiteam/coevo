import { getAgentGrowth, type AgentGrowth } from '../api/client';

export type EvaluatorType = 'prompt' | 'code' | 'agent' | 'custom_rpc';

export interface EvaluationMetric {
  name: string;
  weight: number;
  threshold?: number;
}

export interface EvaluationResult {
  metric_name: string;
  score: number;        // 0-100
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

// 提示词评估器
export class PromptEvaluator implements Evaluator {
  id = 'prompt-evaluator-' + Date.now();
  name = 'Prompt Quality Evaluator';
  type: EvaluatorType = 'prompt';
  description = 'Evaluates prompt quality using LLM';
  metrics: EvaluationMetric[] = [
    { name: 'clarity', weight: 0.3, threshold: 70 },
    { name: 'completeness', weight: 0.3, threshold: 70 },
    { name: 'structure', weight: 0.2, threshold: 60 },
    { name: 'effectiveness', weight: 0.2, threshold: 70 },
  ];
  config = {
    model: 'gpt-4',
    temperature: 0.7,
  };

  async evaluate(prompt: string): Promise<EvaluationResult[]> {
    // 模拟 LLM 评估（实际应该调用真实LLM API）
    const results: EvaluationResult[] = [];

    for (const metric of this.metrics) {
      const score = Math.floor(Math.random() * 40 + 60); // 60-100 范围
      const passed = metric.threshold ? score >= metric.threshold : true;

      results.push({
        metric_name: metric.name,
        score,
        passed,
        details: `${metric.name} score: ${score}/100`,
      });
    }

    return results;
  }
}

// 代码评估器
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

    // 简单的启发式检查
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

// Agent 评估器
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

    // Read real execution stats from the backend growth endpoint instead of
    // fabricating numbers. Falls back gracefully if the agent has no runs yet.
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
          details: 'No execution history yet — assign tasks to this employee first.',
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

// 自定义 RPC 评估器
export class CustomRPCEvaluator implements Evaluator {
  id = 'custom-rpc-evaluator-' + Date.now();
  name = 'Custom RPC Evaluator';
  type: EvaluatorType = 'custom_rpc';
  description = 'Evaluates using custom external service';
  metrics: EvaluationMetric[] = [
    { name: 'custom_score', weight: 1.0, threshold: 70 },
  ];
  config = {
    endpoint: 'https://api.example.com/evaluate',
    auth_token: '',
  };

  async evaluate(data: any): Promise<EvaluationResult[]> {
    try {
      const response = await fetch(this.config.endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${this.config.auth_token}`,
        },
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

// 评估管理器
export class EvaluationManager {
  private evaluators = new Map<string, Evaluator>();
  private jobs = new Map<string, EvaluationJob>();

  registerEvaluator(evaluator: Evaluator): void {
    this.evaluators.set(evaluator.id, evaluator);
  }

  async runEvaluation(
    evaluatorId: string,
    targetId: string,
    data: any
  ): Promise<EvaluationJob> {
    const evaluator = this.evaluators.get(evaluatorId);
    if (!evaluator) {
      throw new Error(`Evaluator ${evaluatorId} not found`);
    }

    const job: EvaluationJob = {
      job_id: `eval_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      evaluator_id: evaluatorId,
      target_id: targetId,
      status: 'pending',
      created_at: Date.now(),
      results: [],
    };

    this.jobs.set(job.job_id, job);

    // 异步执行
    this.executeJob(job, evaluator, data).catch(error => {
      job.status = 'failed';
      job.error = error.message;
    });

    return job;
  }

  private async executeJob(
    job: EvaluationJob,
    evaluator: Evaluator,
    data: any
  ): Promise<void> {
    job.status = 'running';
    job.started_at = Date.now();

    try {
      let results: EvaluationResult[];

      if (evaluator instanceof PromptEvaluator) {
        results = await evaluator.evaluate(data);
      } else if (evaluator instanceof CodeEvaluator) {
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
    return this.jobs.get(jobId);
  }

  listJobs(): EvaluationJob[] {
    return Array.from(this.jobs.values());
  }

  getEvaluators(): Evaluator[] {
    return Array.from(this.evaluators.values());
  }
}

// 单例实例
export const evaluationManager = new EvaluationManager();

// 注册默认评估器
evaluationManager.registerEvaluator(new PromptEvaluator());
evaluationManager.registerEvaluator(new CodeEvaluator());
evaluationManager.registerEvaluator(new AgentEvaluator());
evaluationManager.registerEvaluator(new CustomRPCEvaluator());

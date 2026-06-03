import { useMemo, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useWorkflowStore } from "../stores/workflowStore";
import { DAGExecutor } from "../engine/dag";
import type { Workflow, WorkflowNode } from "../engine/dag";

const NODE_TYPES: Array<{ type: string; labelKey: string; icon: Parameters<typeof Icon>[0]["name"] }> = [
  { type: "llm", labelKey: "workflows.step_think", icon: "brain" },
  { type: "tool", labelKey: "workflows.step_tool", icon: "wrench" },
  { type: "condition", labelKey: "workflows.step_condition", icon: "git-branch" },
  { type: "http", labelKey: "workflows.step_fetch", icon: "external" },
];

function newWorkflow(): Workflow {
  return {
    id: `wf_${Date.now()}`,
    name: "Untitled workflow",
    nodes: [],
    edges: [],
  };
}

export default function Workflows() {
  useLanguage();
  const {
    workflows,
    activeWorkflowId,
    addWorkflow,
    setActiveWorkflow,
    addNode,
    deleteNode,
    addEdge,
    deleteWorkflow,
  } = useWorkflowStore();

  const [runResult, setRunResult] = useState<string>("");
  const [running, setRunning] = useState(false);

  const list = useMemo(() => Object.values(workflows), [workflows]);
  const active = activeWorkflowId ? workflows[activeWorkflowId] : undefined;

  function handleCreate() {
    const wf = newWorkflow();
    addWorkflow(wf);
    setActiveWorkflow(wf.id);
  }

  function handleAddNode(type: string) {
    if (!active) return;
    const id = `n_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
    const labelKey = NODE_TYPES.find((n) => n.type === type)?.labelKey || "workflows.step_think";
    const node: WorkflowNode = {
      id,
      type,
      data: { label: t(labelKey) },
      inputs: [],
      outputs: ["out"],
    };
    addNode(active.id, node);
    // Auto-chain to previous node for a simple linear default.
    if (active.nodes.length > 0) {
      const prev = active.nodes[active.nodes.length - 1];
      addEdge(active.id, { id: `e_${id}`, source: prev.id, target: id });
    }
  }

  async function handleRun() {
    if (!active || running) return;
    setRunning(true);
    setRunResult("");
    try {
      const executor = new DAGExecutor();
      for (const nt of NODE_TYPES) {
        executor.registerNodeType(nt.type, async (node, inputs) => {
          await new Promise((r) => setTimeout(r, 120));
          return { node: node.id, type: nt.type, inputs };
        });
      }
      const result = await executor.execute(active);
      setRunResult(
        result.status === "completed"
          ? t("workflows.run_done").replace("{n}", String(result.results.size))
          : result.status === "partial"
            ? t("workflows.run_partial").replace("{n}", String(result.errors.size))
            : t("workflows.run_failed")
      );
    } catch (e) {
      setRunResult(`error: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setRunning(false);
    }
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("workflows.kicker")}</div>
          <h1 className="product-title">{t("workflows.title")}</h1>
        </div>
        <div className="product-actions">
          <button className="primary-button" onClick={handleCreate}>
            <Icon name="plus" /> {t("workflows.new")}
          </button>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="git-branch" /></div>
        <div>
          <h2>{t("workflows.title")}</h2>
          <p>{t("workflows.desc")}</p>
        </div>
      </section>

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("workflows.list")}</h2>
            <span>{list.length}</span>
          </div>
          {list.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="git-branch" /></div>
              <p>{t("workflows.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {list.map((wf) => (
                <div
                  key={wf.id}
                  className="product-list-row"
                  style={{ borderColor: wf.id === activeWorkflowId ? "var(--accent)" : undefined }}
                >
                  <button className="product-row-main text-left" onClick={() => setActiveWorkflow(wf.id)}>
                    {wf.name}
                  </button>
                  <span className="flex items-center gap-2">
                    <span className="mono-chip">{wf.nodes.length} nodes</span>
                    <button className="icon-button" onClick={() => deleteWorkflow(wf.id)} aria-label={t("workflows.delete")}>
                      <Icon name="x" />
                    </button>
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("workflows.canvas")}</h2>
            {active && (
              <button className="product-link-button" disabled={running} onClick={handleRun}>
                {running ? <Icon name="spinner" className="icon-spin" /> : <Icon name="send" />} {t("workflows.run")}
              </button>
            )}
          </div>

          {!active ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>{t("workflows.select")}</p>
            </div>
          ) : (
            <>
              <div className="chip-row mb-3">
                {NODE_TYPES.map((nt) => (
                  <button key={nt.type} className="product-link-button" onClick={() => handleAddNode(nt.type)}>
                    <Icon name={nt.icon} /> {t(nt.labelKey)}
                  </button>
                ))}
              </div>

              <div className="workflow-canvas">
                {active.nodes.length === 0 ? (
                  <div className="empty-state">
                    <p>{t("workflows.add_node_hint")}</p>
                  </div>
                ) : (
                  active.nodes.map((node, i) => (
                    <div key={node.id} className="workflow-node-wrap">
                      <div className="workflow-node">
                        <Icon name={(NODE_TYPES.find((n) => n.type === node.type)?.icon) || "info"} />
                        <span className="truncate">{String(node.data.label || node.type)}</span>
                        <button className="icon-button" onClick={() => deleteNode(active.id, node.id)} aria-label={t("workflows.delete")}>
                          <Icon name="x" />
                        </button>
                      </div>
                      {i < active.nodes.length - 1 && (
                        <div className="workflow-connector"><Icon name="chevron-down" /></div>
                      )}
                    </div>
                  ))
                )}
              </div>

              {runResult && <div className="mono-chip mt-3">{runResult}</div>}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

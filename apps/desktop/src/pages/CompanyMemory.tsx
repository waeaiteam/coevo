import { useEffect, useState } from "react";
import { createMemory, listMemory, markMemoryStale, revokeMemory } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

const SCOPES = ["User", "Company", "Agent", "Task", "Skill", "Executor", "Audit"];
const API_SCOPES: Record<string, string> = {
  User: "user",
  Company: "company",
  Agent: "agent",
  Task: "task",
  Skill: "skill",
  Executor: "executor",
  Audit: "audit",
};

function apiScope(scope: string) {
  return API_SCOPES[scope] || scope.toLowerCase();
}

export default function CompanyMemory() {
  useLanguage();
  const [memories, setMemories] = useState<Record<string, unknown>[]>([]);
  const [scope, setScope] = useState("");
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newContent, setNewContent] = useState("");
  const [newScope, setNewScope] = useState("Company");
  const [error, setError] = useState("");

  async function load(nextScope?: string) {
    setLoading(true);
    try {
      const rows = await listMemory(nextScope ? { scope: apiScope(nextScope) } : undefined);
      setMemories((rows as Record<string, unknown>[]) || []);
    } catch {
      setMemories([]);
    }
    setLoading(false);
  }

  useEffect(() => {
    void load();
  }, []);

  async function create() {
    if (!newTitle.trim()) return;
    setError("");
    try {
      await createMemory({
        memory_id: crypto.randomUUID(),
        scope: apiScope(newScope),
        owner_id: "default-founder",
        title: newTitle,
        content: newContent,
        tags: [],
        source: "desktop",
        provenance: "desktop:company-memory",
        confidence: 0.5,
        ttl_seconds: 86400,
        created_at_ms: Date.now(),
        updated_at_ms: Date.now(),
        access_policy: "opc-local",
        status: "active",
        cognitive_layer: "Hypothesis",
        linked_contract_hash: null,
        linked_plan_hash: null,
        linked_adr_id: null,
      });
      setShowCreate(false);
      setNewTitle("");
      setNewContent("");
      await load(scope || undefined);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg font-semibold" style={{ color: "var(--accent)" }}>M</span>
          <h2 className="text-lg font-bold">{t("memory.title")}</h2>
        </div>
        <button onClick={() => setShowCreate(!showCreate)} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>
          + {t("memory.new")}
        </button>
      </div>

      <div className="flex flex-wrap gap-2">
        <button
          onClick={() => {
            setScope("");
            void load();
          }}
          className={`px-3 py-1 text-xs rounded-md border ${!scope ? "font-bold" : ""}`}
          style={{ borderColor: "var(--border-accent)", color: !scope ? "var(--accent)" : "var(--text-secondary)" }}
        >
          {t("memory.all")}
        </button>
        {SCOPES.map((item) => (
          <button
            key={item}
            onClick={() => {
              setScope(item);
              void load(item);
            }}
            className={`px-3 py-1 text-xs rounded-md border ${scope === item ? "font-bold" : ""}`}
            style={{ borderColor: "var(--border-accent)", color: scope === item ? "var(--accent)" : "var(--text-secondary)" }}
          >
            {item}
          </button>
        ))}
      </div>

      {showCreate && (
        <div className="card space-y-2">
          <select value={newScope} onChange={(event) => setNewScope(event.target.value)} className="input" aria-label={t("memory.scope_label")}>
            {SCOPES.map((scope) => (
              <option key={scope} value={scope}>{scope}</option>
            ))}
          </select>
          <input placeholder={t("memory.title_placeholder")} value={newTitle} onChange={(event) => setNewTitle(event.target.value)} className="input" />
          <textarea placeholder={t("memory.content_placeholder")} value={newContent} onChange={(event) => setNewContent(event.target.value)} className="input" rows={3} />
          <div className="flex gap-2">
            <button onClick={create} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("memory.create")}</button>
            <button onClick={() => setShowCreate(false)} className="px-3 py-1.5 text-xs rounded-md" style={{ color: "var(--text-muted)" }}>{t("memory.cancel")}</button>
          </div>
          {error && <div className="text-xs" style={{ color: "var(--red)" }}>{error}</div>}
        </div>
      )}

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("memory.loading")}</div>}
      <div className="space-y-2">
        {memories.map((memory, index) => {
          const active = String(memory.status || "").toLowerCase() === "active";
          const fact = String(memory.cognitive_layer || "") === "Fact";
          return (
            <div key={String(memory.memory_id || index)} className="card">
              <div className="mb-1 flex items-center gap-2">
                <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: "var(--bg-secondary)", color: "var(--accent)" }}>{memory.scope as string}</span>
                <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: fact ? "var(--green-dim)" : "var(--bg-secondary)", color: fact ? "var(--green)" : "var(--text-muted)" }}>{memory.cognitive_layer as string}</span>
                <span className="text-xs" style={{ color: active ? "var(--green)" : "var(--text-muted)" }}>{memory.status as string}</span>
                <span className="text-xs" style={{ color: "var(--text-muted)" }}>{t("memory.confidence")}: {String(memory.confidence)}</span>
              </div>
              <div className="text-sm font-semibold">{memory.title as string}</div>
              <div className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>{memory.content as string}</div>
              <div className="mt-2 flex gap-2">
                <button onClick={async () => { await markMemoryStale(memory.memory_id as string); await load(scope || undefined); }} className="text-xs" style={{ color: "var(--yellow)" }}>{t("memory.stale")}</button>
                <button onClick={async () => { await revokeMemory(memory.memory_id as string); await load(scope || undefined); }} className="text-xs" style={{ color: "var(--red)" }}>{t("memory.revoke")}</button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

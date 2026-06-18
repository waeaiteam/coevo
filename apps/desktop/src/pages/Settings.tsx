import { useEffect, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import NumberField from "../components/NumberField";
import PasswordField from "../components/PasswordField";
import SelectField from "../components/SelectField";
import SettingRow from "../components/SettingRow";
import SettingsLayout from "../components/SettingsLayout";
import SettingsSection from "../components/SettingsSection";
import TextField from "../components/TextField";
import ToggleField from "../components/ToggleField";
import {
  connectMcpServer,
  createMcpServer,
  deleteMcpServer,
  discoverModels,
  disconnectMcpServer,
  getApiBase,
  listMcpServerTools,
  listMcpServers,
  testMcpServer,
  testModelConnection,
  updateMcpServer,
  updateModelConfig,
  type McpServerRecord,
  type McpServerUpsertRequest,
} from "../api/client";
import { getTauriInvoke } from "../api/tauri";
import { SettingsProvider, useSettings } from "../hooks/useSettings";
import { useAdvancedMode } from "../hooks/useAdvancedMode";
import { setAdvancedMode } from "../settings/appMode";
import { setLanguage, t, useLanguage } from "../settings/i18n";
import { chooseModelRoles, isKnownProvider, presetFor, providerOptions, type DiscoveredModel } from "../settings/modelPresets";
import { markModelProviderConfigured } from "../settings/onboarding";
import type { PolicyEngineType, ProviderType } from "../settings/types";
import { useToast } from "../components/ToastProvider";

const CATEGORIES = ["general", "appearance", "model_provider", "mcp_servers", "agent_runtime", "governance", "risk_gate", "cognitive_customs", "policy_engine", "privacy", "developer", "data_management"];

const panels: Record<string, React.FC> = {
  general: GeneralPanel,
  appearance: AppearancePanel,
  model_provider: ModelProviderPanel,
  mcp_servers: McpServersPanel,
  agent_runtime: AgentRuntimePanel,
  governance: GovernancePanel,
  risk_gate: RiskGatePanel,
  cognitive_customs: CognitiveCustomsPanel,
  policy_engine: PolicyEnginePanel,
  privacy: PrivacyPanel,
  developer: DeveloperPanel,
  data_management: DataManagementPanel,
};

export default function Settings() {
  return (
    <SettingsProvider>
      <Routes>
        <Route index element={<Navigate to="general" replace />} />
        {CATEGORIES.map((cat) => <Route key={cat} path={cat} element={<SettingsCategory cat={cat} />} />)}
      </Routes>
    </SettingsProvider>
  );
}

function SettingsCategory({ cat }: { cat: string }) {
  useLanguage();
  const Panel = panels[cat] || GeneralPanel;
  return <SettingsLayout section={cat as never} content={<Panel />} />;
}

function GeneralPanel() {
  const { settings, update } = useSettings();
  const g = settings.general;
  return (
    <SettingsSection title={t("settings.general")}>
      <SettingRow label={t("settings.default_home")} desc={t("settings.default_home_desc")}>
        <SelectField value={g.default_home} options={[{ value: "mission-chat", label: t("settings.home_mission_chat") }, { value: "dashboard", label: t("settings.home_opc") }]} onChange={(v) => update("general", { default_home: v as never })} />
      </SettingRow>
      <SettingRow label={t("settings.startup_behavior")} desc={t("settings.startup_behavior_desc")}>
        <SelectField value={g.startup_behavior} options={[{ value: "last-task", label: t("settings.open_last_task") }, { value: "new-task", label: t("settings.open_new_task") }]} onChange={(v) => update("general", { startup_behavior: v as never })} />
      </SettingRow>
    </SettingsSection>
  );
}

function AppearancePanel() {
  const { settings, update } = useSettings();
  const a = settings.appearance;
  const advancedMode = useAdvancedMode();
  return (
    <SettingsSection title={t("settings.appearance")}>
      <SettingRow label={t("settings.language")} htmlFor="appearance-language">
        <SelectField id="appearance-language" value={a.language} options={[{ value: "en", label: "English" }, { value: "zh", label: "中文" }]} onChange={(v) => { update("appearance", { language: v as never }); setLanguage(v as never); }} />
      </SettingRow>
      <SettingRow label={t("settings.advanced_mode")} desc={t("settings.advanced_mode_desc")}>
        <ToggleField checked={advancedMode} onChange={(v) => setAdvancedMode(v)} label={t("settings.advanced_mode")} />
      </SettingRow>
    </SettingsSection>
  );
}

function ModelProviderPanel() {
  const { settings, update, replaceAndMarkSaved } = useSettings();
  const navigate = useNavigate();
  const m = settings.model_provider;
  const [testResult, setTestResult] = useState<"idle" | "ok" | "fail">("idle");
  const [testMsg, setTestMsg] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [models, setModels] = useState<DiscoveredModel[]>([]);
  const selectedProvider = isKnownProvider(m.provider) ? m.provider : "openai";
  const preset = presetFor(selectedProvider);
  const safeModel = (value: string, fallback: string) => {
    const trimmed = String(value || "").trim();
    return trimmed && !trimmed.toLowerCase().includes("mock") ? trimmed : fallback;
  };
  const defaultModel = safeModel(m.default_model, preset.defaultModel);
  const fastModel = safeModel(m.fast_model, preset.fastModel);
  const reasoningModel = safeModel(m.reasoning_model, preset.reasoningModel);
  const structuredModel = safeModel(m.structured_output_model, preset.structuredModel);
  const modelOptions = [
    ...models,
    { id: defaultModel },
    { id: fastModel },
    { id: reasoningModel },
    { id: structuredModel },
  ]
    .filter((item) => item.id && !item.id.toLowerCase().includes("mock"))
    .filter((item, index, all) => all.findIndex((candidate) => candidate.id === item.id) === index)
    .map((item) => ({ value: item.id, label: item.display_name || item.id }));

  function configFromCurrent(patch: Partial<typeof m> = {}) {
    const next = { ...m, ...patch };
    const p = presetFor(isKnownProvider(next.provider) ? next.provider : selectedProvider);
    return { provider_id: "desktop", kind: p.kind, base_url: next.base_url || p.baseUrl, api_key: next.api_key, default_model: safeModel(next.default_model, p.defaultModel), fast_model: safeModel(next.fast_model, p.fastModel), reasoning_model: safeModel(next.reasoning_model, p.reasoningModel), structured_output_model: safeModel(next.structured_output_model, p.structuredModel), max_tokens: next.max_tokens || p.maxTokens, temperature: next.temperature, timeout_ms: next.request_timeout_ms, max_cost_per_task_usd: next.max_cost_per_task_usd };
  }

  function changeProvider(value: string) {
    const nextProvider = value as ProviderType;
    const p = presetFor(nextProvider);
    update("model_provider", { provider: nextProvider, base_url: p.baseUrl, default_model: p.defaultModel, fast_model: p.fastModel, reasoning_model: p.reasoningModel, structured_output_model: p.structuredModel, max_tokens: p.maxTokens });
    setModels([]);
  }

  async function handleSaveAndTestConnection() {
    setTestResult("idle");
    setTestMsg("");
    try {
      const baseConfig = configFromCurrent();
      const r = await testModelConnection(baseConfig) as Record<string, unknown>;
      let discovered: DiscoveredModel[] = [];
      try {
        discovered = ((await discoverModels(baseConfig)) as { models?: DiscoveredModel[] }).models || [];
        setModels(discovered);
      } catch {}
      const roles = chooseModelRoles(discovered, preset);
      const finalPatch = { default_model: roles.default_model, fast_model: roles.fast_model, reasoning_model: roles.reasoning_model, structured_output_model: roles.structured_output_model, max_tokens: roles.max_tokens };
      const persistedSettings = { ...settings, model_provider: { ...settings.model_provider, ...finalPatch, api_key: "" } };
      await updateModelConfig(configFromCurrent(finalPatch));
      replaceAndMarkSaved(persistedSettings);
      markModelProviderConfigured();
      const finalModel = finalPatch.default_model || m.default_model || "";
      setTestResult("ok");
      setTestMsg(finalModel ? `${finalModel} | ${r.latency_ms || ""}ms` : t("settings.saved_connected_ready"));
    } catch (e: unknown) {
      setTestResult("fail");
      setTestMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <SettingsSection title={t("settings.model_provider")}>
      <SettingRow label={t("settings.provider")} htmlFor="provider-select"><SelectField id="provider-select" value={selectedProvider} options={providerOptions()} onChange={changeProvider} /></SettingRow>
      <SettingRow label={t("settings.api_key")} desc={t("settings.api_key_warning")} htmlFor="provider-api-key">
        <div className="flex flex-col gap-1">
          <PasswordField id="provider-api-key" value={m.api_key} onChange={(v) => update("model_provider", { api_key: v })} />
          <a href={preset.apiKeyHelpUrl || "https://platform.openai.com/api-keys"} target="_blank" rel="noreferrer" className="text-xs" style={{ color: "var(--accent)" }}>
            {t("settings.api_key_help")}
          </a>
        </div>
      </SettingRow>
      <SettingRow label={t("settings.connection")}>
        <div className="flex items-center gap-2 flex-wrap">
          <button onClick={handleSaveAndTestConnection} className="px-3 py-1.5 text-xs rounded-md border" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("settings.test_discover")}</button>
          {testResult === "ok" && <span className="text-xs" style={{ color: "var(--green)" }}>{t("settings.saved_connected")}: {testMsg}</span>}
          {testResult === "fail" && <span className="text-xs" style={{ color: "var(--red)" }}>{t("settings.connection_failed")}: {testMsg}</span>}
          {testResult === "ok" && <button onClick={() => navigate("/")} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("settings.continue_to_chat")}</button>}
        </div>
      </SettingRow>
      <SettingRow label={t("settings.advanced_toggle")}><button type="button" onClick={() => setAdvanced((v) => !v)} className="px-3 py-1.5 text-xs rounded-md border">{advanced ? t("settings.hide_advanced") : t("settings.advanced_toggle")}</button></SettingRow>
      {advanced && (
        <>
          {/* Model-role selection is auto-filled by Test/Discover; only power users who
              want to override which model fills each role need these, so they live here. */}
          <SettingRow label={t("settings.default_model")} htmlFor="provider-default-model"><SelectField id="provider-default-model" value={defaultModel} options={modelOptions} onChange={(v) => update("model_provider", { default_model: v })} /></SettingRow>
          <SettingRow label={t("settings.fast_model")} htmlFor="provider-fast-model"><SelectField id="provider-fast-model" value={fastModel} options={modelOptions} onChange={(v) => update("model_provider", { fast_model: v })} /></SettingRow>
          <SettingRow label={t("settings.reasoning_model")} htmlFor="provider-reasoning-model"><SelectField id="provider-reasoning-model" value={reasoningModel} options={modelOptions} onChange={(v) => update("model_provider", { reasoning_model: v })} /></SettingRow>
          <SettingRow label={t("settings.structured_model")} htmlFor="provider-structured-model"><SelectField id="provider-structured-model" value={structuredModel} options={modelOptions} onChange={(v) => update("model_provider", { structured_output_model: v })} /></SettingRow>
          <SettingRow label={t("settings.base_url")} htmlFor="provider-base-url"><TextField id="provider-base-url" monospace value={m.base_url || preset.baseUrl} onChange={(v) => update("model_provider", { base_url: v })} /></SettingRow>
          <SettingRow label={t("settings.max_tokens")} htmlFor="provider-max-tokens"><NumberField id="provider-max-tokens" value={m.max_tokens || preset.maxTokens} onChange={(v) => update("model_provider", { max_tokens: v })} min={1} max={1000000} step={1024} /></SettingRow>
          <SettingRow label={t("settings.max_cost_per_task")}><NumberField value={m.max_cost_per_task_usd} onChange={(v) => update("model_provider", { max_cost_per_task_usd: v })} min={0} max={100} step={0.1} /></SettingRow>
        </>
      )}
    </SettingsSection>
  );
}

type McpToolRow = {
  name?: string;
  description?: string;
  urn?: string;
};

function parseMcpTools(raw: unknown): McpToolRow[] {
  if (Array.isArray(raw)) return raw as McpToolRow[];
  if (typeof raw === "string") {
    try {
      return parseMcpTools(JSON.parse(raw));
    } catch {
      return [];
    }
  }
  if (raw && typeof raw === "object" && Array.isArray((raw as { tools?: unknown[] }).tools)) {
    return (raw as { tools: McpToolRow[] }).tools;
  }
  return [];
}

function McpServersPanel() {
  useLanguage();
  const toast = useToast();
  const [servers, setServers] = useState<McpServerRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [testingId, setTestingId] = useState("");
  const [connectingId, setConnectingId] = useState("");
  const [disconnectingId, setDisconnectingId] = useState("");
  const [toolsByServer, setToolsByServer] = useState<Record<string, McpToolRow[]>>({});
  const [form, setForm] = useState<McpServerUpsertRequest>({
    id: "",
    name: "",
    transport: "stdio",
    command: "",
    args_json: "[]",
    env_json: "{}",
    url: "",
    headers_json: "{}",
    enabled: true,
  });

  async function load() {
    setLoading(true);
    try {
      const rows = await listMcpServers();
      const normalizedRows = Array.isArray(rows) ? rows : [];
      setServers(normalizedRows);
      setToolsByServer((prev) => {
        const next = { ...prev };
        for (const row of normalizedRows) {
          const cachedTools = parseMcpTools(row.tools_json);
          if (cachedTools.length > 0) {
            next[row.id] = cachedTools;
          }
        }
        return next;
      });
    } catch (error: unknown) {
      setServers([]);
      toast.error(String(error instanceof Error ? error.message : error));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
  }, []);

  function resetForm() {
    setForm({
      id: "",
      name: "",
      transport: "stdio",
      command: "",
      args_json: "[]",
      env_json: "{}",
      url: "",
      headers_json: "{}",
      enabled: true,
    });
  }

  function fillFromServer(server: McpServerRecord) {
    setForm({
      id: server.id,
      name: server.name,
      transport: server.transport,
      command: server.command || "",
      args_json: server.args_json || "[]",
      env_json: server.env_json || "{}",
      url: server.url || "",
      headers_json: server.headers_json || "{}",
      enabled: server.enabled,
    });
  }

  async function saveServer() {
    if (!form.id.trim() || !form.name.trim()) return;
    const payload: McpServerUpsertRequest = {
      ...form,
      id: form.id.trim(),
      name: form.name.trim(),
      command: form.transport === "stdio" ? String(form.command || "").trim() : null,
      url: form.transport === "http" ? String(form.url || "").trim() : null,
      args_json: form.args_json || "[]",
      env_json: form.env_json || "{}",
      headers_json: form.headers_json || "{}",
      enabled: Boolean(form.enabled),
    };
    try {
      const exists = servers.some((server) => server.id === payload.id);
      if (exists) await updateMcpServer(payload.id, payload);
      else await createMcpServer(payload);
      await load();
      resetForm();
      toast.success(t("settings.mcp_saved"));
    } catch (error: unknown) {
      toast.error(
        `${t("settings.mcp_save_failed")}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  async function runTest() {
    if (!form.id.trim() || !form.name.trim()) return;
    setTestingId(form.id);
    try {
      await testMcpServer({
        ...form,
        id: form.id.trim(),
        name: form.name.trim(),
      });
      toast.success(t("settings.mcp_test_ok"));
    } catch (error: unknown) {
      toast.error(
        `${t("settings.mcp_test_failed")}: ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setTestingId("");
    }
  }

  async function removeServer(id: string) {
    try {
      await deleteMcpServer(id);
      setToolsByServer((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      await load();
      if (form.id === id) resetForm();
      toast.info(t("settings.mcp_deleted"));
    } catch (error: unknown) {
      toast.error(
        `${t("settings.mcp_delete_failed")}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  async function connectAndLoadTools(server: McpServerRecord) {
    setConnectingId(server.id);
    try {
      await connectMcpServer(server.id);
      const tools = parseMcpTools(await listMcpServerTools(server.id));
      setToolsByServer((prev) => ({ ...prev, [server.id]: tools }));
      await load();
      toast.success(t("settings.mcp_connect_ok"));
    } catch (error: unknown) {
      toast.error(
        `${t("settings.mcp_connect_failed")}: ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setConnectingId("");
    }
  }

  async function disconnectServer(server: McpServerRecord) {
    setDisconnectingId(server.id);
    try {
      await disconnectMcpServer(server.id);
      await load();
      toast.info(t("settings.mcp_disconnect_ok"));
    } catch (error: unknown) {
      toast.error(
        `${t("settings.mcp_disconnect_failed")}: ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setDisconnectingId("");
    }
  }

  return (
    <SettingsSection title={t("settings.mcp_servers")} desc={t("settings.mcp_servers_desc")}>
      <SettingRow label={t("settings.mcp_id")} htmlFor="mcp-id">
        <TextField
          id="mcp-id"
          monospace
          value={form.id}
          onChange={(value) => setForm((prev) => ({ ...prev, id: value }))}
        />
      </SettingRow>
      <SettingRow label={t("settings.mcp_name")} htmlFor="mcp-name">
        <TextField
          id="mcp-name"
          value={form.name}
          onChange={(value) => setForm((prev) => ({ ...prev, name: value }))}
        />
      </SettingRow>
      <SettingRow label={t("settings.mcp_transport")} htmlFor="mcp-transport">
        <SelectField
          id="mcp-transport"
          value={form.transport}
          options={[
            { value: "stdio", label: "stdio" },
            { value: "http", label: "http" },
          ]}
          onChange={(value) => setForm((prev) => ({ ...prev, transport: value }))}
        />
      </SettingRow>
      {form.transport === "stdio" ? (
        <>
          <SettingRow label={t("settings.mcp_command")} htmlFor="mcp-command">
            <TextField
              id="mcp-command"
              monospace
              value={form.command || ""}
              onChange={(value) => setForm((prev) => ({ ...prev, command: value }))}
            />
          </SettingRow>
          <SettingRow label={t("settings.mcp_args_json")} htmlFor="mcp-args">
            <TextField
              id="mcp-args"
              monospace
              value={form.args_json || "[]"}
              onChange={(value) => setForm((prev) => ({ ...prev, args_json: value }))}
            />
          </SettingRow>
          <SettingRow label={t("settings.mcp_env_json")} htmlFor="mcp-env">
            <TextField
              id="mcp-env"
              monospace
              value={form.env_json || "{}"}
              onChange={(value) => setForm((prev) => ({ ...prev, env_json: value }))}
            />
          </SettingRow>
        </>
      ) : (
        <>
          <SettingRow label={t("settings.mcp_url")} htmlFor="mcp-url">
            <TextField
              id="mcp-url"
              monospace
              value={form.url || ""}
              onChange={(value) => setForm((prev) => ({ ...prev, url: value }))}
            />
          </SettingRow>
          <SettingRow label={t("settings.mcp_headers_json")} htmlFor="mcp-headers">
            <TextField
              id="mcp-headers"
              monospace
              value={form.headers_json || "{}"}
              onChange={(value) => setForm((prev) => ({ ...prev, headers_json: value }))}
            />
          </SettingRow>
        </>
      )}
      <SettingRow label={t("settings.mcp_enabled")}>
        <ToggleField
          checked={Boolean(form.enabled)}
          onChange={(value) => setForm((prev) => ({ ...prev, enabled: value }))}
        />
      </SettingRow>
      <SettingRow label={t("settings.connection")}>
        <div className="flex items-center gap-2 flex-wrap">
          <button
            type="button"
            onClick={saveServer}
            className="px-3 py-1.5 text-xs rounded-md border"
            style={{ borderColor: "var(--accent)", color: "var(--accent)" }}
          >
            {t("settings.mcp_save")}
          </button>
          <button
            type="button"
            onClick={runTest}
            disabled={testingId === form.id}
            className="px-3 py-1.5 text-xs rounded-md border"
            style={{ borderColor: "var(--border-accent)", color: "var(--text-secondary)" }}
          >
            {testingId === form.id ? t("settings.loading") : t("settings.mcp_test")}
          </button>
          <button
            type="button"
            onClick={resetForm}
            className="px-3 py-1.5 text-xs rounded-md border"
            style={{ borderColor: "var(--border-subtle)", color: "var(--text-muted)" }}
          >
            {t("settings.reset")}
          </button>
        </div>
      </SettingRow>
      <div className="border-t px-4 py-3" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-primary)" }}>
          {t("settings.mcp_saved_servers")}
        </div>
        {loading ? (
          <div className="text-xs" style={{ color: "var(--text-muted)" }}>
            {t("settings.loading")}
          </div>
        ) : servers.length === 0 ? (
          <div className="text-xs" style={{ color: "var(--text-muted)" }}>
            {t("settings.mcp_empty")}
          </div>
        ) : (
          <div className="space-y-3">
            {servers.map((server) => (
              <div
                key={server.id}
                className="rounded-md border p-3"
                style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-semibold">{server.name}</div>
                    <div className="text-xs font-mono" style={{ color: "var(--text-muted)" }}>
                      {server.id}
                    </div>
                    <div className="mt-1 text-xs" style={{ color: "var(--text-secondary)" }}>
                      {server.transport}
                      {server.transport === "stdio"
                        ? server.command
                          ? ` · ${server.command}`
                          : ""
                        : server.url
                          ? ` · ${server.url}`
                          : ""}
                    </div>
                    <div className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>
                      {server.status || "unknown"}
                      {server.last_error ? ` · ${server.last_error}` : ""}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 flex-wrap justify-end">
                    <button
                      type="button"
                      onClick={() => fillFromServer(server)}
                      className="px-2 py-1 text-xs rounded-md border"
                      style={{ borderColor: "var(--border-accent)", color: "var(--text-secondary)" }}
                    >
                      {t("settings.mcp_edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        server.status === "connected"
                          ? void disconnectServer(server)
                          : void connectAndLoadTools(server)
                      }
                      disabled={connectingId === server.id || disconnectingId === server.id}
                      className="px-2 py-1 text-xs rounded-md border"
                      style={{ borderColor: "var(--accent)", color: "var(--accent)" }}
                    >
                      {connectingId === server.id || disconnectingId === server.id
                        ? t("settings.loading")
                        : server.status === "connected"
                          ? t("settings.mcp_disconnect")
                          : t("settings.mcp_connect")}
                    </button>
                    <button
                      type="button"
                      onClick={() => void removeServer(server.id)}
                      className="px-2 py-1 text-xs rounded-md border"
                      style={{ borderColor: "var(--red)", color: "var(--red)" }}
                    >
                      {t("settings.mcp_delete")}
                    </button>
                  </div>
                </div>
                {toolsByServer[server.id]?.length ? (
                  <div className="mt-3 rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>
                    <div className="mb-2 text-[11px] font-semibold" style={{ color: "var(--text-primary)" }}>
                      {t("settings.mcp_tools")}
                    </div>
                    <div className="space-y-2">
                      {toolsByServer[server.id].map((tool, index) => (
                        <div key={`${server.id}-tool-${index}`} className="text-xs">
                          <div className="font-semibold">{tool.name || t("settings.mcp_tool_unnamed")}</div>
                          {tool.urn && (
                            <div className="font-mono" style={{ color: "var(--accent)" }}>
                              {tool.urn}
                            </div>
                          )}
                          {tool.description && (
                            <div style={{ color: "var(--text-muted)" }}>{tool.description}</div>
                          )}
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>
    </SettingsSection>
  );
}

function AgentRuntimePanel() {
  const { settings, update } = useSettings();
  const a = settings.agent_runtime;
  return (
    <SettingsSection title={t("settings.agent_runtime")} desc={t("settings.agent_runtime_desc")}>
      <SettingRow label={t("settings.max_agents_per_task")} desc={t("settings.max_agents_per_task_desc")}>
        <NumberField value={a.max_agents_per_task} min={1} max={20} step={1} onChange={(v) => update("agent_runtime", { max_agents_per_task: v })} />
      </SettingRow>
      <SettingRow label={t("settings.allow_task_agent_instance")} desc={t("settings.allow_task_agent_instance_desc")}>
        <ToggleField checked={a.allow_task_agent_instance} onChange={(v) => update("agent_runtime", { allow_task_agent_instance: v })} />
      </SettingRow>
      <SettingRow label={t("settings.allow_ephemeral_sub_agent")} desc={t("settings.allow_ephemeral_sub_agent_desc")}>
        <ToggleField checked={a.allow_ephemeral_sub_agent} onChange={(v) => update("agent_runtime", { allow_ephemeral_sub_agent: v })} />
      </SettingRow>
      <SettingRow label={t("settings.default_tool_scope")}>
        <TextField monospace value={a.default_tool_scope} onChange={(v) => update("agent_runtime", { default_tool_scope: v })} />
      </SettingRow>
    </SettingsSection>
  );
}

function GovernancePanel() {
  const { settings, update } = useSettings();
  const g = settings.governance;
  return (
    <SettingsSection title={t("settings.governance")} desc={t("settings.governance_desc")}>
      <SettingRow label={t("settings.auto_confirm_green_track")} desc={t("settings.auto_confirm_green_track_desc")}>
        <ToggleField checked={g.auto_confirm_green_track} onChange={(v) => update("governance", { auto_confirm_green_track: v })} />
      </SettingRow>
      <SettingRow label={t("settings.yellow_approval_mode")}>
        <SelectField value={g.yellow_approval_mode} options={[{ value: "negative_consent", label: t("settings.negative_consent") }, { value: "explicit_approval", label: t("settings.explicit_approval") }]} onChange={(v) => update("governance", { yellow_approval_mode: v as never })} />
      </SettingRow>
      <SettingRow label={t("settings.negative_consent_timeout")}>
        <NumberField value={g.negative_consent_timeout_seconds} min={30} max={86400} step={30} onChange={(v) => update("governance", { negative_consent_timeout_seconds: v })} />
      </SettingRow>
      <SettingRow label={t("settings.responsibility_anchor_required")}>
        <ToggleField checked={g.responsibility_anchor_required} onChange={(v) => update("governance", { responsibility_anchor_required: v })} />
      </SettingRow>
    </SettingsSection>
  );
}

function RiskGatePanel() {
  const { settings, update } = useSettings();
  const r = settings.risk_gate;
  return (
    <SettingsSection title={t("settings.risk_gate")} desc={t("settings.risk_gate_desc")}>
      <SettingRow label={t("settings.green_threshold")}>
        <NumberField value={r.green_threshold} min={0} max={1} step={0.05} onChange={(v) => update("risk_gate", { green_threshold: v })} />
      </SettingRow>
      <SettingRow label={t("settings.yellow_threshold")}>
        <NumberField value={r.yellow_threshold} min={0} max={1} step={0.05} onChange={(v) => update("risk_gate", { yellow_threshold: v })} />
      </SettingRow>
      <SettingRow label={t("settings.red_threshold")}>
        <NumberField value={r.red_threshold} min={0} max={1} step={0.05} onChange={(v) => update("risk_gate", { red_threshold: v })} />
      </SettingRow>
      <SettingRow label={t("settings.emergency_lease_enabled")}>
        <ToggleField checked={r.emergency_lease_enabled} onChange={(v) => update("risk_gate", { emergency_lease_enabled: v })} />
      </SettingRow>
    </SettingsSection>
  );
}

function CognitiveCustomsPanel() {
  const { settings, update } = useSettings();
  const c = settings.cognitive_customs;
  return (
    <SettingsSection title={t("settings.cognitive_customs")} desc={t("settings.cognitive_customs_desc")}>
      <SettingRow label={t("settings.default_fact_ttl")}>
        <NumberField value={c.default_fact_ttl_seconds} min={60} max={31536000} step={60} onChange={(v) => update("cognitive_customs", { default_fact_ttl_seconds: v })} />
      </SettingRow>
      <SettingRow label={t("settings.require_provenance_for_fact")}>
        <ToggleField checked={c.require_provenance_for_fact} onChange={(v) => update("cognitive_customs", { require_provenance_for_fact: v })} />
      </SettingRow>
      <SettingRow label={t("settings.allow_hypothesis_auto_promotion")}>
        <ToggleField checked={c.allow_hypothesis_auto_promotion} onChange={(v) => update("cognitive_customs", { allow_hypothesis_auto_promotion: v })} />
      </SettingRow>
      <SettingRow label={t("settings.replay_mode_blocks_fact_write")}>
        <ToggleField checked={c.replay_mode_blocks_fact_write} onChange={(v) => update("cognitive_customs", { replay_mode_blocks_fact_write: v })} />
      </SettingRow>
    </SettingsSection>
  );
}
function PolicyEnginePanel() { const { settings, update } = useSettings(); const p = settings.policy_engine; const selectedPolicyEngine: PolicyEngineType = p.policy_engine === "custom" ? "custom" : "opa"; return <SettingsSection title={t("settings.policy_engine")}><SettingRow label={t("settings.policy_engine_choice")}><SelectField value={selectedPolicyEngine} options={[{ value: "opa", label: "OPA" }, { value: "custom", label: t("settings.policy_custom") }]} onChange={(v) => update("policy_engine", { policy_engine: v as PolicyEngineType })} /></SettingRow></SettingsSection>; }
function PrivacyPanel() {
  const { settings, update } = useSettings();
  const p = settings.privacy;
  return (
    <SettingsSection title={t("settings.privacy")} desc={t("settings.privacy_desc")}>
      <SettingRow label={t("settings.log_retention_days")}>
        <NumberField value={p.log_retention_days} min={1} max={3650} step={1} onChange={(v) => update("privacy", { log_retention_days: v })} />
      </SettingRow>
      <SettingRow label={t("settings.store_full_prompts")}>
        <ToggleField checked={p.store_full_prompts} onChange={(v) => update("privacy", { store_full_prompts: v })} />
      </SettingRow>
      <SettingRow label={t("settings.store_model_outputs")}>
        <ToggleField checked={p.store_model_outputs} onChange={(v) => update("privacy", { store_model_outputs: v })} />
      </SettingRow>
      <SettingRow label={t("settings.pii_redaction_enabled")}>
        <ToggleField checked={p.pii_redaction_enabled} onChange={(v) => update("privacy", { pii_redaction_enabled: v })} />
      </SettingRow>
    </SettingsSection>
  );
}
function DeveloperPanel() {
  const { settings, update } = useSettings();
  const d = settings.developer;
  const [resetDone, setResetDone] = useState(false);
  function resetLocalUiState() {
    const apiBase = localStorage.getItem("coevo-api-base");
    ["coevo-settings", "coevo-theme", "coevo-font-size", "coevo-density"].forEach((key) => localStorage.removeItem(key));
    if (apiBase) localStorage.setItem("coevo-api-base", apiBase);
    setResetDone(true);
  }
  return (
    <SettingsSection title={t("settings.developer")} desc={t("settings.developer_desc")}>
      <SettingRow label={t("settings.api_base")}><TextField monospace value={d.api_base_url} onChange={(v) => update("developer", { api_base_url: v })} /></SettingRow>
      <SettingRow label={t("settings.openapi_url")}><TextField monospace value={d.openapi_url} onChange={(v) => update("developer", { openapi_url: v })} /></SettingRow>
      <SettingRow label={t("settings.debug_logs_enabled")}><ToggleField checked={d.debug_logs_enabled} onChange={(v) => update("developer", { debug_logs_enabled: v })} /></SettingRow>
      <SettingRow label={t("settings.show_raw_json_panels")}><ToggleField checked={d.show_raw_json_panels} onChange={(v) => update("developer", { show_raw_json_panels: v })} /></SettingRow>
      <SettingRow label={t("settings.reset_local_ui")}>
        <div className="flex items-center gap-2">
          <button type="button" onClick={resetLocalUiState} className="px-3 py-1.5 text-xs rounded-md border">{t("settings.reset_button")}</button>
          {resetDone && <span className="text-xs" style={{ color: "var(--green)" }}>{t("settings.local_ui_reset_done")}</span>}
        </div>
      </SettingRow>
    </SettingsSection>
  );
}

function DataManagementPanel() {
  useLanguage();
  const [coevoHome, setCoevoHome] = useState<string>(t("settings.loading"));
  useEffect(() => {
    (async () => {
      try {
        const invoke = getTauriInvoke();
        if (invoke) setCoevoHome(await invoke<string>("get_coevo_home"));
        else setCoevoHome(t("settings.web_mode_home"));
      } catch { setCoevoHome("~/.coevo"); }
    })();
  }, []);
  return (
    <SettingsSection title={t("settings.data_management")}>
      <SettingRow label="COEVO_HOME"><div className="text-xs font-mono">{coevoHome}</div></SettingRow>
      <SettingRow label={t("settings.api_base")}><div className="text-xs font-mono">{getApiBase()}</div></SettingRow>
    </SettingsSection>
  );
}

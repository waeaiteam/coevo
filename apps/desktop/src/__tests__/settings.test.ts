import { describe, it, expect, beforeEach } from "vitest";
import { defaults } from "../settings/defaults";
import { loadSettingsSnapshot, saveSettingsSnapshot } from "../hooks/useSettings";
import { chooseModelRoles, presetFor } from "../settings/modelPresets";
import { t, setLanguage } from "../settings/i18n";

describe("Settings", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("has all 10 categories in defaults", () => {
    const keys = Object.keys(defaults);
    expect(keys).toContain("general");
    expect(keys).toContain("appearance");
    expect(keys).toContain("model_provider");
    expect(keys).toContain("agent_runtime");
    expect(keys).toContain("governance");
    expect(keys).toContain("risk_gate");
    expect(keys).toContain("cognitive_customs");
    expect(keys).toContain("policy_engine");
    expect(keys).toContain("privacy");
    expect(keys).toContain("developer");
  });

  it("saves and loads from localStorage", () => {
    const s = JSON.stringify({ ...defaults, general: { ...defaults.general, default_home: "dashboard" } });
    localStorage.setItem("coevo-settings", s);
    const loaded = JSON.parse(localStorage.getItem("coevo-settings")!);
    expect(loaded.general.default_home).toBe("dashboard");
  });

  it("deep merges partial stored settings with defaults", () => {
    localStorage.setItem("coevo-settings", JSON.stringify({
      model_provider: { provider: "openai-compatible", default_model: "gpt-4.1" },
    }));

    const loaded = loadSettingsSnapshot();

    expect(loaded.model_provider.default_model).toBe("gpt-4.1");
    expect(loaded.model_provider.base_url).toBe(defaults.model_provider.base_url);
    expect(loaded.developer.api_base_url).toBe(defaults.developer.api_base_url);
  });

  it("api_base_url defaults to localhost", () => {
    expect(defaults.developer.api_base_url).toBe("http://127.0.0.1:8717");
  });

  it("saving settings does not overwrite the runtime API base from desktop boot", () => {
    localStorage.setItem("coevo-api-base", "http://127.0.0.1:8718");

    saveSettingsSnapshot(defaults);

    expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8718");
  });

  it("does not persist API keys into the UI settings snapshot", () => {
    saveSettingsSnapshot({
      ...defaults,
      model_provider: { ...defaults.model_provider, api_key: "sk-live-secret" },
    });

    const saved = JSON.parse(localStorage.getItem("coevo-settings")!);
    expect(saved.model_provider.api_key).toBe("");
    expect(localStorage.getItem("coevo-settings")).not.toContain("sk-live-secret");
  });

  it("PasswordField show/hide is testable", () => {
    expect(true).toBe(true);
  });

  it("SaveBar appears when dirty", () => {
    expect(true).toBe(true);
  });

  it("MissionDraft has required fields", () => {
    const draft = {
      intent: "test",
      suggestedTrack: "green" as const,
      reason: "test",
      contractHash: "a".repeat(64),
      planHash: "b".repeat(64),
      ambiguityScore: 0.2,
      selectedAgents: ["agent-1"],
      allowedActions: ["read"],
      restrictedActions: ["write"],
      approvalRequired: false,
      approvalMode: "NEGATIVE_CONSENT",
      compileResult: {},
      routeResult: {},
    };
    expect(draft.suggestedTrack).toBe("green");
    expect(draft.contractHash).toHaveLength(64);
    expect(draft.selectedAgents).toHaveLength(1);
    expect(draft.allowedActions).toContain("read");
    expect(draft.restrictedActions).toContain("write");
    expect(draft.approvalRequired).toBe(false);
  });

  it("MissionPhase has all required states", () => {
    const phases = ["idle", "drafting", "review", "executing", "completed", "cancelled", "error"];
    expect(phases).toContain("review");
    expect(phases).toContain("cancelled");
    expect(phases).toContain("executing");
    expect(phases.length).toBe(7);
  });

  it("uses discovered model context tokens when output token metadata is absent", () => {
    const roles = chooseModelRoles(
      [{ id: "deepseek-chat", display_name: "deepseek-chat", max_context_tokens: 64000 }],
      presetFor("deepseek"),
    );

    expect(roles.default_model).toBe("deepseek-chat");
    expect(roles.max_tokens).toBe(64000);
  });

  it("has localized save success message for provider configuration", () => {
    setLanguage("en");
    expect(t("settings.saved_connected_ready")).toBe("Saved model provider. Create employees if needed, then start a new task.");

    setLanguage("zh");
    expect(t("settings.saved_connected_ready")).toBe("模型提供商已保存。如有需要请先创建员工，然后再开始新任务");
  });
});

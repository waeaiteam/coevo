import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { getUserProfile, updateUserProfile } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

export default function FounderProfile() {
  useLanguage();
  const [profile, setProfile] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [form, setForm] = useState<Record<string, unknown>>({});

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    setLoading(true);
    try {
      const next = await getUserProfile();
      setProfile(next);
      setForm(next);
    } catch {
      setProfile(null);
    }
    setLoading(false);
  }

  async function save() {
    setError("");
    setSaved(false);
    try {
      await updateUserProfile(form);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      await load();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const field = (key: string) => form[key] as string || "";
  const setField = (key: string, value: unknown) => setForm({ ...form, [key]: value });

  if (loading) return <div className="p-5 text-sm" style={{ color: "var(--text-muted)" }}>{t("founder.loading")}</div>;
  if (!profile) {
    return (
      <div className="p-5 space-y-3">
        <div className="text-sm" style={{ color: "var(--text-muted)" }}>{t("founder.uninitialized")}</div>
        <button onClick={save} className="px-4 py-2 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("founder.create_default")}</button>
      </div>
    );
  }

  return (
    <div className="space-y-5 max-w-2xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg font-semibold" style={{ color: "var(--accent)" }}>F</span>
          <h2 className="text-lg font-bold">{t("founder.title")}</h2>
        </div>
        {saved && <span className="text-xs" style={{ color: "var(--green)" }}>{t("founder.saved")}</span>}
      </div>
      {error && <div className="text-xs p-2 rounded" style={{ color: "var(--red)", background: "var(--red-dim)" }}>{error}</div>}
      <div className="card space-y-3 text-sm">
        <Row label={t("founder.display_name")}><input value={field("display_name")} onChange={(event) => setField("display_name", event.target.value)} className="input" /></Row>
        <Row label={t("founder.language")}><input value={field("preferred_language")} onChange={(event) => setField("preferred_language", event.target.value)} className="input" /></Row>
        <Row label={t("founder.timezone")}><input value={field("timezone")} onChange={(event) => setField("timezone", event.target.value)} className="input" /></Row>
        <Row label={t("founder.communication_style")}><input value={field("communication_style")} onChange={(event) => setField("communication_style", event.target.value)} className="input" /></Row>
        <Row label={t("founder.risk_preference")}><input value={field("risk_preference")} onChange={(event) => setField("risk_preference", event.target.value)} className="input" /></Row>
        <Row label={t("founder.default_mission_mode")}><input value={field("default_mission_mode")} onChange={(event) => setField("default_mission_mode", event.target.value)} className="input" /></Row>
        <Row label={t("founder.long_term_goals")}><textarea value={field("long_term_goals")} onChange={(event) => setField("long_term_goals", event.target.value)} className="input" rows={3} /></Row>
        <Row label={t("founder.business_domains")}><input value={field("business_domains")} onChange={(event) => setField("business_domains", event.target.value)} className="input" /></Row>
        <Row label={t("founder.budget")}>
          <input
            value={field("budget_limits") ? JSON.stringify(form.budget_limits) : ""}
            onChange={(event) => {
              try { setField("budget_limits", JSON.parse(event.target.value)); } catch { /* keep previous valid JSON */ }
            }}
            className="input font-mono text-xs"
          />
        </Row>
        <button onClick={save} className="px-4 py-2 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("founder.save")}</button>
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return <div className="flex flex-col gap-1"><span className="text-xs" style={{ color: "var(--text-muted)" }}>{label}</span>{children}</div>;
}

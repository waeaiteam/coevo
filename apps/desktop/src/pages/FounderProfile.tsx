import { useState, useEffect } from "react";
import { getUserProfile, updateUserProfile } from "../api/client";

export default function FounderProfile() {
  const [profile, setProfile] = useState<Record<string,unknown>|null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [form, setForm] = useState<Record<string,unknown>>({});

  useEffect(() => { load(); }, []);
  async function load() {
    setLoading(true);
    try { const p = await getUserProfile(); setProfile(p); setForm(p); } catch { setProfile(null); }
    setLoading(false);
  }
  async function save() {
    setError(""); setSaved(false);
    try { await updateUserProfile(form); setSaved(true); setTimeout(()=>setSaved(false),2000); load(); }
    catch(e:unknown){ setError(e instanceof Error?e.message:String(e)); }
  }
  const f = (k:string)=>form[k] as string||"";
  const sf = (k:string,v:string)=>setForm({...form,[k]:v});

  if (loading) return <div className="p-5 text-sm" style={{color:"var(--text-muted)"}}>Loading...</div>;
  if (!profile) return (
    <div className="p-5 space-y-3">
      <div className="text-sm" style={{color:"var(--text-muted)"}}>Founder Profile not initialized</div>
      <button onClick={save} className="px-4 py-2 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>Create Default Founder Profile</button>
    </div>
  );

  return (
    <div className="space-y-5 max-w-2xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><span className="text-lg" style={{color:"var(--accent)"}}>◈</span><h2 className="text-lg font-bold">Founder Profile</h2></div>
        {saved && <span className="text-xs" style={{color:"var(--green)"}}>✓ Saved</span>}
      </div>
      {error && <div className="text-xs p-2 rounded" style={{color:"var(--red)",background:"var(--red-dim)"}}>{error}</div>}
      <div className="card space-y-3 text-sm">
        <Row label="Display Name"><input value={f("display_name")} onChange={e=>sf("display_name",e.target.value)} className="input" /></Row>
        <Row label="Language"><input value={f("preferred_language")} onChange={e=>sf("preferred_language",e.target.value)} className="input" /></Row>
        <Row label="Timezone"><input value={f("timezone")} onChange={e=>sf("timezone",e.target.value)} className="input" /></Row>
        <Row label="Communication Style"><input value={f("communication_style")} onChange={e=>sf("communication_style",e.target.value)} className="input" /></Row>
        <Row label="Risk Preference"><input value={f("risk_preference")} onChange={e=>sf("risk_preference",e.target.value)} className="input" /></Row>
        <Row label="Default Mission Mode"><input value={f("default_mission_mode")} onChange={e=>sf("default_mission_mode",e.target.value)} className="input" /></Row>
        <Row label="Long-term Goals"><textarea value={f("long_term_goals")} onChange={e=>sf("long_term_goals",e.target.value)} className="input" rows={3} /></Row>
        <Row label="Business Domains"><input value={f("business_domains")} onChange={e=>sf("business_domains",e.target.value)} className="input" /></Row>
        <Row label="Budget (max/task USD)"><input value={f("budget_limits")?JSON.stringify(form["budget_limits"]):""} onChange={e=>{try{sf("budget_limits",JSON.parse(e.target.value))}catch{}}} className="input font-mono text-xs" /></Row>
        <button onClick={save} className="px-4 py-2 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>Save Profile</button>
      </div>
    </div>
  );
}
function Row({label,children}:{label:string;children:React.ReactNode}){return<div className="flex flex-col gap-1"><span className="text-xs" style={{color:"var(--text-muted)"}}>{label}</span>{children}</div>}

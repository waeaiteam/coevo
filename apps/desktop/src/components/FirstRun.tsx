import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { seedEmployees, registerExecutor, seedSkills, updateModelConfig } from "../api/client";

export default function FirstRun({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  async function quickStart() {
    setStep(1); setMsg("Initializing Mock Provider...");
    try { await updateModelConfig({ provider_id: "mock-default", kind: "Mock" }); } catch {}
    setStep(2); setMsg("Seeding AI Employees...");
    try { await seedEmployees(); } catch { setErr("Failed to seed employees"); return; }
    setStep(3); setMsg("Seeding Skills...");
    try { await seedSkills(); } catch {}
    setStep(4); setMsg("Registering Mock Executor...");
    try { await registerExecutor({ executor_id:"mock-openclaw","display_name":"Mock OpenClaw","source_type":"open_claw","runtime_endpoint":"","capabilities":[],"required_credentials":[],"permission_boundary":{"max_risk_score":0.5,"can_write_fact":false,"can_write_decision":false,"can_access_network":false,"can_access_filesystem":false,"can_call_external_executor":false,"can_propose_skill":false},"file_scope":[],"network_scope":[],"memory_scope":"executor","risk_ceiling":0.5,"supported_actions":["read"],"sandbox_level":"none","health_check_url":"","audit_callback_url":"","status":"registered","created_at_ms":Date.now(),"updated_at_ms":Date.now() }); } catch {}
    setStep(5); setMsg("Done!");
    setTimeout(onDone, 800);
  }

  return (
    <div className="flex flex-col items-center justify-center h-screen" style={{background:"var(--bg-primary)",color:"var(--text-primary)"}}>
      <div className="text-4xl mb-4" style={{color:"var(--accent)"}}>◈</div>
      <h1 className="text-2xl font-bold mb-2">Welcome to coevo</h1>
      <p className="text-sm mb-6" style={{color:"var(--text-secondary)"}}>Your one-person company AI operating system</p>
      <div className="space-y-3 w-80">
        <button onClick={quickStart} className="w-full py-3 text-sm rounded-md text-white font-semibold" style={{background:"var(--accent)"}}>Quick Start with Mock</button>
        <button onClick={() => navigate("/settings/model_provider")} className="w-full py-3 text-sm rounded-md border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}>Configure Real Model</button>
      </div>
      {step > 0 && (
        <div className="mt-6 text-sm text-center" style={{color:"var(--text-secondary)"}}>
          <div>Step {step}/5: {msg}</div>
          {err && <div className="mt-2" style={{color:"var(--red)"}}>{err}</div>}
        </div>
      )}
      <p className="text-xs mt-8" style={{color:"var(--text-muted)"}}>Mock mode uses local AI simulation. No API key needed.</p>
    </div>
  );
}

export default function FounderProfile() {
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg" style={{color:"var(--accent)"}}>◈</span><h2 className="text-lg font-bold">Founder Profile</h2></div>
      <div className="card space-y-3 text-sm">
        <div className="grid grid-cols-2 gap-3"><Field label="User ID" value="opc-founder-01"/><Field label="Display Name" value="OPC Founder"/></div>
        <div className="grid grid-cols-2 gap-3"><Field label="Language" value="zh"/><Field label="Timezone" value="Asia/Shanghai"/></div>
        <div className="grid grid-cols-2 gap-3"><Field label="Risk Preference" value="Balanced"/><Field label="Default Mission Mode" value="Auto"/></div>
        <Field label="Long-term Goals" value="Build a sustainable one-person company with AI governance"/>
        <Field label="Business Domains" value="AI infrastructure, developer tools, governance"/>
        <Field label="Budget" value="$50/task, $500/day"/>
        <div className="text-xs" style={{color:"var(--text-muted)"}}>Edit via Settings → General or backend API PUT /opc/profile/user</div>
      </div>
    </div>
  );
}
function Field({label,value}:{label:string;value:string}){return <div><span style={{color:"var(--text-muted)"}} className="text-xs">{label}</span><div className="text-sm font-mono mt-0.5" style={{color:"var(--text-primary)"}}>{value}</div></div>}

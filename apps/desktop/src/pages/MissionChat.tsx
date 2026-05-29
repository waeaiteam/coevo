import { useState } from "react";

type Msg = { role: "user" | "system"; text: string };

export default function MissionChat() {
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);

  function send() {
    const text = input.trim();
    if (!text) return;
    setInput("");
    setMessages((prev) => [
      ...prev,
      { role: "user", text },
      { role: "system", text: "Mission received. Review WorkOrders for the governed run details." },
    ]);
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-3 border-b" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">Mission Composer</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>Governed OPC mission entry</div>
      </div>
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {messages.length === 0 && (
          <div className="text-center pt-16">
            <div className="text-3xl mb-3" style={{ color: "var(--accent)" }}>◈</div>
            <h1 className="text-xl font-bold mb-1">你想让 coevo 治理什么任务？</h1>
            <p className="text-sm" style={{ color: "var(--text-muted)" }}>Enter a mission, then inspect the resulting work order trace.</p>
          </div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={m.role === "user" ? "text-right" : "text-left"}>
            <div className="chat-msg inline-block" style={{ background: m.role === "user" ? "#f0f0ff" : "#fff", border: "1px solid var(--border-subtle)" }}>
              <div className="text-xs mb-1" style={{ color: "var(--text-muted)" }}>{m.role === "user" ? "You" : "coevo"}</div>
              <div>{m.text}</div>
            </div>
          </div>
        ))}
      </div>
      <div className="px-5 py-3 border-t" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="flex gap-2 max-w-3xl mx-auto">
          <textarea className="flex-1 p-3 rounded-xl border text-sm resize-none" rows={2} value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }} />
          <button className="px-5 py-3 rounded-xl text-sm font-semibold" style={{ background: "var(--accent)", color: "#fff" }} onClick={send}>Send</button>
        </div>
      </div>
    </div>
  );
}

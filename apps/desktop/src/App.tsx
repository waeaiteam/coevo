import { useState } from "react";
import { Routes, Route } from "react-router-dom";
import { GovernanceProvider } from "./hooks/useGovernance";
import { getModelConfig } from "./api/client";
import { clearModelProviderConfigured, isModelProviderConfigured, markModelProviderConfigured } from "./settings/onboarding";
import BootPage from "./components/BootPage";
import FirstRun from "./components/FirstRun";
import Layout from "./components/Layout";
import MissionChat from "./pages/MissionChat";
import Dashboard from "./pages/Dashboard";
import MyCompany from "./pages/MyCompany";
import CompanyDetail from "./pages/CompanyDetail";
import Projects from "./pages/Projects";
import ProjectDetail from "./pages/ProjectDetail";
import TaskDetail from "./pages/TaskDetail";
import FounderProfile from "./pages/FounderProfile";
import CompanyMemory from "./pages/CompanyMemory";
import AIEmployees from "./pages/AIEmployees";
import SkillsPage from "./pages/SkillsPage";
import ExternalExecutors from "./pages/ExternalExecutors";
import WorkOrders from "./pages/WorkOrders";
import Contracts from "./pages/Contracts";
import Plans from "./pages/Plans";
import Customs from "./pages/Customs";
import RiskGate from "./pages/RiskGate";
import Resolution from "./pages/Resolution";
import Audit from "./pages/Audit";
import Settings from "./pages/Settings";
import Timeline from "./pages/Timeline";

export default function App() {
  const [booted, setBooted] = useState(false);
  const [showFirstRun, setShowFirstRun] = useState(false);

  async function handleBootReady() {
    let configured = false;
    try {
      const config = (await getModelConfig()) as Record<string, unknown>;
      configured = Boolean(config.has_api_key) && String(config.kind || "") !== "Mock";
    } catch {
      configured = false;
    }
    if (configured) {
      if (!isModelProviderConfigured()) markModelProviderConfigured();
    } else {
      clearModelProviderConfigured();
    }
    setShowFirstRun(!configured);
    setBooted(true);
  }

  if (!booted) return <BootPage onReady={handleBootReady} />;
  if (showFirstRun) return <FirstRun onDone={() => setShowFirstRun(false)} />;
  return (
    <GovernanceProvider>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<MissionChat />} />
          <Route path="/mission" element={<MissionChat />} />
          <Route path="/conversations/:conversationId" element={<MissionChat />} />
          <Route path="/company" element={<MyCompany />} />
          <Route path="/company/details" element={<CompanyDetail />} />
          <Route path="/projects" element={<Projects />} />
          <Route path="/projects/:projectId" element={<ProjectDetail />} />
          <Route path="/tasks/:workOrderId" element={<TaskDetail />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/founder" element={<FounderProfile />} />
          <Route path="/memory" element={<CompanyMemory />} />
          <Route path="/employees" element={<AIEmployees />} />
          <Route path="/skills" element={<SkillsPage />} />
          <Route path="/executors" element={<ExternalExecutors />} />
          <Route path="/work-orders" element={<WorkOrders />} />
          <Route path="/contracts" element={<Contracts />} />
          <Route path="/plans" element={<Plans />} />
          <Route path="/customs" element={<Customs />} />
          <Route path="/risk" element={<RiskGate />} />
          <Route path="/resolution" element={<Resolution />} />
          <Route path="/audit" element={<Audit />} />
          <Route path="/timeline" element={<Timeline />} />
          <Route path="/settings/*" element={<Settings />} />
        </Route>
      </Routes>
    </GovernanceProvider>
  );
}

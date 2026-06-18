import { useState } from "react";
import { Navigate, Routes, Route } from "react-router-dom";
import { ensureActiveCompany } from "./api/companies";
import { GovernanceProvider } from "./hooks/useGovernance";
import { ToastProvider } from "./components/ToastProvider";
import { getModelConfig } from "./api/client";
import { clearModelProviderConfigured, isModelProviderConfigured, markModelProviderConfigured } from "./settings/onboarding";
import { initTraceWiring } from "./stores/traceStore";
import BootPage from "./components/BootPage";
import FirstRun from "./components/FirstRun";
import Layout from "./components/Layout";
import MissionChat from "./pages/MissionChat";
import Dashboard from "./pages/Dashboard";
import MyCompany from "./pages/MyCompany";
import CompanyDetail from "./pages/CompanyDetail";
import Office from "./pages/Office";
import MeetingRoom from "./pages/MeetingRoom";
import PerformanceBoard from "./pages/PerformanceBoard";
import OperatingReports from "./pages/OperatingReports";
import CostManagement from "./pages/CostManagement";
import Projects from "./pages/Projects";
import ProjectDetail from "./pages/ProjectDetail";
import TaskDetail from "./pages/TaskDetail";
import FounderProfile from "./pages/FounderProfile";
import CompanyMemory from "./pages/CompanyMemory";
import AIEmployees from "./pages/AIEmployees";
import Team from "./pages/Team";
import TalentMarket from "./pages/TalentMarket";
import SkillsPage from "./pages/SkillsPage";
import ExternalExecutors from "./pages/ExternalExecutors";
import WorkOrders from "./pages/WorkOrders";
import Contracts from "./pages/Contracts";
import Customs from "./pages/Customs";
import Resolution from "./pages/Resolution";
import Audit from "./pages/Audit";
import Settings from "./pages/Settings";
import Timeline from "./pages/Timeline";
import Evaluations from "./pages/Evaluations";
import Traces from "./pages/Traces";
import Workflows from "./pages/Workflows";
import Performance from "./pages/Performance";
import EmployeeGrowth from "./pages/EmployeeGrowth";
import EmployeeOffice from "./pages/EmployeeOffice";

initTraceWiring();

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
      await ensureActiveCompany();
    } else {
      clearModelProviderConfigured();
    }
    setShowFirstRun(!configured);
    setBooted(true);
  }

  const content = !booted ? (
    <BootPage onReady={handleBootReady} />
  ) : showFirstRun ? (
    <FirstRun onDone={() => setShowFirstRun(false)} />
  ) : (
    <GovernanceProvider>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<MissionChat />} />
          <Route path="/mission" element={<MissionChat />} />
          <Route path="/conversations/:conversationId" element={<MissionChat />} />
          <Route path="/company" element={<MyCompany />} />
          <Route path="/company/details" element={<CompanyDetail />} />
          <Route path="/companies/:opcId" element={<CompanyDetail />} />
          <Route path="/companies/:opcId/office" element={<Office />} />
          <Route path="/companies/:opcId/meetings" element={<MeetingRoom />} />
          <Route path="/companies/:opcId/performance" element={<PerformanceBoard />} />
          <Route path="/companies/:opcId/reports" element={<OperatingReports />} />
          <Route path="/companies/:opcId/cost" element={<CostManagement />} />
          <Route path="/projects" element={<Projects />} />
          <Route path="/projects/:projectId" element={<ProjectDetail />} />
          <Route path="/tasks" element={<WorkOrders />} />
          <Route path="/tasks/:workOrderId" element={<TaskDetail />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/founder" element={<FounderProfile />} />
          <Route path="/memory" element={<CompanyMemory />} />
          <Route path="/team" element={<Team />} />
          <Route path="/employees" element={<AIEmployees />} />
          <Route path="/market" element={<TalentMarket />} />
          <Route path="/employees/:agentId/growth" element={<EmployeeGrowth />} />
          <Route path="/employees/:agentId" element={<EmployeeOffice />} />
          <Route path="/skills" element={<SkillsPage />} />
          <Route path="/executors" element={<ExternalExecutors />} />
          <Route path="/work-orders" element={<WorkOrders />} />
          <Route path="/contracts" element={<Contracts />} />
          <Route path="/plans" element={<Navigate to="/work-orders" replace />} />
          <Route path="/customs" element={<Customs />} />
          <Route path="/risk" element={<Navigate to="/settings/risk_gate" replace />} />
          <Route path="/resolution" element={<Resolution />} />
          <Route path="/audit" element={<Audit />} />
          <Route path="/timeline" element={<Timeline />} />
          <Route path="/evaluations" element={<Evaluations />} />
          <Route path="/traces" element={<Traces />} />
          <Route path="/workflows" element={<Workflows />} />
          <Route path="/performance" element={<Performance />} />
          <Route path="/settings/*" element={<Settings />} />
        </Route>
      </Routes>
    </GovernanceProvider>
  );

  return <ToastProvider>{content}</ToastProvider>;
}

import { Routes, Route } from "react-router-dom";
import { GovernanceProvider } from "./hooks/useGovernance";
import Layout from "./components/Layout";
import MissionChat from "./pages/MissionChat";
import Dashboard from "./pages/Dashboard";
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
import Demos from "./pages/Demos";
import Settings from "./pages/Settings";

export default function App() {
  return (
    <GovernanceProvider>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<MissionChat />} />
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
          <Route path="/demos" element={<Demos />} />
          <Route path="/settings/*" element={<Settings />} />
        </Route>
      </Routes>
    </GovernanceProvider>
  );
}

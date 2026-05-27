import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import Contracts from "./pages/Contracts";
import Plans from "./pages/Plans";
import Customs from "./pages/Customs";
import RiskGate from "./pages/RiskGate";
import Resolution from "./pages/Resolution";
import Demos from "./pages/Demos";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Dashboard />} />
        <Route path="/contracts" element={<Contracts />} />
        <Route path="/plans" element={<Plans />} />
        <Route path="/customs" element={<Customs />} />
        <Route path="/risk" element={<RiskGate />} />
        <Route path="/resolution" element={<Resolution />} />
        <Route path="/demos" element={<Demos />} />
      </Route>
    </Routes>
  );
}

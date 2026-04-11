import { BrowserRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import Layout from "./components/Layout";
import Overview from "./pages/Overview";
import Deployments from "./pages/Deployments";
import Agents from "./pages/Agents";
import Health from "./pages/Health";
import Skills from "./pages/Skills";
import Chat from "./pages/Chat";
import Metrics from "./pages/Metrics";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 5000,
      refetchOnWindowFocus: true,
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<Overview />} />
            <Route path="deployments" element={<Deployments />} />
            <Route path="agents" element={<Agents />} />
            <Route path="health" element={<Health />} />
            <Route path="skills" element={<Skills />} />
            <Route path="chat" element={<Chat />} />
            <Route path="metrics" element={<Metrics />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

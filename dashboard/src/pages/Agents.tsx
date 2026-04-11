import { useState, useMemo } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Search } from "lucide-react";
import DataTable from "../components/DataTable";
import type { Column } from "../components/DataTable";
import { fetchAgents, registerAgent } from "../api/client";
import type { Agent } from "../types";

export default function Agents() {
  const queryClient = useQueryClient();
  const [showRegister, setShowRegister] = useState(false);
  const [search, setSearch] = useState("");

  // Form state
  const [formName, setFormName] = useState("");
  const [formRole, setFormRole] = useState("worker");
  const [formVersion, setFormVersion] = useState("0.1.0");
  const [formCaps, setFormCaps] = useState("");

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: fetchAgents,
  });

  const registerMut = useMutation({
    mutationFn: registerAgent,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["agents"] });
      setShowRegister(false);
      setFormName("");
      setFormCaps("");
    },
  });

  const filteredAgents = useMemo(() => {
    if (!agentsQuery.data) return [];
    if (!search.trim()) return agentsQuery.data;
    const q = search.toLowerCase();
    return agentsQuery.data.filter((a) => {
      const name = (a.name ?? a.agent_name ?? "").toLowerCase();
      const role = (a.role ?? "").toLowerCase();
      const caps = Array.isArray(a.capabilities)
        ? a.capabilities.join(" ").toLowerCase()
        : "";
      return name.includes(q) || role.includes(q) || caps.includes(q);
    });
  }, [agentsQuery.data, search]);

  const formatList = (val: string[] | string | undefined): string => {
    if (!val) return "";
    if (Array.isArray(val)) return val.join(", ");
    return val;
  };

  const columns: Column<Agent>[] = [
    {
      key: "name",
      header: "Name",
      render: (a) => a.name ?? a.agent_name ?? "--",
    },
    { key: "role", header: "Role", render: (a) => a.role ?? "--" },
    { key: "version", header: "Version", render: (a) => a.version ?? "--" },
    {
      key: "capabilities",
      header: "Capabilities",
      render: (a) => {
        const caps = formatList(a.capabilities);
        return caps ? (
          <span className="truncate-cell" title={caps}>
            {caps}
          </span>
        ) : (
          <span className="text-muted">none</span>
        );
      },
    },
    {
      key: "skills",
      header: "Skills",
      render: (a) => {
        const skills = formatList(a.skills);
        return skills ? (
          <span className="truncate-cell" title={skills}>
            {skills}
          </span>
        ) : (
          <span className="text-muted">none</span>
        );
      },
    },
  ];

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Agents</h1>
          <p className="section-subtitle">
            Agent registry and capability management
          </p>
        </div>
      </div>

      {/* Search */}
      <div className="search-bar">
        <Search size={16} className="search-bar__icon" />
        <input
          type="text"
          placeholder="Search agents by name, role, or capability..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      {/* Register Form */}
      {showRegister && (
        <div className="form-panel">
          <h3>Register Agent</h3>
          <div className="form-row">
            <div className="form-group">
              <label htmlFor="agent-name">Name</label>
              <input
                id="agent-name"
                type="text"
                placeholder="my-agent"
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
              />
            </div>
            <div className="form-group">
              <label htmlFor="agent-role">Role</label>
              <select
                id="agent-role"
                value={formRole}
                onChange={(e) => setFormRole(e.target.value)}
              >
                <option value="worker">Worker</option>
                <option value="orchestrator">Orchestrator</option>
                <option value="evaluator">Evaluator</option>
              </select>
            </div>
            <div className="form-group" style={{ maxWidth: 120 }}>
              <label htmlFor="agent-version">Version</label>
              <input
                id="agent-version"
                type="text"
                value={formVersion}
                onChange={(e) => setFormVersion(e.target.value)}
              />
            </div>
            <div className="form-group">
              <label htmlFor="agent-caps">Capabilities (comma-separated)</label>
              <input
                id="agent-caps"
                type="text"
                placeholder="chat, search, code"
                value={formCaps}
                onChange={(e) => setFormCaps(e.target.value)}
              />
            </div>
          </div>
          <div className="form-row" style={{ marginTop: 12 }}>
            <button
              className="btn btn--primary"
              disabled={registerMut.isPending || !formName.trim()}
              onClick={() =>
                registerMut.mutate({
                  agent_name: formName.trim(),
                  role: formRole,
                  version: formVersion || "0.1.0",
                  capabilities: formCaps
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
            >
              {registerMut.isPending ? "Registering..." : "Register"}
            </button>
            <button className="btn" onClick={() => setShowRegister(false)}>
              Cancel
            </button>
          </div>
          {registerMut.isError && (
            <p className="form-error">{registerMut.error.message}</p>
          )}
        </div>
      )}

      <DataTable
        title={`Registered Agents (${filteredAgents.length})`}
        columns={columns}
        data={filteredAgents}
        emptyMessage="No agents registered"
        actions={
          <button
            className="btn btn--primary btn--sm"
            onClick={() => setShowRegister(!showRegister)}
          >
            <Plus size={14} /> Register Agent
          </button>
        }
      />
    </section>
  );
}

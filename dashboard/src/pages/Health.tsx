import { useQuery } from "@tanstack/react-query";
import { HeartPulse, AlertTriangle, XCircle, Skull, RefreshCw } from "lucide-react";
import StatCard from "../components/StatCard";
import StatusBadge from "../components/StatusBadge";
import { fetchHealth } from "../api/client";
import type { AgentHealth, HealthProbe } from "../types";

export default function Health() {
  const healthQuery = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
    refetchInterval: 10000,
  });

  const { summary, agents } = healthQuery.data ?? {
    summary: { healthy: 0, degraded: 0, unhealthy: 0, dead: 0 },
    agents: [],
  };

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Health Monitoring</h1>
          <p className="section-subtitle">
            Agent health status and probe results
          </p>
        </div>
        <button
          className="btn btn--icon"
          onClick={() => void healthQuery.refetch()}
          title="Refresh"
        >
          <RefreshCw
            size={14}
            className={healthQuery.isFetching ? "spin" : ""}
          />
          Refresh
        </button>
      </div>

      {/* Summary cards */}
      <div className="cards-grid">
        <StatCard
          label="Healthy"
          value={summary.healthy}
          icon={HeartPulse}
          color="green"
        />
        <StatCard
          label="Degraded"
          value={summary.degraded}
          icon={AlertTriangle}
          color="yellow"
        />
        <StatCard
          label="Unhealthy"
          value={summary.unhealthy}
          icon={XCircle}
          color="red"
        />
        <StatCard
          label="Dead"
          value={summary.dead}
          icon={Skull}
          color="red"
        />
      </div>

      {/* Per-agent health cards */}
      <div className="health-grid">
        {agents.length === 0 && (
          <div className="empty-state" style={{ gridColumn: "1 / -1" }}>
            <HeartPulse size={48} strokeWidth={1.2} />
            <p>No health data available</p>
          </div>
        )}
        {agents.map((ag, i) => (
          <HealthCard key={ag.id ?? i} agent={ag} />
        ))}
      </div>
    </section>
  );
}

function HealthCard({ agent }: { agent: AgentHealth }) {
  const name = agent.agent_name ?? agent.name ?? agent.id ?? "Unknown";
  const status = (
    agent.status ??
    agent.health_status ??
    "unknown"
  ).toLowerCase();
  const probes = agent.probes ?? agent.checks ?? [];

  return (
    <div className="health-card">
      <div className="health-card__header">
        <span className="health-card__name">{name}</span>
        <StatusBadge status={status} />
      </div>

      {probes.length > 0 ? (
        probes.map((probe, i) => <ProbeRow key={i} probe={probe} index={i} />)
      ) : (
        <>
          {(agent.last_check ?? agent.last_heartbeat ?? agent.updated_at) && (
            <div className="health-probe">
              <span>Last Check</span>
              <span className="text-muted">
                {agent.last_check ?? agent.last_heartbeat ?? agent.updated_at}
              </span>
            </div>
          )}
          {agent.role && (
            <div className="health-probe">
              <span>Role</span>
              <span className="text-muted">{agent.role}</span>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function ProbeRow({ probe, index }: { probe: HealthProbe; index: number }) {
  const name = probe.name ?? probe.probe_name ?? `Probe ${index + 1}`;
  const ok =
    probe.ok ??
    probe.passed ??
    (probe.status === "ok" || probe.status === "healthy");

  return (
    <div className="health-probe">
      <span>{name}</span>
      <span className="health-probe__status">
        <span
          className="health-probe__dot"
          style={{
            background: ok ? "var(--color-success)" : "var(--color-error)",
          }}
        />
        <span
          style={{
            color: ok ? "var(--color-success)" : "var(--color-error)",
          }}
        >
          {ok ? "OK" : "FAIL"}
        </span>
      </span>
    </div>
  );
}

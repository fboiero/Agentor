import { useQuery } from "@tanstack/react-query";
import { Box, Users, HeartPulse, ListChecks, Clock, RefreshCw } from "lucide-react";
import StatCard from "../components/StatCard";
import { fetchSummary, fetchEvents } from "../api/client";
import type { ControlPlaneEvent } from "../types";

export default function Overview() {
  const summary = useQuery({
    queryKey: ["summary"],
    queryFn: fetchSummary,
    refetchInterval: 5000,
  });

  const events = useQuery({
    queryKey: ["events"],
    queryFn: fetchEvents,
    refetchInterval: 5000,
  });

  const s = summary.data;

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Overview</h1>
          <p className="section-subtitle">
            Real-time cluster status and recent activity
          </p>
        </div>
        <button
          className="btn btn--icon"
          onClick={() => {
            void summary.refetch();
            void events.refetch();
          }}
          title="Refresh"
        >
          <RefreshCw
            size={14}
            className={summary.isFetching ? "spin" : ""}
          />
          Refresh
        </button>
      </div>

      {/* Stat cards */}
      <div className="cards-grid">
        <StatCard
          label="Deployments"
          value={s?.total_deployments ?? "--"}
          icon={Box}
          color="blue"
        />
        <StatCard
          label="Running Instances"
          value={s?.running_instances ?? "--"}
          icon={ListChecks}
          color="green"
        />
        <StatCard
          label="Healthy Agents"
          value={s?.healthy_agents ?? "--"}
          icon={Users}
          color="yellow"
        />
        <StatCard
          label="Total Tasks"
          value={s?.total_tasks ?? "--"}
          icon={HeartPulse}
          color="red"
        />
      </div>

      {/* Events */}
      <div className="events-panel">
        <div className="events-panel__header">
          <span className="events-panel__title">Recent Events</span>
        </div>
        <div className="events-panel__list">
          {events.isError && (
            <div className="empty-state">
              <p>Failed to load events: {events.error.message}</p>
            </div>
          )}
          {events.data && events.data.length === 0 && (
            <div className="empty-state">
              <Clock size={48} strokeWidth={1.2} />
              <p>No recent events</p>
            </div>
          )}
          {events.data
            ?.slice(-15)
            .reverse()
            .map((ev, i) => (
              <EventItem key={i} event={ev} />
            ))}
        </div>
      </div>
    </section>
  );
}

function EventItem({ event }: { event: ControlPlaneEvent }) {
  const kind = classifyEvent(event);
  const msg =
    event.message ?? event.description ?? event.event_type ?? "Unknown event";
  const time = event.timestamp ? formatRelative(event.timestamp) : "";

  return (
    <div className="event-item">
      <span className={`event-item__dot event-item__dot--${kind}`} />
      <div className="event-item__content">
        <div className="event-item__msg">{msg}</div>
        {time && <div className="event-item__time">{time}</div>}
      </div>
    </div>
  );
}

function classifyEvent(ev: ControlPlaneEvent): string {
  const t = (ev.event_type ?? ev.kind ?? "").toLowerCase();
  if (t.includes("error") || t.includes("fail")) return "error";
  if (t.includes("warn") || t.includes("degrad")) return "warning";
  if (t.includes("creat") || t.includes("start") || t.includes("deploy"))
    return "success";
  return "info";
}

function formatRelative(ts: string): string {
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return ts;
    const diff = Date.now() - d.getTime();
    if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  } catch {
    return ts;
  }
}

import { useQuery } from "@tanstack/react-query";
import { BarChart3, RefreshCw, Search } from "lucide-react";
import { useState, useMemo } from "react";
import { fetchMetricsRaw, parsePrometheusMetrics } from "../api/client";

export default function Metrics() {
  const [search, setSearch] = useState("");

  const metricsQuery = useQuery({
    queryKey: ["metrics"],
    queryFn: fetchMetricsRaw,
  });

  const parsed = useMemo(() => {
    if (!metricsQuery.data) return [];
    return parsePrometheusMetrics(metricsQuery.data);
  }, [metricsQuery.data]);

  const filtered = useMemo(() => {
    if (!search.trim()) return parsed;
    const q = search.toLowerCase();
    return parsed.filter((m) => m.name.toLowerCase().includes(q));
  }, [parsed, search]);

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Metrics</h1>
          <p className="section-subtitle">
            Prometheus-compatible metrics viewer
            {parsed.length > 0 && (
              <span> | {parsed.length} data points</span>
            )}
          </p>
        </div>
        <button
          className="btn btn--icon"
          onClick={() => void metricsQuery.refetch()}
          title="Refresh"
        >
          <RefreshCw
            size={14}
            className={metricsQuery.isFetching ? "spin" : ""}
          />
          Refresh
        </button>
      </div>

      {metricsQuery.isError && (
        <div className="metrics-placeholder">
          <BarChart3 size={48} strokeWidth={1.2} />
          <h3>Metrics Not Available</h3>
          <p>
            {metricsQuery.error.message}.{" "}
            Use{" "}
            <code className="inline-code">
              GatewayServer::build_with_metrics()
            </code>{" "}
            to enable metrics collection.
          </p>
        </div>
      )}

      {parsed.length > 0 && (
        <>
          <div className="search-bar">
            <Search size={16} className="search-bar__icon" />
            <input
              type="text"
              placeholder="Filter metrics by name..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>

          <div className="data-table">
            <div className="data-table__header">
              <span className="data-table__title">
                Prometheus Metrics ({filtered.length})
              </span>
            </div>
            <div className="data-table__scroll">
              <table>
                <thead>
                  <tr>
                    <th>Metric</th>
                    <th style={{ textAlign: "right" }}>Value</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.slice(0, 100).map((m, i) => (
                    <tr key={i}>
                      <td
                        style={{
                          wordBreak: "break-all",
                          fontFamily: "var(--font-mono)",
                          fontSize: 12,
                        }}
                      >
                        {m.name}
                      </td>
                      <td
                        style={{
                          textAlign: "right",
                          fontFamily: "var(--font-mono)",
                          fontWeight: 600,
                        }}
                      >
                        {m.value}
                      </td>
                    </tr>
                  ))}
                  {filtered.length > 100 && (
                    <tr>
                      <td
                        colSpan={2}
                        style={{ textAlign: "center", color: "var(--color-text-muted)" }}
                      >
                        Showing 100 of {filtered.length} metrics. Use search to
                        narrow down.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}

      {metricsQuery.isSuccess && parsed.length === 0 && (
        <div className="metrics-placeholder">
          <BarChart3 size={48} strokeWidth={1.2} />
          <h3>No Metrics Data</h3>
          <p>The metrics endpoint returned no data points.</p>
        </div>
      )}
    </section>
  );
}

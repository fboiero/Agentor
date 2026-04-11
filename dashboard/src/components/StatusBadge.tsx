interface StatusBadgeProps {
  status: string;
}

const statusColors: Record<string, { bg: string; text: string }> = {
  running: { bg: "rgba(52, 211, 153, 0.15)", text: "#34d399" },
  healthy: { bg: "rgba(52, 211, 153, 0.15)", text: "#34d399" },
  stopped: { bg: "rgba(148, 163, 184, 0.15)", text: "#94a3b8" },
  degraded: { bg: "rgba(251, 191, 36, 0.15)", text: "#fbbf24" },
  failed: { bg: "rgba(248, 113, 113, 0.15)", text: "#f87171" },
  unhealthy: { bg: "rgba(248, 113, 113, 0.15)", text: "#f87171" },
  dead: { bg: "rgba(30, 30, 30, 0.8)", text: "#6b7280" },
  unknown: { bg: "rgba(148, 163, 184, 0.15)", text: "#94a3b8" },
};

export default function StatusBadge({ status }: StatusBadgeProps) {
  const key = status.toLowerCase();
  const colors = statusColors[key] ?? statusColors.unknown;
  const isAnimated = key === "running" || key === "healthy";

  return (
    <span
      className="status-badge"
      style={{ background: colors.bg, color: colors.text }}
    >
      <span
        className={`status-badge__dot${isAnimated ? " status-badge__dot--pulse" : ""}`}
        style={{ background: colors.text }}
      />
      {key}
    </span>
  );
}

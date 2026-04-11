import type { LucideIcon } from "lucide-react";

interface StatCardProps {
  label: string;
  value: string | number;
  icon: LucideIcon;
  color: "blue" | "green" | "yellow" | "red";
}

const colorMap: Record<string, { bg: string; text: string }> = {
  blue: { bg: "rgba(79, 140, 255, 0.12)", text: "#4f8cff" },
  green: { bg: "rgba(52, 211, 153, 0.12)", text: "#34d399" },
  yellow: { bg: "rgba(251, 191, 36, 0.12)", text: "#fbbf24" },
  red: { bg: "rgba(248, 113, 113, 0.12)", text: "#f87171" },
};

export default function StatCard({ label, value, icon: Icon, color }: StatCardProps) {
  const c = colorMap[color];

  return (
    <div className="stat-card">
      <div
        className="stat-card__icon"
        style={{ background: c.bg, color: c.text }}
      >
        <Icon size={20} />
      </div>
      <div className="stat-card__value">{value}</div>
      <div className="stat-card__label">{label}</div>
    </div>
  );
}

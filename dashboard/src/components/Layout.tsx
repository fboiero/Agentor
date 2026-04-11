import { useState, useEffect } from "react";
import { NavLink, Outlet } from "react-router-dom";
import {
  LayoutDashboard,
  Rocket,
  Users,
  HeartPulse,
  Puzzle,
  MessageSquare,
  BarChart3,
  Menu,
  X,
} from "lucide-react";

const NAV_ITEMS = [
  { to: "/", label: "Overview", icon: LayoutDashboard },
  { to: "/deployments", label: "Deployments", icon: Rocket },
  { to: "/agents", label: "Agents", icon: Users },
  { to: "/health", label: "Health", icon: HeartPulse },
  { to: "/skills", label: "Skills", icon: Puzzle },
  { to: "/chat", label: "Chat", icon: MessageSquare },
  { to: "/metrics", label: "Metrics", icon: BarChart3 },
] as const;

export default function Layout() {
  const [clock, setClock] = useState(formatTime());
  const [mobileOpen, setMobileOpen] = useState(false);

  useEffect(() => {
    const id = setInterval(() => setClock(formatTime()), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <div className="app-layout">
      {/* Top Navigation */}
      <header className="topnav">
        <div className="topnav__brand">
          <button
            className="topnav__menu-btn"
            onClick={() => setMobileOpen((v) => !v)}
            aria-label="Toggle menu"
          >
            {mobileOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
          <svg
            width="28"
            height="28"
            viewBox="0 0 100 100"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <circle cx="50" cy="50" r="48" stroke="#4f8cff" strokeWidth="4" />
            <circle cx="50" cy="50" r="20" fill="#4f8cff" opacity="0.8" />
            <circle cx="50" cy="18" r="6" fill="#4f8cff" />
            <circle cx="78" cy="66" r="6" fill="#4f8cff" />
            <circle cx="22" cy="66" r="6" fill="#4f8cff" />
            <line x1="50" y1="24" x2="50" y2="30" stroke="#4f8cff" strokeWidth="2" />
            <line x1="73" y1="63" x2="67" y2="58" stroke="#4f8cff" strokeWidth="2" />
            <line x1="27" y1="63" x2="33" y2="58" stroke="#4f8cff" strokeWidth="2" />
          </svg>
          <span className="topnav__title">Argentor</span>
          <span className="topnav__tag">Control Plane</span>
        </div>
        <div className="topnav__right">
          <span className="topnav__clock">{clock}</span>
        </div>
      </header>

      {/* Sidebar */}
      <aside className={`sidebar${mobileOpen ? " sidebar--open" : ""}`}>
        <div className="sidebar__section-label">Navigation</div>
        <nav className="sidebar__nav">
          {NAV_ITEMS.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `sidebar__link${isActive ? " sidebar__link--active" : ""}`
              }
              onClick={() => setMobileOpen(false)}
            >
              <Icon size={18} />
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar__footer">Argentor v0.1.0</div>
      </aside>

      {/* Main Content */}
      <main className="main-content">
        <Outlet />
      </main>
    </div>
  );
}

function formatTime(): string {
  const now = new Date();
  const time = now.toLocaleTimeString("en-US", { hour12: false });
  const date = now.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
  return `${time} ${date}`;
}

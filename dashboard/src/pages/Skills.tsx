import { useQuery } from "@tanstack/react-query";
import { Puzzle, RefreshCw } from "lucide-react";
import { fetchSkills } from "../api/client";

export default function Skills() {
  const skillsQuery = useQuery({
    queryKey: ["skills"],
    queryFn: fetchSkills,
  });

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Skills</h1>
          <p className="section-subtitle">
            Browse registered agent skills and capabilities
          </p>
        </div>
        <button
          className="btn btn--icon"
          onClick={() => void skillsQuery.refetch()}
          title="Refresh"
        >
          <RefreshCw
            size={14}
            className={skillsQuery.isFetching ? "spin" : ""}
          />
          Refresh
        </button>
      </div>

      {skillsQuery.isError && (
        <div className="empty-state">
          <p>Failed to load skills: {skillsQuery.error.message}</p>
        </div>
      )}

      {skillsQuery.data && skillsQuery.data.length === 0 && (
        <div className="empty-state">
          <Puzzle size={48} strokeWidth={1.2} />
          <p>No skills registered</p>
        </div>
      )}

      <div className="skills-grid">
        {skillsQuery.data?.map((skill) => (
          <div key={skill.name} className="skill-card">
            <div className="skill-card__header">
              <Puzzle size={18} className="skill-card__icon" />
              <span className="skill-card__name">{skill.name}</span>
              {skill.version && (
                <span className="skill-card__version">v{skill.version}</span>
              )}
            </div>
            {skill.description && (
              <p className="skill-card__desc">{skill.description}</p>
            )}
            {skill.parameters &&
              Object.keys(skill.parameters).length > 0 && (
                <div className="skill-card__params">
                  <span className="skill-card__params-label">Parameters:</span>
                  <code>{Object.keys(skill.parameters).join(", ")}</code>
                </div>
              )}
          </div>
        ))}
      </div>
    </section>
  );
}

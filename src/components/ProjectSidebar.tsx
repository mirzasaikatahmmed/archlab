import { useState } from "react";
import type { ProjectSummary } from "../bindings";
import { Icon } from "./Icon";
import { ThemeToggle } from "./ThemeToggle";

interface Props {
  projects: ProjectSummary[];
  currentId: string | null;
  busy: boolean;
  onCreate: (name: string) => void;
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  /** Opens the scenario library; the button is hidden when absent. */
  onOpenScenarios?: () => void;
  scenariosActive?: boolean;
  /** Opens a project file; the button is hidden when absent. */
  onImport?: () => void;
}

export function ProjectSidebar({
  projects,
  currentId,
  busy,
  onCreate,
  onOpen,
  onDelete,
  onOpenScenarios,
  scenariosActive = false,
  onImport,
}: Props) {
  const [name, setName] = useState("");
  const trimmed = name.trim();

  return (
    <nav className="sidebar" aria-label="Projects">
      <div className="brand">
        <Icon name="logo" size={20} />
        ArchLab
      </div>
      {onOpenScenarios && (
        <button
          type="button"
          className="nav-button"
          aria-pressed={scenariosActive}
          onClick={onOpenScenarios}
        >
          <Icon name="book" />
          Scenarios
        </button>
      )}
      <form
        className="new-project"
        onSubmit={(e) => {
          e.preventDefault();
          if (trimmed) {
            onCreate(trimmed);
            setName("");
          }
        }}
      >
        <label htmlFor="new-project">New project</label>
        <input
          id="new-project"
          value={name}
          maxLength={100}
          placeholder="Project name"
          onChange={(e) => {
            setName(e.target.value);
          }}
        />
        <button type="submit" className="primary" disabled={busy || !trimmed}>
          <Icon name="plus" />
          Create
        </button>
      </form>
      {onImport && (
        <button type="button" className="nav-button" disabled={busy} onClick={onImport}>
          <Icon name="upload" />
          Import project
        </button>
      )}
      {projects.length === 0 ? (
        <p className="muted">No projects yet.</p>
      ) : (
        <ul className="project-list">
          {projects.map((p) => (
            <li key={p.id} className={p.id === currentId ? "active" : undefined}>
              <button
                type="button"
                className="link"
                aria-current={p.id === currentId}
                onClick={() => {
                  onOpen(p.id);
                }}
              >
                {p.name}
              </button>
              <button
                type="button"
                className="icon-btn"
                aria-label={`Delete ${p.name}`}
                onClick={() => {
                  if (window.confirm(`Delete "${p.name}"? This cannot be undone.`)) {
                    onDelete(p.id);
                  }
                }}
              >
                <Icon name="trash" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="sidebar-foot">
        <ThemeToggle />
        <p className="muted">All simulation results are estimates.</p>
      </div>
    </nav>
  );
}

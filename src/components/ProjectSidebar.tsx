import { useState } from "react";
import type { ProjectSummary } from "../bindings";

interface Props {
  projects: ProjectSummary[];
  currentId: string | null;
  busy: boolean;
  onCreate: (name: string) => void;
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
}

export function ProjectSidebar({ projects, currentId, busy, onCreate, onOpen, onDelete }: Props) {
  const [name, setName] = useState("");
  const trimmed = name.trim();

  return (
    <nav className="sidebar" aria-label="Projects">
      <form
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
        <button type="submit" disabled={busy || !trimmed}>
          Create
        </button>
      </form>
      {projects.length === 0 ? (
        <p className="muted">No projects yet.</p>
      ) : (
        <ul>
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
                aria-label={`Delete ${p.name}`}
                onClick={() => {
                  if (window.confirm(`Delete "${p.name}"? This cannot be undone.`)) {
                    onDelete(p.id);
                  }
                }}
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}

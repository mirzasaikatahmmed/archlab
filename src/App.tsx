import { useEffect, useState } from "react";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { SimulationPanel } from "./components/SimulationPanel";
import { ServerProfileForm } from "./components/ServerProfileForm";
import { TrafficProfileEditor } from "./components/TrafficProfileEditor";
import { TopologyCanvas } from "./components/topology/TopologyCanvas";
import { useProjectStore } from "./store/projectStore";

type Tab = "server" | "traffic" | "topology" | "simulate";

const TABS: { id: Tab; label: string }[] = [
  { id: "server", label: "Server profile" },
  { id: "traffic", label: "Traffic" },
  { id: "topology", label: "Topology" },
  { id: "simulate", label: "Simulate" },
];

export default function App() {
  const store = useProjectStore();
  const [tab, setTab] = useState<Tab>("server");
  const { current } = store;

  useEffect(() => {
    void useProjectStore.getState().init();
  }, []);

  return (
    <div className="shell">
      <ProjectSidebar
        projects={store.projects}
        currentId={current?.id ?? null}
        busy={store.busy}
        onCreate={(name) => void store.createProject(name)}
        onOpen={(id) => void store.openProject(id)}
        onDelete={(id) => void store.deleteProject(id)}
      />
      <main className="main">
        {store.error && (
          <div role="alert" className="banner">
            {store.issues.length > 0 ? (
              <ul>
                {store.issues.map((i) => (
                  <li key={`${i.field}:${i.message}`}>
                    <code>{i.field}</code>: {i.message}
                  </li>
                ))}
              </ul>
            ) : (
              store.error
            )}
          </div>
        )}
        {current ? (
          <div key={current.id}>
            <header className="project-header">
              <input
                aria-label="Project name"
                value={current.name}
                maxLength={100}
                onChange={(e) => {
                  store.rename(e.target.value);
                }}
              />
              <button
                type="button"
                onClick={() => void store.saveCurrent()}
                disabled={store.busy || !store.dirty}
              >
                Save
              </button>
              <span className="muted">{store.dirty ? "Unsaved changes" : "Saved"}</span>
            </header>
            <div role="tablist" className="tabs">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  role="tab"
                  type="button"
                  aria-selected={tab === t.id}
                  onClick={() => {
                    setTab(t.id);
                  }}
                >
                  {t.label}
                </button>
              ))}
            </div>
            <section role="tabpanel">
              {tab === "server" && (
                <ServerProfileForm
                  value={current.serverProfile}
                  onChange={store.setServerProfile}
                />
              )}
              {tab === "traffic" && (
                <TrafficProfileEditor
                  value={current.trafficProfile}
                  onChange={store.setTrafficProfile}
                />
              )}
              {tab === "topology" && (
                <TopologyCanvas
                  topology={current.topology}
                  templates={store.templates}
                  onChange={store.setTopology}
                />
              )}
              {tab === "simulate" && <SimulationPanel project={current} />}
            </section>
          </div>
        ) : (
          <div className="empty">
            <h1>ArchLab</h1>
            <p>Create or open a project to define a server, traffic and topology.</p>
          </div>
        )}
        <footer className="muted">All simulation results are estimates.</footer>
      </main>
    </div>
  );
}

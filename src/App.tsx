import { useEffect, useState } from "react";
import { CalibrationSection } from "./components/CalibrationSection";
import { ChaosEditor } from "./components/ChaosEditor";
import { Icon, type IconName } from "./components/Icon";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { ScenarioLibrary } from "./components/ScenarioLibrary";
import { ScenarioPlayer } from "./components/ScenarioPlayer";
import { SimulateTab } from "./components/SimulateTab";
import { WhatIfTab } from "./components/WhatIfTab";
import { ServerProfileForm } from "./components/ServerProfileForm";
import { TrafficProfileEditor } from "./components/TrafficProfileEditor";
import { TopologyCanvas } from "./components/topology/TopologyCanvas";
import { useProjectStore } from "./store/projectStore";

type View = { kind: "project" } | { kind: "library" } | { kind: "play"; id: string };

type Tab = "server" | "traffic" | "topology" | "chaos" | "simulate" | "whatif";

const TABS: { id: Tab; label: string; icon: IconName }[] = [
  { id: "server", label: "Server profile", icon: "server" },
  { id: "traffic", label: "Traffic", icon: "traffic" },
  { id: "topology", label: "Topology", icon: "topology" },
  { id: "chaos", label: "Chaos", icon: "bolt" },
  { id: "simulate", label: "Simulate", icon: "play" },
  { id: "whatif", label: "What-if", icon: "branch" },
];

export default function App() {
  const store = useProjectStore();
  const [tab, setTab] = useState<Tab>("server");
  const [view, setView] = useState<View>({ kind: "project" });
  /** Bumped when calibration rewrites the profile, so the uncontrolled form reloads it. */
  const [formVersion, setFormVersion] = useState(0);
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
        onCreate={(name) => {
          setView({ kind: "project" });
          void store.createProject(name);
        }}
        onOpen={(id) => {
          setView({ kind: "project" });
          void store.openProject(id);
        }}
        onDelete={(id) => void store.deleteProject(id)}
        onOpenScenarios={() => {
          setView({ kind: "library" });
        }}
        scenariosActive={view.kind !== "project"}
        onImport={() => {
          void store.importFile().then((opened) => {
            if (opened) {
              setView({ kind: "project" });
            }
          });
        }}
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
        {view.kind === "library" ? (
          <ScenarioLibrary
            onPlay={(id) => {
              setView({ kind: "play", id });
            }}
          />
        ) : view.kind === "play" ? (
          <ScenarioPlayer
            scenarioId={view.id}
            onExit={() => {
              setView({ kind: "library" });
            }}
            onOpenAsProject={(project) => {
              void store.importProject(project).then(() => {
                setView({ kind: "project" });
              });
            }}
          />
        ) : current ? (
          <div key={current.id} className="workspace">
            <header className="topbar">
              <input
                aria-label="Project name"
                value={current.name}
                maxLength={100}
                onChange={(e) => {
                  store.rename(e.target.value);
                }}
              />
              <span className="status" data-dirty={store.dirty}>
                {store.dirty ? "Unsaved changes" : "Saved"}
              </span>
              <button
                type="button"
                className="primary"
                onClick={() => void store.saveCurrent()}
                disabled={store.busy || !store.dirty}
              >
                Save
              </button>
            </header>
            <div role="tablist" className="tabs">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  role="tab"
                  type="button"
                  className="tab"
                  aria-selected={tab === t.id}
                  onClick={() => {
                    setTab(t.id);
                  }}
                >
                  <Icon name={t.icon} />
                  {t.label}
                </button>
              ))}
            </div>
            <section role="tabpanel" className="page">
              {tab === "server" && (
                <>
                  <ServerProfileForm
                    key={formVersion}
                    value={current.serverProfile}
                    onChange={store.setServerProfile}
                  />
                  <CalibrationSection
                    profile={current.serverProfile}
                    onApply={(profile) => {
                      store.setServerProfile(profile);
                      setFormVersion((v) => v + 1);
                    }}
                  />
                </>
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
              {tab === "chaos" && (
                <ChaosEditor
                  value={current.chaos}
                  topology={current.topology}
                  onChange={store.setChaos}
                />
              )}
              {tab === "whatif" && (
                <WhatIfTab
                  project={current}
                  templates={store.templates}
                  onChange={store.setWhatIf}
                />
              )}
              {tab === "simulate" && <SimulateTab project={current} />}
            </section>
          </div>
        ) : (
          <div className="empty">
            <Icon name="logo" size={36} />
            <h1>Plan capacity before you build</h1>
            <p className="muted">
              Create a project in the sidebar, describe your server, traffic and topology, then
              simulate where it saturates.
            </p>
          </div>
        )}
      </main>
    </div>
  );
}

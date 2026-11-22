import { useEffect, useState } from "react";
import type { BlueprintView, Project, TierView } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { kindLabel } from "./topology/flowMapping";

interface Props {
  /** Receives the design at the chosen size, ready to open as a project. */
  onUse: (project: Project, tier: TierView) => void;
}

const number = (n: number | null) => Math.round(n ?? 0).toLocaleString("en-US");

function kinds(blueprint: BlueprintView): string[] {
  const seen = new Set<string>();
  for (const n of blueprint.topology.nodes) {
    if (n.kind !== "client") {
      seen.add(kindLabel(n.kind));
    }
  }
  return [...seen];
}

/** Cards for the ready-made architectures, each in sizes by number of daily users. */
export function BlueprintLibrary({ onUse }: Props) {
  const [blueprints, setBlueprints] = useState<BlueprintView[] | null>(null);
  const [chosen, setChosen] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .listBlueprints()
      .then((list) => {
        if (live) {
          setBlueprints(list);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(
            err instanceof CommandError ? describeError(err.error) : "Could not load blueprints",
          );
        }
      });
    return () => {
      live = false;
    };
  }, []);

  const use = async (blueprint: BlueprintView, tier: TierView) => {
    setBusy(true);
    setError(null);
    try {
      onUse(await api.blueprintProject(blueprint.id, tier.id), tier);
    } catch (err) {
      setError(
        err instanceof CommandError ? describeError(err.error) : "Could not open the blueprint",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="library">
      <header className="library-head">
        <h1>Blueprints</h1>
        <p className="muted">
          Ready-made architectures for common kinds of product, each in sizes by daily users. Every
          size was simulated and meets its targets, so you start from a design that works and change
          it from there. Results are estimates.
        </p>
      </header>
      {error && <p role="alert">{error}</p>}
      {!blueprints && !error && <p className="muted">Loading…</p>}
      <ul className="scenario-grid">
        {blueprints?.map((b) => {
          const tier = b.tiers.find((t) => t.id === chosen[b.id]) ?? b.tiers[0];
          return (
            <li key={b.id} className="scenario-card blueprint-card">
              <h2>{b.title}</h2>
              <p className="muted">{b.summary}</p>
              <ul className="chips" aria-label="Parts">
                {kinds(b).map((k) => (
                  <li key={k} className="chip">
                    {k}
                  </li>
                ))}
              </ul>
              <div role="group" aria-label={`Size of ${b.title}`} className="mode-switch">
                {b.tiers.map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    aria-pressed={t.id === tier.id}
                    onClick={() => {
                      setChosen((prev) => ({ ...prev, [b.id]: t.id }));
                    }}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
              <p className="blueprint-facts">
                About {number(tier.peakRps)} requests per second at peak, {tier.machines}{" "}
                {tier.machines === 1 ? "machine" : "machines"}. Run the simulation for {tier.runS}{" "}
                s.
              </p>
              <details>
                <summary>Why this design</summary>
                <ul>
                  {b.why.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </details>
              <button
                type="button"
                className="primary"
                disabled={busy}
                aria-label={`Use ${b.title}`}
                onClick={() => {
                  void use(b, tier);
                }}
              >
                Use this design
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

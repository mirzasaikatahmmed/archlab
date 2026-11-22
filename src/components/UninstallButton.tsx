import { useState } from "react";
import type { UninstallPlan } from "../bindings";
import { CommandError, api, describeError } from "../ipc";

/** A quiet button in the sidebar that opens a confirmation before anything is removed. */
export function UninstallButton() {
  const [plan, setPlan] = useState<UninstallPlan | null>(null);
  const [open, setOpen] = useState(false);
  const [deleteData, setDeleteData] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const show = async () => {
    setError(null);
    setDeleteData(false);
    try {
      setPlan(await api.uninstallPlan());
      setOpen(true);
    } catch (err) {
      setError(err instanceof CommandError ? describeError(err.error) : "Could not check.");
    }
  };

  const close = () => {
    setOpen(false);
    setError(null);
  };

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.uninstallApp(deleteData);
    } catch (err) {
      setError(
        err instanceof CommandError
          ? describeError(err.error)
          : "ArchLab could not be uninstalled.",
      );
      setBusy(false);
    }
  };

  return (
    <>
      <button
        type="button"
        className="link-button"
        onClick={() => {
          void show();
        }}
      >
        Uninstall ArchLab…
      </button>
      {error && !open && <p role="alert">{error}</p>}
      {open && plan && (
        <div className="overlay">
          <div role="dialog" aria-modal="true" aria-labelledby="uninstall-title" className="dialog">
            <h2 id="uninstall-title">Uninstall ArchLab</h2>
            <p>{plan.title}</p>
            {plan.supported && (
              <>
                <ul>
                  {plan.details.map((d) => (
                    <li key={d}>{d}</li>
                  ))}
                </ul>
                {plan.canRemoveData && (
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={deleteData}
                      onChange={(e) => {
                        setDeleteData(e.target.checked);
                      }}
                    />
                    Also delete my projects and settings
                  </label>
                )}
                {plan.canRemoveData && (
                  <p className="muted">
                    {deleteData
                      ? `This permanently deletes ${plan.dataDir}. It cannot be undone.`
                      : `Your projects stay in ${plan.dataDir}, so a later install finds them.`}
                  </p>
                )}
              </>
            )}
            {error && <p role="alert">{error}</p>}
            <div className="form-actions">
              <button type="button" onClick={close} disabled={busy}>
                {plan.supported ? "Cancel" : "Close"}
              </button>
              {plan.supported && (
                <button
                  type="button"
                  className="danger"
                  disabled={busy}
                  onClick={() => {
                    void confirm();
                  }}
                >
                  {busy ? "Uninstalling…" : "Uninstall"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}

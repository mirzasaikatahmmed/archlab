import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UninstallPlan } from "../bindings";

const api = vi.hoisted(() => ({ uninstallPlan: vi.fn(), uninstallApp: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { UninstallButton } from "./UninstallButton";

const plan: UninstallPlan = {
  supported: true,
  title: "Move ArchLab to the Trash",
  details: ["ArchLab is moved to the Trash, so you can still restore it.", "ArchLab quits first."],
  asksForPassword: false,
  canRemoveData: true,
  dataDir: "/Users/me/Library/Application Support/dev.archlab.app",
};

describe("UninstallButton", () => {
  beforeEach(() => {
    api.uninstallPlan.mockReset();
    api.uninstallApp.mockReset();
    api.uninstallPlan.mockResolvedValue(plan);
  });

  it("does nothing until the person confirms", async () => {
    render(<UninstallButton />);
    await userEvent.click(screen.getByRole("button", { name: "Uninstall ArchLab…" }));
    const dialog = await screen.findByRole("dialog", { name: "Uninstall ArchLab" });
    expect(dialog).toHaveTextContent("Move ArchLab to the Trash");
    expect(dialog).toHaveTextContent("you can still restore it");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.uninstallApp).not.toHaveBeenCalled();
  });

  it("keeps the projects by default and says where they stay", async () => {
    api.uninstallApp.mockResolvedValue(null);
    render(<UninstallButton />);
    await userEvent.click(screen.getByRole("button", { name: "Uninstall ArchLab…" }));
    expect(await screen.findByText(/Your projects stay in/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    expect(api.uninstallApp).toHaveBeenCalledWith(false);
  });

  it("warns before deleting projects and settings, and only then asks for it", async () => {
    api.uninstallApp.mockResolvedValue(null);
    render(<UninstallButton />);
    await userEvent.click(screen.getByRole("button", { name: "Uninstall ArchLab…" }));
    await userEvent.click(await screen.findByRole("checkbox", { name: /delete my projects/ }));
    expect(screen.getByText(/permanently deletes/)).toHaveTextContent("It cannot be undone");
    await userEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    expect(api.uninstallApp).toHaveBeenCalledWith(true);
  });

  it("explains a copy that cannot be uninstalled and offers no uninstall button", async () => {
    api.uninstallPlan.mockResolvedValue({
      ...plan,
      supported: false,
      title:
        "ArchLab is not installed in your Applications folder, so there is nothing to uninstall here.",
      details: [],
      canRemoveData: false,
    });
    render(<UninstallButton />);
    await userEvent.click(screen.getByRole("button", { name: "Uninstall ArchLab…" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("nothing to uninstall here");
    expect(screen.queryByRole("button", { name: "Uninstall" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows why an uninstall failed and keeps the dialog open", async () => {
    api.uninstallApp.mockRejectedValue(
      new CommandError({ kind: "runtime", message: "The package was not removed." }),
    );
    render(<UninstallButton />);
    await userEvent.click(screen.getByRole("button", { name: "Uninstall ArchLab…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Uninstall" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The package was not removed.");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });
});

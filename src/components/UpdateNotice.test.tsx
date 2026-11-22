import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  checkForUpdate: vi.fn(),
  appInfo: vi.fn(),
  restartApp: vi.fn(),
}));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { UpdateNotice } from "./UpdateNotice";

describe("UpdateNotice", () => {
  beforeEach(() => {
    Object.values(api).forEach((m) => {
      m.mockReset();
    });
    api.appInfo.mockResolvedValue({ name: "ArchLab", version: "0.1.0" });
    api.restartApp.mockResolvedValue(undefined);
    localStorage.clear();
  });

  it("shows the installed version", async () => {
    api.checkForUpdate.mockResolvedValue(null);
    render(<UpdateNotice />);
    expect(await screen.findByText("ArchLab 0.1.0")).toBeInTheDocument();
  });

  it("checks at startup, offers a newer version with its notes, installs and restarts", async () => {
    const install = vi.fn((onProgress: (f: number | null) => void) => {
      onProgress(0.5);
      return Promise.resolve();
    });
    api.checkForUpdate.mockResolvedValue({ version: "0.2.0", notes: "Adds blueprints.", install });
    render(<UpdateNotice />);
    expect(await screen.findByText("Version 0.2.0 is available.")).toBeInTheDocument();
    expect(screen.getByText("Adds blueprints.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Install update" }));
    expect(install).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(/Version 0.2.0 is installed/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restart now" }));
    expect(api.restartApp).toHaveBeenCalledTimes(1);
  });

  it("shows the download progress while installing", async () => {
    let finish: () => void = () => undefined;
    const install = vi.fn(
      (onProgress: (f: number | null) => void) =>
        new Promise<void>((resolve) => {
          onProgress(0.25);
          finish = resolve;
        }),
    );
    api.checkForUpdate.mockResolvedValue({ version: "0.2.0", notes: null, install });
    render(<UpdateNotice />);
    await userEvent.click(await screen.findByRole("button", { name: "Install update" }));
    const bar = await screen.findByRole("progressbar", { name: "Update download" });
    expect(bar).toHaveAttribute("value", "0.25");
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    finish();
    expect(await screen.findByText(/is installed/)).toBeInTheDocument();
  });

  it("stays quiet when the startup check fails", async () => {
    api.checkForUpdate.mockRejectedValue(new Error("offline"));
    render(<UpdateNotice />);
    await vi.waitFor(() => {
      expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
  });

  it("explains a failed manual check, including a plain-string error from the updater", async () => {
    localStorage.setItem("archlab.autoUpdateCheck", "off");
    render(<UpdateNotice />);
    expect(api.checkForUpdate).not.toHaveBeenCalled();

    api.checkForUpdate.mockRejectedValueOnce(
      "Could not fetch a valid release JSON from the remote",
    );
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not check for updates: Could not fetch a valid release JSON from the remote",
    );

    api.checkForUpdate.mockRejectedValueOnce(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not reach the update server. Try again later.",
    );

    api.checkForUpdate.mockResolvedValueOnce(null);
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("status")).toHaveTextContent("up to date");
  });

  it("remembers turning the startup check off", async () => {
    api.checkForUpdate.mockResolvedValue(null);
    render(<UpdateNotice />);
    await userEvent.click(screen.getByRole("checkbox", { name: "Check at startup" }));
    expect(localStorage.getItem("archlab.autoUpdateCheck")).toBe("off");
  });

  it("shows an install failure", async () => {
    api.checkForUpdate.mockResolvedValue({
      version: "0.2.0",
      notes: null,
      install: () => Promise.reject(new Error("signature does not match")),
    });
    render(<UpdateNotice />);
    await userEvent.click(await screen.findByRole("button", { name: "Install update" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The update could not be installed: signature does not match",
    );
  });
});

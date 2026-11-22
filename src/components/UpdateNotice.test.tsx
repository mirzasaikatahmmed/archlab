import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const checkForUpdate = vi.hoisted(() => vi.fn());
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api: { checkForUpdate } };
});

import { UpdateNotice } from "./UpdateNotice";

describe("UpdateNotice", () => {
  beforeEach(() => {
    checkForUpdate.mockReset();
    localStorage.clear();
  });

  it("checks at startup and offers a newer version", async () => {
    const install = vi.fn(() => Promise.resolve());
    checkForUpdate.mockResolvedValue({ version: "0.2.0", install });
    render(<UpdateNotice />);
    expect(await screen.findByRole("status")).toHaveTextContent("Version 0.2.0 is available.");
    await userEvent.click(screen.getByRole("button", { name: "Install update" }));
    expect(install).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("status")).toHaveTextContent("Restart ArchLab");
  });

  it("stays quiet when the startup check fails", async () => {
    checkForUpdate.mockRejectedValue(new Error("offline"));
    render(<UpdateNotice />);
    await vi.waitFor(() => {
      expect(checkForUpdate).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
  });

  it("reports a failed manual check and a current version", async () => {
    localStorage.setItem("archlab.autoUpdateCheck", "off");
    render(<UpdateNotice />);
    expect(checkForUpdate).not.toHaveBeenCalled();

    checkForUpdate.mockRejectedValueOnce(new Error("could not reach the update server"));
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("could not reach the update server");

    checkForUpdate.mockResolvedValueOnce(null);
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("status")).toHaveTextContent("up to date");
  });

  it("remembers turning the startup check off", async () => {
    checkForUpdate.mockResolvedValue(null);
    render(<UpdateNotice />);
    await userEvent.click(screen.getByRole("checkbox", { name: "Check at startup" }));
    expect(localStorage.getItem("archlab.autoUpdateCheck")).toBe("off");
  });

  it("shows an install failure", async () => {
    checkForUpdate.mockResolvedValue({
      version: "0.2.0",
      install: () => Promise.reject(new Error("signature does not match")),
    });
    render(<UpdateNotice />);
    await userEvent.click(await screen.findByRole("button", { name: "Install update" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("signature does not match");
  });
});

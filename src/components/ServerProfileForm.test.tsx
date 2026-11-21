import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { serverProfile } from "../test/fixtures";

const detectMachine = vi.hoisted(() => vi.fn());
vi.mock("../ipc", () => ({ api: { detectMachine } }));

import { ServerProfileForm } from "./ServerProfileForm";

describe("ServerProfileForm", () => {
  beforeEach(() => {
    detectMachine.mockReset();
  });

  it("renders the current profile", () => {
    render(<ServerProfileForm value={serverProfile} onChange={vi.fn()} />);
    expect(screen.getByLabelText("CPU cores")).toHaveValue(4);
    expect(screen.getByLabelText("Disk IOPS")).toHaveValue(20000);
    expect(screen.getByLabelText("Operating system")).toHaveValue("linux");
  });

  it("emits valid edits", async () => {
    const onChange = vi.fn();
    render(<ServerProfileForm value={serverProfile} onChange={onChange} />);
    const cores = screen.getByLabelText("CPU cores");
    await userEvent.clear(cores);
    await userEvent.type(cores, "16");
    await waitFor(() => {
      expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ cpuCores: 16 }));
    });
  });

  it("shows an error and withholds invalid values", async () => {
    const onChange = vi.fn();
    render(<ServerProfileForm value={serverProfile} onChange={onChange} />);
    const cores = screen.getByLabelText("CPU cores");
    await userEvent.clear(cores);
    await userEvent.type(cores, "0");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalledWith(expect.objectContaining({ cpuCores: 0 }));
  });

  it("fills the form from 'Detect this machine' and shows notes", async () => {
    detectMachine.mockResolvedValue({
      profile: { ...serverProfile, cpuCores: 12, ramGb: 32 },
      notes: ["Disk IOPS is a typical value"],
    });
    const onChange = vi.fn();
    render(<ServerProfileForm value={serverProfile} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "Detect this machine" }));
    await waitFor(() => {
      expect(screen.getByLabelText("CPU cores")).toHaveValue(12);
    });
    expect(screen.getByLabelText("RAM (GB)")).toHaveValue(32);
    expect(screen.getByText("Disk IOPS is a typical value")).toBeInTheDocument();
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ cpuCores: 12 }));
  });

  it("reports detection failures", async () => {
    detectMachine.mockRejectedValue(new Error("no access"));
    render(<ServerProfileForm value={serverProfile} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Detect this machine" }));
    expect(await screen.findByText(/Detection failed: no access/)).toBeInTheDocument();
  });
});

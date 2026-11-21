import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { TrafficProfile } from "../bindings";
import { trafficProfile } from "../test/fixtures";
import { TrafficProfileEditor } from "./TrafficProfileEditor";

describe("TrafficProfileEditor", () => {
  it("shows the write percentage derived from the read ratio", () => {
    render(<TrafficProfileEditor value={trafficProfile} onChange={vi.fn()} />);
    expect(screen.getByText("20% writes")).toBeInTheDocument();
  });

  it("rejects a read ratio above 1", async () => {
    const onChange = vi.fn();
    render(<TrafficProfileEditor value={trafficProfile} onChange={onChange} />);
    const ratio = screen.getByLabelText("Read ratio (0–1)");
    await userEvent.clear(ratio);
    await userEvent.type(ratio, "1.5");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalledWith(expect.objectContaining({ readRatio: 1.5 }));
  });

  it("reveals spike fields and emits a valid periodic spike", async () => {
    const onChange = vi.fn<(p: TrafficProfile) => void>();
    render(<TrafficProfileEditor value={trafficProfile} onChange={onChange} />);
    expect(screen.queryByLabelText("Spike multiplier")).not.toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Spike pattern"), "periodic");
    expect(await screen.findByLabelText("Spike period (s)")).toBeInTheDocument();
    await waitFor(() => {
      expect(onChange.mock.lastCall?.[0].spike.kind).toBe("periodic");
    });
  });

  it("flags a periodic spike whose duration is not shorter than its period", async () => {
    render(
      <TrafficProfileEditor
        value={{
          ...trafficProfile,
          spike: { kind: "periodic", periodS: 60, multiplier: 2, durationS: 30 },
        }}
        onChange={vi.fn()}
      />,
    );
    const duration = screen.getByLabelText("Spike duration (s)");
    await userEvent.clear(duration);
    await userEvent.type(duration, "60");
    expect(await screen.findByText("Must be shorter than the period")).toBeInTheDocument();
  });
});

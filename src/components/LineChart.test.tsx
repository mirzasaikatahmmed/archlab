import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LineChart } from "./LineChart";

describe("LineChart", () => {
  it("describes the latest values and draws one point per sample", () => {
    const { container } = render(
      <LineChart
        title="Throughput"
        unit="req/s"
        series={[
          { label: "Completed", values: [1, 2, 4] },
          { label: "Errors", values: [0, 0, 1] },
        ]}
      />,
    );
    expect(
      screen.getByRole("img", { name: /Completed 4\.0 req\/s, Errors 1\.0 req\/s/ }),
    ).toBeInTheDocument();
    const lines = container.querySelectorAll("polyline");
    expect(lines).toHaveLength(2);
    expect(lines[0].getAttribute("points")?.split(" ")).toHaveLength(3);
  });

  it("renders with no data", () => {
    render(<LineChart title="Empty" unit="ms" series={[{ label: "p95", values: [] }]} />);
    expect(screen.getByRole("img", { name: "Empty: p95 0.0 ms" })).toBeInTheDocument();
  });
});

describe("LineChart fault bands", () => {
  it("shades a fault window and labels it", () => {
    const { container } = render(
      <LineChart
        title="Throughput"
        unit="req/s"
        series={[{ label: "Completed", values: [1, 2, 3, 4, 5, 6] }]}
        bands={[{ from: 2, to: 4, label: "Crash and restart" }]}
      />,
    );
    expect(container.querySelectorAll(".chart-band")).toHaveLength(1);
    expect(screen.getByText("Fault window")).toBeInTheDocument();
  });

  it("skips windows that have not been reached yet", () => {
    const { container } = render(
      <LineChart
        title="Throughput"
        unit="req/s"
        series={[{ label: "Completed", values: [1, 2, 3] }]}
        bands={[{ from: 20, to: 30, label: "Later" }]}
      />,
    );
    expect(container.querySelectorAll(".chart-band")).toHaveLength(0);
    expect(screen.queryByText("Fault window")).not.toBeInTheDocument();
  });
});

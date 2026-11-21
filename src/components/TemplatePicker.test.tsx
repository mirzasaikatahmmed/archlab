import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { TemplateRef } from "../bindings";
import { templates } from "../test/fixtures";
import { TemplatePicker } from "./TemplatePicker";

describe("TemplatePicker", () => {
  it("lists only templates compatible with the node kind", () => {
    render(
      <TemplatePicker
        nodeKind="database"
        templates={templates}
        template={null}
        settings={{}}
        onChange={vi.fn()}
      />,
    );
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["None", "PostgreSQL (v1)"]);
  });

  it("selects a template with its version and clears overrides", async () => {
    const onChange = vi.fn();
    render(
      <TemplatePicker
        nodeKind="appServer"
        templates={templates}
        template={null}
        settings={{}}
        onChange={onChange}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Config template"), "node");
    expect(onChange).toHaveBeenCalledWith({ id: "node", version: 1 }, {});
  });

  it("edits overrides and flags out-of-range values", async () => {
    function Harness() {
      const [settings, setSettings] = useState<Record<string, number>>({});
      const [template] = useState<TemplateRef>({ id: "node", version: 1 });
      return (
        <TemplatePicker
          nodeKind="appServer"
          templates={templates}
          template={template}
          settings={settings}
          onChange={(_t, s) => {
            setSettings(s);
          }}
        />
      );
    }
    render(<Harness />);
    const input = screen.getByLabelText("Instances");
    expect(input).toHaveAttribute("placeholder", "4");
    await userEvent.type(input, "5000");
    expect(await screen.findByRole("alert")).toHaveTextContent("Must be between 1 and 1024");
    await userEvent.clear(input);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

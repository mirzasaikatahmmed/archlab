import { describe, expect, it } from "vitest";
import { cargoVersion, mismatches } from "./check-version.mjs";

describe("version check", () => {
  it("reads the package version, not a dependency's", () => {
    const toml =
      '[package]\nname = "a"\nversion = "0.3.1"\n\n[dependencies]\nx = { version = "9.9.9" }\n';
    expect(cargoVersion(toml)).toBe("0.3.1");
  });

  it("accepts matching versions and a matching tag", () => {
    const same = { a: "0.1.0", b: "0.1.0" };
    expect(mismatches(same)).toEqual([]);
    expect(mismatches(same, "v0.1.0")).toEqual([]);
  });

  it("reports each file that disagrees and malformed tags", () => {
    expect(mismatches({ a: "0.1.0", b: "0.2.0" })).toEqual(["b is 0.2.0, expected 0.1.0"]);
    expect(mismatches({ a: "0.1.0" }, "v0.2.0")).toEqual(["a is 0.1.0, expected 0.2.0"]);
    expect(mismatches({ a: "1.0.0" }, "release-1")).toHaveLength(2);
  });
});

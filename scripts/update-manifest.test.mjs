import { describe, expect, it } from "vitest";
import { buildManifest, platformFiles } from "./update-manifest.mjs";

const signed = (version) =>
  Object.fromEntries(
    [...new Set(Object.values(platformFiles(version)))].map((f) => [f, `sig-${f}\n`]),
  );

describe("update manifest", () => {
  it("lists every platform with its signature and download url", () => {
    const m = buildManifest({
      tag: "v0.2.1",
      repo: "o/r",
      signatures: signed("0.2.1"),
      notes: "n",
      pubDate: "2026-01-01T00:00:00.000Z",
    });
    expect(m.version).toBe("0.2.1");
    expect(Object.keys(m.platforms)).toHaveLength(10);
    expect(m.platforms["linux-x86_64"]).toEqual({
      signature: "sig-ArchLab_0.2.1_amd64.AppImage",
      url: "https://github.com/o/r/releases/download/v0.2.1/ArchLab_0.2.1_amd64.AppImage",
    });
    expect(m.platforms["darwin-aarch64"].url).toContain("ArchLab_aarch64.app.tar.gz");
  });

  it("refuses to build when a platform was not signed", () => {
    const signatures = signed("0.2.1");
    delete signatures["ArchLab_0.2.1_x64-setup.exe"];
    expect(() =>
      buildManifest({ tag: "v0.2.1", repo: "o/r", signatures, notes: "", pubDate: "x" }),
    ).toThrow(/x64-setup\.exe/);
  });
});

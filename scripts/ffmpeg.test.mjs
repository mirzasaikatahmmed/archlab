import { describe, expect, it } from "vitest";
import { loadManifest, sha256, validateManifest } from "./ffmpeg.mjs";

const good = () => structuredClone(loadManifest());

describe("ffmpeg manifest", () => {
  it("is pinned, static and LGPL-only", () => {
    expect(() => {
      validateManifest(good());
    }).not.toThrow();
  });

  it("rejects the rolling tag, shared builds, GPL builds and bad checksums", () => {
    const rolling = good();
    rolling.tag = "latest";
    expect(() => {
      validateManifest(rolling);
    }).toThrow(/autobuild/);

    const gpl = good();
    gpl.builds["x86_64-pc-windows-msvc"].asset = "ffmpeg-n9.0-win64-gpl-9.0.zip";
    expect(() => {
      validateManifest(gpl);
    }).toThrow(/LGPL/);

    const shared = good();
    shared.builds["x86_64-unknown-linux-gnu"].asset = "ffmpeg-n9.0-linux64-lgpl-shared-9.0.tar.xz";
    expect(() => {
      validateManifest(shared);
    }).toThrow(/static/);

    const sum = good();
    sum.builds["x86_64-unknown-linux-gnu"].sha256 = "abc";
    expect(() => {
      validateManifest(sum);
    }).toThrow(/sha256/);
  });

  it("hashes bytes", () => {
    expect(sha256(Buffer.from("abc"))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });
});

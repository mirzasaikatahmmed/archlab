import { describe, expect, it, vi } from "vitest";
import {
  VIDEO,
  encodeVideo,
  frameCount,
  tickIndexAt,
  type FrameSource,
  type VideoSink,
} from "./video";

const source = (frames: number): FrameSource => ({
  frames,
  render: (i) => new Uint8Array([i]),
});

function sink(overrides: Partial<VideoSink> = {}) {
  const received: number[] = [];
  const s: VideoSink = {
    start: vi.fn(() => Promise.resolve()),
    frame: vi.fn((rgba: Uint8Array) => {
      received.push(...rgba);
      return Promise.resolve();
    }),
    finish: vi.fn(() => Promise.resolve(received.length)),
    cancel: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
  return { s, received };
}

describe("video timing", () => {
  it("plays one simulated second as one second of video", () => {
    expect(VIDEO).toEqual({ width: 1920, height: 1080, fps: 30 });
    expect(frameCount(new Array(12).fill(0))).toBe(12 * 30);
  });

  it("maps frames to the metrics second they belong to and holds the last one", () => {
    expect(tickIndexAt(0, 30, 10)).toBe(0);
    expect(tickIndexAt(29, 30, 10)).toBe(0);
    expect(tickIndexAt(30, 30, 10)).toBe(1);
    expect(tickIndexAt(10_000, 30, 10)).toBe(9);
    expect(tickIndexAt(5, 30, 0)).toBe(0);
  });
});

describe("encodeVideo", () => {
  it("sends every frame in order, reports progress and finishes", async () => {
    const { s, received } = sink();
    const progress = vi.fn();
    const outcome = await encodeVideo(source(4), s, progress, () => false);
    expect(received).toEqual([0, 1, 2, 3]);
    expect(progress).toHaveBeenLastCalledWith(4, 4);
    expect(outcome).toEqual({ status: "done", frames: 4 });
    expect(s.cancel).not.toHaveBeenCalled();
  });

  it("stops and closes the encoder when cancelled", async () => {
    const { s, received } = sink();
    let calls = 0;
    const outcome = await encodeVideo(
      source(10),
      s,
      () => undefined,
      () => {
        calls += 1;
        return calls > 3;
      },
    );
    expect(outcome).toEqual({ status: "cancelled" });
    expect(received).toEqual([0, 1, 2]);
    expect(s.cancel).toHaveBeenCalledTimes(1);
    expect(s.finish).not.toHaveBeenCalled();
  });

  it("closes the encoder and rethrows when a frame fails", async () => {
    const { s } = sink({ frame: vi.fn(() => Promise.reject(new Error("pipe closed"))) });
    await expect(
      encodeVideo(
        source(3),
        s,
        () => undefined,
        () => false,
      ),
    ).rejects.toThrow("pipe closed");
    expect(s.cancel).toHaveBeenCalledTimes(1);
  });
});

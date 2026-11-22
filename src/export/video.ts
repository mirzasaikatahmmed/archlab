import type { Fault, LiveTick, Slo, Topology } from "../bindings";
import { calm, conditionsFor, withEdgeLoss, type Conditions } from "../stage/conditions";
import { drawScene, readPalette } from "../stage/draw";
import { fitScene } from "../stage/iso";
import { sceneExtent, sceneNodes } from "../stage/layout";
import { loadsFrom } from "../stage/loads";
import { IDLE_LOAD, buildGraph, createWorld, stepWorld } from "../stage/particles";

/** 1080p at 30 frames per second; one simulated second plays as one second of video. */
export const VIDEO = { width: 1920, height: 1080, fps: 30 } as const;

/** A video longer than this would exceed the encoder's frame limit. */
export const MAX_VIDEO_SECONDS = 600;

const PADDING = { top: 190, right: 160, bottom: 170, left: 160 };

export function frameCount(ticks: readonly unknown[], fps: number = VIDEO.fps): number {
  return ticks.length * fps;
}

/** The metrics tick shown at `frame`: one per simulated second, holding the last at the end. */
export function tickIndexAt(frame: number, fps: number, tickCount: number): number {
  return Math.max(0, Math.min(tickCount - 1, Math.floor(frame / fps)));
}

export interface FrameSource {
  frames: number;
  /** RGBA pixels of one frame. Frames must be requested in order. */
  render: (index: number) => Uint8Array;
}

export interface VideoScene {
  title: string;
  topology: Topology;
  faults: Fault[];
  ticks: readonly LiveTick[];
  slo: Slo;
}

function tickAt(ticks: readonly LiveTick[], index: number): LiveTick | undefined {
  return ticks[index];
}

const num = (v: number | null | undefined) => v ?? 0;
const whole = (v: number | null | undefined) => String(Math.round(num(v)));

/**
 * Draws the Stage frame by frame from recorded metrics, off screen. The same drawing code
 * as the live view runs with a fixed time step, so the same run always gives the same video.
 */
export function createFrameSource(scene: VideoScene): FrameSource {
  const { width, height, fps } = VIDEO;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) {
    throw new Error("Drawing is not available in this window.");
  }
  const graph = buildGraph(scene.topology);
  const nodes = sceneNodes(scene.topology);
  const size = { w: width, h: height };
  const fit = fitScene(sceneExtent(nodes), size, PADDING, 2.5);
  const palette = readPalette(document.documentElement);
  const world = createWorld(1);
  let loads = graph.kinds.map(() => IDLE_LOAD);
  let conditions: Conditions = calm(graph);
  let dropped = new Map<string, number>();
  let shown = -1;
  let offered = 0;

  const overlay = (tick: LiveTick | undefined, index: number) => {
    // drawScene leaves its scene transform on the context; the overlay uses pixel coordinates.
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.save();
    ctx.globalCompositeOperation = "destination-over";
    ctx.fillStyle = palette.panel;
    ctx.fillRect(0, 0, width, height);
    ctx.restore();
    ctx.textAlign = "left";
    ctx.globalAlpha = 1;
    ctx.textBaseline = "alphabetic";
    ctx.fillStyle = palette.text;
    ctx.font = `600 44px ${palette.font}`;
    ctx.fillText(scene.title, 72, 92);
    ctx.fillStyle = palette.muted;
    ctx.font = `400 26px ${palette.font}`;
    const p = tick?.point;
    const line = p
      ? `${String(Math.min(scene.ticks.length, index + 1))} s of ${String(scene.ticks.length)} s   ${whole(p.offeredRps)} req/s offered   p95 ${whole(p.p95Ms)} ms   ${whole(p.errorRps)} errors/s`
      : "";
    ctx.fillText(line, 72, 140);
    ctx.font = `400 22px ${palette.font}`;
    ctx.fillText(
      `Targets: p95 at most ${whole(scene.slo.p95Ms)} ms, errors at most ${(num(scene.slo.maxErrorRate) * 100).toFixed(1)}%`,
      72,
      height - 96,
    );
    ctx.fillText("Estimate from a simulation model, not a measurement.", 72, height - 56);
  };

  return {
    frames: frameCount(scene.ticks, fps),
    render: (frame) => {
      const index = tickIndexAt(frame, fps, scene.ticks.length);
      const tick = tickAt(scene.ticks, index);
      if (index !== shown) {
        shown = index;
        conditions = conditionsFor(graph, scene.faults, tick?.activeFaults ?? []);
        loads = withEdgeLoss(loadsFrom(scene.topology, tick, dropped), graph, conditions);
        offered = num(tick?.point.offeredRps);
        dropped = new Map(tick?.nodes.map((n) => [n.id, n.dropped]));
      }
      stepWorld(world, graph, { offeredRps: offered, loads }, 1 / fps);
      drawScene({
        ctx,
        size,
        dpr: 1,
        fit,
        palette,
        nodes,
        graph,
        loads,
        world,
        time: frame / fps,
        motion: true,
        conditions,
        highlight: new Set(),
      });
      overlay(tick, index);
      const pixels = ctx.getImageData(0, 0, width, height).data;
      return new Uint8Array(pixels.buffer, pixels.byteOffset, pixels.byteLength);
    },
  };
}

/** The encoder on the other side of the pipe. */
export interface VideoSink {
  start: () => Promise<void>;
  frame: (rgba: Uint8Array) => Promise<void>;
  /** Returns the number of frames in the finished file. */
  finish: () => Promise<number>;
  cancel: () => Promise<void>;
}

export type VideoOutcome = { status: "done"; frames: number } | { status: "cancelled" };

/**
 * Sends every frame to the sink. A failure or a cancel closes the sink so no partial file is
 * left behind; a failure is rethrown.
 */
export async function encodeVideo(
  source: FrameSource,
  sink: VideoSink,
  onProgress: (done: number, total: number) => void,
  isCancelled: () => boolean,
): Promise<VideoOutcome> {
  await sink.start();
  try {
    for (let i = 0; i < source.frames; i += 1) {
      if (isCancelled()) {
        await sink.cancel();
        return { status: "cancelled" };
      }
      await sink.frame(source.render(i));
      onProgress(i + 1, source.frames);
    }
    return { status: "done", frames: await sink.finish() };
  } catch (err) {
    await sink.cancel();
    throw err;
  }
}

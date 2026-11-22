import { useEffect, useMemo, useRef, useState } from "react";
import type { Fault, LiveTick, Topology } from "../bindings";
import { calm, conditionsFor, withEdgeLoss, type Conditions } from "../stage/conditions";
import { drawScene, readPalette, type Palette } from "../stage/draw";
import { fitScene, type Size } from "../stage/iso";
import { sceneExtent, sceneNodes } from "../stage/layout";
import { loadsFrom } from "../stage/loads";
import {
  IDLE_LOAD,
  buildGraph,
  createWorld,
  stepWorld,
  type NodeLoad,
  type World,
} from "../stage/particles";

interface Props {
  topology: Topology;
  /** Latest per-second metrics; absent before a run starts. */
  latest: LiveTick | undefined;
  /** Dots keep flowing only while the run is going. */
  running: boolean;
  /** Faults in the project; the latest tick says which are in effect. */
  faults: Fault[];
  /** A live run is in progress (running or paused); faults are shown only then. */
  active: boolean;
  /** Ids of nodes to point at, for guided scenarios. */
  highlight?: string[];
}

const HEIGHT = 440;
const PADDING = { top: 36, right: 70, bottom: 64, left: 70 };

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-motion: reduce)").matches
    : false;
}

/**
 * Isometric view of the topology. Dots are sampled from per-second metrics, so
 * they show the shape of the traffic, not individual requests.
 */
export function Stage({ topology, latest, running, faults, active, highlight }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const graph = useMemo(() => buildGraph(topology), [topology]);
  const nodes = useMemo(() => sceneNodes(topology), [topology]);
  const [size, setSize] = useState<Size>({ w: 0, h: HEIGHT });
  const [reduced] = useState(prefersReducedMotion);

  const loadsRef = useRef<NodeLoad[]>([]);
  const conditionsRef = useRef<Conditions>(calm(graph));
  const highlightRef = useRef<ReadonlySet<number>>(new Set());
  const offeredRef = useRef(0);
  const droppedRef = useRef(new Map<string, number>());
  const runningRef = useRef(running);
  const worldRef = useRef<World>(createWorld(1));
  const redrawRef = useRef<() => void>(() => undefined);

  useEffect(() => {
    runningRef.current = running;
  }, [running]);

  useEffect(() => {
    highlightRef.current = new Set(
      (highlight ?? []).flatMap((id) => {
        const i = graph.nodeIds.indexOf(id);
        return i >= 0 ? [i] : [];
      }),
    );
    redrawRef.current();
  }, [highlight, graph]);

  useEffect(() => {
    const conditions = active
      ? conditionsFor(graph, faults, latest?.activeFaults ?? [])
      : calm(graph);
    conditionsRef.current = conditions;
    loadsRef.current = withEdgeLoss(
      loadsFrom(topology, latest, droppedRef.current),
      graph,
      conditions,
    );
    offeredRef.current = latest?.point.offeredRps ?? 0;
    droppedRef.current = new Map(latest?.nodes.map((n) => [n.id, n.dropped]));
    redrawRef.current();
  }, [topology, latest, graph, faults, active]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const observer = new ResizeObserver(() => {
      setSize({ w: canvas.clientWidth, h: HEIGHT });
    });
    observer.observe(canvas);
    setSize({ w: canvas.clientWidth, h: HEIGHT });
    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || size.w === 0) {
      return;
    }
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.w * dpr);
    canvas.height = Math.round(size.h * dpr);
    const fit = fitScene(sceneExtent(nodes), size, PADDING);
    let palette: Palette = readPalette(canvas);
    let frame = 0;
    let last = performance.now();
    let raf = 0;

    const draw = (time: number) => {
      const loads = graph.kinds.map((_, i) => loadsRef.current[i] ?? IDLE_LOAD);
      drawScene({
        ctx,
        size,
        dpr,
        fit,
        palette,
        nodes,
        graph,
        loads,
        world: worldRef.current,
        time,
        motion: !reduced,
        conditions: conditionsRef.current,
        highlight: highlightRef.current,
      });
    };
    redrawRef.current = () => {
      palette = readPalette(canvas);
      draw(performance.now() / 1000);
    };

    if (reduced) {
      draw(0);
      return () => {
        redrawRef.current = () => undefined;
      };
    }

    const loop = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      frame += 1;
      if (frame % 45 === 0) {
        palette = readPalette(canvas);
      }
      stepWorld(
        worldRef.current,
        graph,
        {
          offeredRps: runningRef.current ? offeredRef.current : 0,
          loads: graph.kinds.map((_, i) => loadsRef.current[i] ?? IDLE_LOAD),
        },
        dt,
      );
      draw(now / 1000);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      redrawRef.current = () => undefined;
    };
  }, [graph, nodes, size, reduced]);

  return (
    <figure className="stage">
      <canvas
        ref={canvasRef}
        style={{ height: HEIGHT }}
        role="img"
        aria-label="Isometric view of the topology. Each node shows how full it is; moving dots show traffic flowing between nodes."
      />
      <figcaption className="stage-legend">
        <span className="key" data-level="healthy">
          Healthy
        </span>
        <span className="key" data-level="warning">
          Busy
        </span>
        <span className="key" data-level="saturated">
          Saturated
        </span>
        <span className="muted">
          Dashed outline is full capacity. Dots are sampled from the metrics, not single requests.
        </span>
      </figcaption>
    </figure>
  );
}

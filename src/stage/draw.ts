import type { NodeKind } from "../bindings";
import { levelFor } from "../lib/severity";
import type { Conditions } from "./conditions";
import { project, type Fit, type Pt, type Size } from "./iso";
import { CEILING_HEIGHT, TILE, type SceneNode } from "./layout";
import { IDLE_LOAD, type NodeLoad, type StageGraph, type World } from "./particles";

export interface Palette {
  text: string;
  muted: string;
  line: string;
  lineStrong: string;
  panel: string;
  accent: string;
  ok: string;
  warn: string;
  bad: string;
  font: string;
}

const KIND_COLOR: Record<NodeKind, string> = {
  client: "#8e96a8",
  loadBalancer: "#5fa8ff",
  appServer: "#8b8dff",
  database: "#3cc596",
  cache: "#e6b13a",
  queue: "#e879b9",
  apiGateway: "#a3e635",
  cdn: "#22d3ee",
  worker: "#f97316",
};

export function readPalette(el: Element): Palette {
  const css = getComputedStyle(el);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return {
    text: v("--text", "#e8eaf0"),
    muted: v("--muted", "#8e96a8"),
    line: v("--line", "#262b36"),
    lineStrong: v("--line-strong", "#353c4a"),
    panel: v("--panel", "#13161d"),
    accent: v("--accent", "#8b8dff"),
    ok: v("--ok", "#3cc596"),
    warn: v("--warn", "#e6b13a"),
    bad: v("--bad", "#f06a6a"),
    font: css.fontFamily || "system-ui, sans-serif",
  };
}

export interface Frame {
  ctx: CanvasRenderingContext2D;
  size: Size;
  dpr: number;
  fit: Fit;
  palette: Palette;
  nodes: SceneNode[];
  graph: StageGraph;
  loads: NodeLoad[];
  world: World;
  /** Seconds, for gentle idle motion. */
  time: number;
  motion: boolean;
  conditions: Conditions;
  /** Indexes of nodes the narration is pointing at. */
  highlight: ReadonlySet<number>;
}

type Poly = Pt[];

const P = project;

function poly(ctx: CanvasRenderingContext2D, pts: Poly): void {
  ctx.beginPath();
  pts.forEach((p, i) => {
    if (i === 0) {
      ctx.moveTo(p.x, p.y);
    } else {
      ctx.lineTo(p.x, p.y);
    }
  });
  ctx.closePath();
}

/** Opaque backing so things behind a solid never show through, then a tint on top. */
function face(f: Frame, pts: Poly, color: string, alpha: number, edge = 0.9): void {
  const { ctx, palette } = f;
  poly(ctx, pts);
  ctx.globalAlpha = 1;
  ctx.fillStyle = palette.panel;
  ctx.fill();
  ctx.globalAlpha = alpha;
  ctx.fillStyle = color;
  ctx.fill();
  ctx.globalAlpha = edge;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.globalAlpha = 1;
}

function box(
  f: Frame,
  gx: number,
  gy: number,
  hw: number,
  hd: number,
  h: number,
  z0: number,
  color: string,
): void {
  const top = [
    P(gx - hw, gy - hd, z0 + h),
    P(gx + hw, gy - hd, z0 + h),
    P(gx + hw, gy + hd, z0 + h),
    P(gx - hw, gy + hd, z0 + h),
  ];
  const sideX = [
    P(gx + hw, gy - hd, z0),
    P(gx + hw, gy + hd, z0),
    P(gx + hw, gy + hd, z0 + h),
    P(gx + hw, gy - hd, z0 + h),
  ];
  const sideY = [
    P(gx - hw, gy + hd, z0),
    P(gx + hw, gy + hd, z0),
    P(gx + hw, gy + hd, z0 + h),
    P(gx - hw, gy + hd, z0 + h),
  ];
  face(f, sideY, color, 0.2);
  face(f, sideX, color, 0.34);
  face(f, top, color, 0.55);
}

function line(f: Frame, a: Pt, b: Pt, color: string, alpha: number, width = 1): void {
  const { ctx } = f;
  ctx.globalAlpha = alpha;
  ctx.strokeStyle = color;
  ctx.lineWidth = width;
  ctx.beginPath();
  ctx.moveTo(a.x, a.y);
  ctx.lineTo(b.x, b.y);
  ctx.stroke();
  ctx.globalAlpha = 1;
}

function dot(f: Frame, p: Pt, r: number, color: string, alpha = 1): void {
  const { ctx } = f;
  ctx.globalAlpha = alpha;
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalAlpha = 1;
}

function ring(
  gx: number,
  gy: number,
  r: number,
  z: number,
  from: number,
  to: number,
  steps = 18,
): Poly {
  const pts: Poly = [];
  for (let i = 0; i <= steps; i++) {
    const a = from + ((to - from) * i) / steps;
    pts.push(P(gx + Math.cos(a) * r, gy + Math.sin(a) * r, z));
  }
  return pts;
}

function cylinder(f: Frame, gx: number, gy: number, r: number, h: number, color: string): void {
  const front0 = -Math.PI / 4;
  const front1 = (3 * Math.PI) / 4;
  const body = [
    ...ring(gx, gy, r, 0, front0, front1),
    ...ring(gx, gy, r, h, front0, front1).reverse(),
  ];
  face(f, body, color, 0.3);
  for (const z of [h * 0.33, h * 0.66]) {
    const arc = ring(gx, gy, r, z, front0, front1);
    f.ctx.globalAlpha = 0.55;
    f.ctx.strokeStyle = color;
    f.ctx.beginPath();
    arc.forEach((p, i) => {
      if (i === 0) {
        f.ctx.moveTo(p.x, p.y);
      } else {
        f.ctx.lineTo(p.x, p.y);
      }
    });
    f.ctx.stroke();
    f.ctx.globalAlpha = 1;
  }
  face(f, ring(gx, gy, r, h, 0, Math.PI * 2, 28), color, 0.55);
}

/** `#rrggbb` with an alpha; anything else is returned unchanged. */
export function withAlpha(color: string, alpha: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color);
  if (!m) {
    return color;
  }
  const [r, g, b] = [m[1], m[2], m[3]].map((h) => parseInt(h, 16));
  return `rgba(${String(r)}, ${String(g)}, ${String(b)}, ${String(alpha)})`;
}

function status(f: Frame, load: NodeLoad): string {
  const level = levelFor(load.utilization);
  return level === "saturated"
    ? f.palette.bad
    : level === "warning"
      ? f.palette.warn
      : f.palette.ok;
}

function drawObject(f: Frame, node: SceneNode, load: NodeLoad): void {
  const color = KIND_COLOR[node.kind];
  const { gx, gy } = node;
  switch (node.kind) {
    case "appServer": {
      box(f, gx, gy, 15, 15, 58, 0, color);
      for (let i = 0; i < 5; i++) {
        const z = 8 + i * 10;
        line(f, P(gx - 12, gy + 15, z), P(gx + 12, gy + 15, z), color, 0.6);
        dot(f, P(gx + 9, gy + 15, z + 4), 1.3, status(f, load));
      }
      break;
    }
    case "database":
      cylinder(f, gx, gy, 17, 42, color);
      break;
    case "cache": {
      box(f, gx, gy, 18, 18, 8, 0, color);
      box(f, gx, gy, 8, 8, 3, 8, color);
      for (let i = 0; i < 5; i++) {
        const o = -12 + i * 6;
        line(f, P(gx + 18, gy + o, 2), P(gx + 24, gy + o, 2), color, 0.8);
        line(f, P(gx + o, gy + 18, 2), P(gx + o, gy + 24, 2), color, 0.8);
      }
      break;
    }
    case "loadBalancer": {
      box(f, gx, gy, 22, 12, 16, 0, color);
      line(f, P(gx - 14, gy, 16), P(gx + 12, gy - 6, 16), color, 0.9, 1.5);
      line(f, P(gx - 14, gy, 16), P(gx + 12, gy + 6, 16), color, 0.9, 1.5);
      line(f, P(gx - 14, gy, 16), P(gx + 12, gy, 16), color, 0.9, 1.5);
      break;
    }
    case "queue": {
      for (const o of [-16, 0, 16]) {
        box(f, gx + o, gy, 7, 14, 14, 0, color);
      }
      break;
    }
    case "apiGateway": {
      for (const o of [-15, 15]) {
        box(f, gx + o, gy, 4, 5, 30, 0, color);
      }
      box(f, gx, gy, 19, 5, 6, 30, color);
      dot(f, P(gx, gy + 5, 33), 1.6, status(f, load));
      line(f, P(gx - 11, gy + 5, 8), P(gx + 11, gy + 5, 8), color, 0.6);
      break;
    }
    case "cdn": {
      cylinder(f, gx, gy, 20, 9, color);
      box(f, gx, gy, 5, 5, 14, 9, color);
      for (const [dx, dy] of [
        [-14, -14],
        [14, -14],
        [14, 14],
        [-14, 14],
      ] as const) {
        line(f, P(gx + dx * 0.4, gy + dy * 0.4, 24), P(gx + dx, gy + dy, 30), color, 0.7, 1.2);
      }
      break;
    }
    case "worker": {
      box(f, gx, gy, 17, 15, 18, 0, color);
      for (const o of [-9, 9]) {
        box(f, gx + o, gy, 4, 4, 7, 18, color);
      }
      box(f, gx, gy, 5, 5, 11, 18, color);
      dot(f, P(gx + 11, gy + 15, 9), 1.5, status(f, load));
      break;
    }
    case "client":
      drawCrowd(f, node, load);
      break;
  }
}

/** A small crowd whose size follows the offered traffic. */
function drawCrowd(f: Frame, node: SceneNode, load: NodeLoad): void {
  const count = Math.round(6 + load.queue);
  for (let i = 0; i < count; i++) {
    const a = i * 2.399963;
    const r = 8 + Math.sqrt(i + 1) * 6.2;
    const gx = node.gx + Math.cos(a) * r;
    const gy = node.gy + Math.sin(a) * r;
    const bob = f.motion ? Math.sin(f.time * 3 + i * 1.7) * 1.1 : 0;
    const base = P(gx, gy, 0);
    f.ctx.globalAlpha = 0.25;
    f.ctx.fillStyle = f.palette.muted;
    f.ctx.beginPath();
    f.ctx.ellipse(base.x, base.y, 3.4, 1.5, 0, 0, Math.PI * 2);
    f.ctx.fill();
    f.ctx.globalAlpha = 0.85;
    f.ctx.strokeStyle = i % 7 === 0 ? f.palette.accent : f.palette.muted;
    f.ctx.lineWidth = 3;
    f.ctx.lineCap = "round";
    f.ctx.beginPath();
    const foot = P(gx, gy, 2 + bob);
    const neck = P(gx, gy, 8 + bob);
    f.ctx.moveTo(foot.x, foot.y);
    f.ctx.lineTo(neck.x, neck.y);
    f.ctx.stroke();
    f.ctx.fillStyle = f.ctx.strokeStyle;
    const head = P(gx, gy, 11.5 + bob);
    f.ctx.beginPath();
    f.ctx.arc(head.x, head.y, 2.6, 0, Math.PI * 2);
    f.ctx.fill();
    f.ctx.globalAlpha = 1;
  }
}

/** Translucent column that rises toward the dashed ceiling as utilization approaches 100%. */
function drawCapacity(f: Frame, node: SceneNode, load: NodeLoad): void {
  const { ctx } = f;
  const hw = TILE - 8;
  const color = status(f, load);
  const fill = Math.min(1, Math.max(0, load.utilization)) * CEILING_HEIGHT;
  if (fill > 1) {
    const { gx, gy } = node;
    const sideY = [
      P(gx - hw, gy + hw, 0),
      P(gx + hw, gy + hw, 0),
      P(gx + hw, gy + hw, fill),
      P(gx - hw, gy + hw, fill),
    ];
    const sideX = [
      P(gx + hw, gy - hw, 0),
      P(gx + hw, gy + hw, 0),
      P(gx + hw, gy + hw, fill),
      P(gx + hw, gy - hw, fill),
    ];
    for (const side of [sideY, sideX]) {
      poly(ctx, side);
      ctx.globalAlpha = 0.13;
      ctx.fillStyle = color;
      ctx.fill();
      ctx.globalAlpha = 0.55;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }
  const { gx, gy } = node;
  const ceiling = [
    P(gx - hw, gy - hw, CEILING_HEIGHT),
    P(gx + hw, gy - hw, CEILING_HEIGHT),
    P(gx + hw, gy + hw, CEILING_HEIGHT),
    P(gx - hw, gy + hw, CEILING_HEIGHT),
  ];
  ctx.setLineDash([2, 4]);
  ctx.strokeStyle = f.palette.lineStrong;
  ctx.lineWidth = 1;
  ctx.globalAlpha = 0.7;
  for (const [cx, cy] of [
    [gx - hw, gy - hw],
    [gx + hw, gy - hw],
    [gx + hw, gy + hw],
    [gx - hw, gy + hw],
  ] as const) {
    const base = P(cx, cy, 0);
    const top = P(cx, cy, CEILING_HEIGHT);
    ctx.beginPath();
    ctx.moveTo(base.x, base.y);
    ctx.lineTo(top.x, top.y);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  poly(ctx, ceiling);
  ctx.setLineDash([5, 4]);
  ctx.strokeStyle = load.utilization >= 0.9 ? f.palette.bad : f.palette.lineStrong;
  ctx.lineWidth = 1.2;
  ctx.stroke();
  ctx.setLineDash([]);
}

function drawQueue(f: Frame, node: SceneNode, load: NodeLoad): void {
  if (node.kind === "client") {
    return;
  }
  const count = Math.min(12, Math.round(load.queue));
  const color = status(f, load);
  for (let i = 0; i < count; i++) {
    const row = i % 4;
    const level = Math.floor(i / 4);
    box(f, node.gx - TILE - 6 - level * 8, node.gy - 12 + row * 8, 3.5, 3.5, 4, 0, color);
  }
}

function drawFloor(f: Frame): void {
  const { ctx, nodes, palette } = f;
  const xs = nodes.map((n) => n.gx);
  const ys = nodes.map((n) => n.gy);
  const x0 = Math.floor((Math.min(...xs) - 140) / 80) * 80;
  const x1 = Math.ceil((Math.max(...xs) + 140) / 80) * 80;
  const y0 = Math.floor((Math.min(...ys) - 140) / 80) * 80;
  const y1 = Math.ceil((Math.max(...ys) + 140) / 80) * 80;
  ctx.strokeStyle = palette.line;
  ctx.lineWidth = 0.6;
  ctx.globalAlpha = 0.7;
  ctx.beginPath();
  for (let x = x0; x <= x1; x += 80) {
    const a = P(x, y0);
    const b = P(x, y1);
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
  }
  for (let y = y0; y <= y1; y += 80) {
    const a = P(x0, y);
    const b = P(x1, y);
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
  }
  ctx.stroke();
  ctx.globalAlpha = 1;
}

function drawEdges(f: Frame): void {
  const { graph, nodes } = f;
  for (const e of graph.edges) {
    const a = nodes[e.from];
    const b = nodes[e.to];
    const from = P(a.gx, a.gy, 1);
    const to = P(b.gx, b.gy, 1);
    const cond = f.conditions.edges[graph.edges.indexOf(e)];
    const color =
      cond.loss > 0 ? f.palette.bad : cond.extraMs > 0 ? f.palette.warn : f.palette.lineStrong;
    if (cond.loss > 0 || cond.extraMs > 0) {
      f.ctx.setLineDash([6, 5]);
    }
    line(f, from, to, color, 0.95, cond.loss > 0 || cond.extraMs > 0 ? 2 : 1.4);
    f.ctx.setLineDash([]);
    const mid = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
    if (cond.loss >= 1) {
      line(
        f,
        { x: mid.x - 6, y: mid.y - 6 },
        { x: mid.x + 6, y: mid.y + 6 },
        f.palette.bad,
        1,
        2.5,
      );
      line(
        f,
        { x: mid.x - 6, y: mid.y + 6 },
        { x: mid.x + 6, y: mid.y - 6 },
        f.palette.bad,
        1,
        2.5,
      );
    }
    const angle = Math.atan2(to.y - from.y, to.x - from.x);
    const tip = { x: mid.x + Math.cos(angle) * 5, y: mid.y + Math.sin(angle) * 5 };
    const left = { x: mid.x + Math.cos(angle + 2.5) * 5, y: mid.y + Math.sin(angle + 2.5) * 5 };
    const right = { x: mid.x + Math.cos(angle - 2.5) * 5, y: mid.y + Math.sin(angle - 2.5) * 5 };
    poly(f.ctx, [tip, left, right]);
    f.ctx.fillStyle = color;
    f.ctx.fill();
  }
}

/** Greys out a crashed node and marks its tile with a red cross. */
function drawDown(f: Frame, node: SceneNode): void {
  const { gx, gy } = node;
  const hw = 22;
  const h = 66;
  const faces: Poly[] = [
    [
      P(gx - hw, gy + hw, 0),
      P(gx + hw, gy + hw, 0),
      P(gx + hw, gy + hw, h),
      P(gx - hw, gy + hw, h),
    ],
    [
      P(gx + hw, gy - hw, 0),
      P(gx + hw, gy + hw, 0),
      P(gx + hw, gy + hw, h),
      P(gx + hw, gy - hw, h),
    ],
    [
      P(gx - hw, gy - hw, h),
      P(gx + hw, gy - hw, h),
      P(gx + hw, gy + hw, h),
      P(gx - hw, gy + hw, h),
    ],
  ];
  for (const pts of faces) {
    poly(f.ctx, pts);
    f.ctx.globalAlpha = 0.72;
    f.ctx.fillStyle = f.palette.panel;
    f.ctx.fill();
  }
  f.ctx.globalAlpha = 1;
  line(f, P(gx - 20, gy - 20, 2), P(gx + 20, gy + 20, 2), f.palette.bad, 1, 3);
  line(f, P(gx - 20, gy + 20, 2), P(gx + 20, gy - 20, 2), f.palette.bad, 1, 3);
}

function drawTile(f: Frame, node: SceneNode, load: NodeLoad): void {
  const { ctx } = f;
  const color = status(f, load);
  const centre = P(node.gx, node.gy, 0);
  if (load.utilization > 0.3) {
    const g = ctx.createRadialGradient(centre.x, centre.y, 4, centre.x, centre.y, 78);
    g.addColorStop(0, withAlpha(color, 1));
    g.addColorStop(1, withAlpha(color, 0));
    ctx.globalAlpha = Math.min(0.4, (load.utilization - 0.3) * 0.7);
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.ellipse(centre.x, centre.y, 78, 44, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
  }
  const t = TILE - 2;
  const tile = [
    P(node.gx - t, node.gy - t),
    P(node.gx + t, node.gy - t),
    P(node.gx + t, node.gy + t),
    P(node.gx - t, node.gy + t),
  ];
  poly(ctx, tile);
  ctx.globalAlpha = 0.55;
  ctx.fillStyle = f.palette.panel;
  ctx.fill();
  ctx.globalAlpha = 0.9;
  ctx.strokeStyle = node.kind === "client" ? f.palette.lineStrong : color;
  ctx.lineWidth = 1.2;
  ctx.stroke();
  ctx.globalAlpha = 1;
  const index = f.nodes.indexOf(node);
  if (f.highlight.has(index)) {
    const r = t + 14;
    poly(ctx, [
      P(node.gx - r, node.gy - r),
      P(node.gx + r, node.gy - r),
      P(node.gx + r, node.gy + r),
      P(node.gx - r, node.gy + r),
    ]);
    ctx.globalAlpha = f.motion ? 0.55 + 0.4 * Math.sin(f.time * 3) : 0.9;
    ctx.strokeStyle = f.palette.accent;
    ctx.lineWidth = 3;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }
  const cond = f.conditions.nodes[index];
  if (cond.slowFactor > 1 || cond.lostFraction > 0) {
    const r = t + 7;
    poly(ctx, [
      P(node.gx - r, node.gy - r),
      P(node.gx + r, node.gy - r),
      P(node.gx + r, node.gy + r),
      P(node.gx - r, node.gy + r),
    ]);
    ctx.globalAlpha = f.motion ? 0.55 + 0.35 * Math.sin(f.time * 5) : 0.8;
    ctx.strokeStyle = f.palette.warn;
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }
}

function drawParticles(f: Frame): void {
  const { graph, nodes, world, palette } = f;
  for (const p of world.particles) {
    const e = graph.edges[p.edge];
    const a = nodes[e.from];
    const b = nodes[e.to];
    const t = p.t * p.t * (3 - 2 * p.t);
    const pos = P(a.gx + (b.gx - a.gx) * t, a.gy + (b.gy - a.gy) * t, 7);
    dot(f, pos, 2.6, palette.accent, p.hold > 0 ? 0.55 : 0.95);
  }
  for (const s of world.sparks) {
    const n = nodes[s.node];
    dot(f, P(n.gx + s.x, n.gy + s.y, s.z), 2.2, palette.bad, Math.min(1, s.life / 0.5));
  }
}

function drawLabels(f: Frame): void {
  const { ctx, nodes, loads, palette, fit, dpr } = f;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.textAlign = "center";
  ctx.textBaseline = "alphabetic";
  nodes.forEach((n, i) => {
    const anchor = P(n.gx + TILE, n.gy + TILE, 0);
    const x = anchor.x * fit.scale + fit.offset.x - TILE * 0.4 * fit.scale;
    const y = anchor.y * fit.scale + fit.offset.y + 16;
    ctx.font = `600 12px ${palette.font}`;
    ctx.fillStyle = palette.text;
    ctx.fillText(n.label, x, y);
    const cond = f.conditions.nodes[i];
    if (n.kind !== "client") {
      const load = loads[i] ?? IDLE_LOAD;
      ctx.font = `11px ${palette.font}`;
      ctx.fillStyle = cond.down ? palette.bad : palette.muted;
      ctx.fillText(
        cond.down ? "Down" : `${String(Math.round(load.utilization * 100))}% busy`,
        x,
        y + 14,
      );
    }
    const notes = [
      cond.slowFactor > 1 ? `${String(+cond.slowFactor.toFixed(1))}× slower` : "",
      cond.lostFraction > 0 ? `${String(Math.round(cond.lostFraction * 100))}% workers lost` : "",
    ].filter(Boolean);
    ctx.fillStyle = palette.warn;
    notes.forEach((note, k) => {
      ctx.fillText(note, x, y + 28 + k * 13);
    });
  });
  f.graph.edges.forEach((e, i) => {
    const cond = f.conditions.edges[i];
    const text = [
      cond.extraMs > 0 ? `+${String(Math.round(cond.extraMs))} ms` : "",
      cond.loss > 0 ? `${String(Math.round(cond.loss * 100))}% lost` : "",
    ]
      .filter(Boolean)
      .join(" · ");
    if (text) {
      const a = P(nodes[e.from].gx, nodes[e.from].gy, 1);
      const b = P(nodes[e.to].gx, nodes[e.to].gy, 1);
      ctx.font = `600 11px ${palette.font}`;
      ctx.fillStyle = cond.loss > 0 ? palette.bad : palette.warn;
      ctx.fillText(
        text,
        ((a.x + b.x) / 2) * fit.scale + fit.offset.x,
        ((a.y + b.y) / 2) * fit.scale + fit.offset.y - 12,
      );
    }
  });
}

export function drawScene(f: Frame): void {
  const { ctx, size, dpr, fit, nodes, loads } = f;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, size.w, size.h);
  ctx.setTransform(dpr * fit.scale, 0, 0, dpr * fit.scale, dpr * fit.offset.x, dpr * fit.offset.y);
  drawFloor(f);
  drawEdges(f);
  const order = nodes.map((n, i) => ({ n, i })).sort((a, b) => a.n.gx + a.n.gy - (b.n.gx + b.n.gy));
  for (const { n, i } of order) {
    drawTile(f, n, loads[i] ?? IDLE_LOAD);
  }
  drawParticles(f);
  for (const { n, i } of order) {
    const load = loads[i] ?? IDLE_LOAD;
    if (n.kind !== "client") {
      drawCapacity(f, n, load);
    }
    drawObject(f, n, load);
    if (f.conditions.nodes[i].down) {
      drawDown(f, n);
    }
    drawQueue(f, n, load);
  }
  drawLabels(f);
}

/** Isometric projection: ground x runs down-right, ground y down-left, z is up. */

const COS30 = Math.cos(Math.PI / 6);

export interface Pt {
  x: number;
  y: number;
}

export interface Size {
  w: number;
  h: number;
}

export interface Padding {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export function project(gx: number, gy: number, gz = 0): Pt {
  return { x: (gx - gy) * COS30, y: (gx + gy) * 0.5 - gz };
}

export interface Fit {
  scale: number;
  offset: Pt;
}

/** Scale and offset that centre `points` inside `size`, leaving `pad` free on every side. */
export function fitScene(points: Pt[], size: Size, pad: Padding, maxScale = 1.6): Fit {
  if (points.length === 0 || size.w <= 0 || size.h <= 0) {
    return { scale: 1, offset: { x: size.w / 2, y: size.h / 2 } };
  }
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const minX = Math.min(...xs) - pad.left;
  const maxX = Math.max(...xs) + pad.right;
  const minY = Math.min(...ys) - pad.top;
  const maxY = Math.max(...ys) + pad.bottom;
  const scale = Math.min(maxScale, size.w / (maxX - minX), size.h / (maxY - minY));
  return {
    scale,
    offset: {
      x: (size.w - (maxX + minX) * scale) / 2,
      y: (size.h - (maxY + minY) * scale) / 2,
    },
  };
}

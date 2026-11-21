import { z } from "zod";
import type { TrafficProfile } from "../bindings";

const num = (min: number, max: number) => z.number({ error: "Enter a number" }).min(min).max(max);
const seconds = num(0.001, 86_400);

export const spikeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("none") }),
  z.object({
    kind: z.literal("step"),
    atS: num(0, 86_400),
    multiplier: num(1, 100),
    durationS: seconds,
  }),
  z
    .object({
      kind: z.literal("periodic"),
      periodS: seconds,
      multiplier: num(1, 100),
      durationS: seconds,
    })
    .refine((s) => s.durationS < s.periodS, {
      path: ["durationS"],
      message: "Must be shorter than the period",
    }),
]);

/** Mirrors `TrafficProfile::validate` in Rust (ranges are checked against src/schemas/limits.json). */
export const trafficProfileSchema = z.object({
  concurrentUsers: z
    .number({ error: "Enter a number" })
    .int("Must be a whole number")
    .min(1)
    .max(100_000_000),
  requestsPerSecond: num(0.001, 10_000_000),
  readRatio: num(0, 1),
  spike: spikeSchema,
});

export type TrafficProfileValues = z.infer<typeof trafficProfileSchema>;
export type SpikeKind = TrafficProfileValues["spike"]["kind"];

export const toTrafficProfile = (values: TrafficProfileValues): TrafficProfile => values;

export function defaultSpike(kind: SpikeKind): TrafficProfileValues["spike"] {
  switch (kind) {
    case "none":
      return { kind };
    case "step":
      return { kind, atS: 60, multiplier: 3, durationS: 30 };
    case "periodic":
      return { kind, periodS: 300, multiplier: 2, durationS: 30 };
  }
}

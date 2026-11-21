import { z } from "zod";
import type { ServerProfile } from "../bindings";

const int = (min: number, max: number) =>
  z.number({ error: "Enter a number" }).int("Must be a whole number").min(min).max(max);
const num = (min: number, max: number) => z.number({ error: "Enter a number" }).min(min).max(max);

/** Mirrors `ServerProfile::validate` in Rust (ranges are checked against src/schemas/limits.json). */
export const serverProfileSchema = z.object({
  os: z.enum(["linux", "macOs", "windows", "other"]),
  cpuCores: int(1, 1024),
  cpuGhz: num(0.1, 10),
  ramGb: num(0.25, 16_384),
  disk: z.object({
    kind: z.enum(["hdd", "ssd", "nvme"]),
    iops: int(1, 10_000_000),
  }),
  network: z.object({
    bandwidthMbps: num(1, 1_000_000),
    latencyMs: num(0, 10_000),
  }),
  virtualization: z.enum(["bareMetal", "vm", "container"]),
  fdLimit: int(16, 10_000_000),
});

export type ServerProfileValues = z.infer<typeof serverProfileSchema>;

// Compile-time guard: form values must be assignable to the generated Rust type.
export const toServerProfile = (values: ServerProfileValues): ServerProfile => values;

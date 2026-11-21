import { describe, expect, it } from "vitest";
import { CommandError, describeError, unwrap, type AppError } from ".";

describe("unwrap", () => {
  it("returns data on ok", async () => {
    await expect(unwrap(Promise.resolve({ status: "ok" as const, data: 42 }))).resolves.toBe(42);
  });

  it("throws CommandError with validation issues on error", async () => {
    const error: AppError = {
      kind: "validation",
      issues: [{ field: "cpuCores", message: "must be between 1 and 1024" }],
    };
    const err = await unwrap(Promise.resolve({ status: "error" as const, error })).catch(
      (e: unknown) => e,
    );
    expect(err).toBeInstanceOf(CommandError);
    expect((err as CommandError).issues).toEqual(error.issues);
    expect((err as CommandError).message).toContain("cpuCores");
  });
});

describe("describeError", () => {
  it("describes each error kind", () => {
    expect(describeError({ kind: "notFound", id: "x" })).toBe("Not found: x");
    expect(describeError({ kind: "storage", message: "disk full" })).toBe("disk full");
  });
});

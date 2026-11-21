/**
 * specta types every Rust float as `number | null` (NaN/Infinity serialize to null).
 * Validated profiles never contain those, so UI code works with plain numbers.
 */
export type NoNull<T> = T extends null
  ? number
  : T extends readonly (infer U)[]
    ? NoNull<U>[]
    : T extends object
      ? { [K in keyof T]: NoNull<T[K]> }
      : T;

/** Replaces `null` numbers with NaN (which zod rejects) so forms never see null. */
export function stripNulls<T>(value: T): NoNull<T> {
  if (value === null) {
    return Number.NaN as NoNull<T>;
  }
  if (Array.isArray(value)) {
    return value.map((v: unknown) => stripNulls(v)) as NoNull<T>;
  }
  if (typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [k, stripNulls(v)]),
    ) as NoNull<T>;
  }
  return value as NoNull<T>;
}

export function last<T>(items: readonly T[]): T | undefined {
  return items[items.length - 1];
}

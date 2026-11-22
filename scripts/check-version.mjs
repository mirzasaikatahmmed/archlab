// Fails unless package.json, tauri.conf.json and Cargo.toml agree on the version, and, when a
// tag such as v0.2.0 is given, unless it matches too. Usage: node scripts/check-version.mjs [tag]
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export function cargoVersion(toml) {
  const section = toml.split(/^\[/m).find((s) => s.startsWith("package]"));
  const match = section?.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("no version in the [package] section of Cargo.toml");
  }
  return match[1];
}

/** Returns the list of mismatches; empty when all agree. */
export function mismatches(versions, tag) {
  const entries = Object.entries(versions);
  const expected = tag ? tag.replace(/^v/, "") : entries[0][1];
  const problems = entries
    .filter(([, v]) => v !== expected)
    .map(([name, v]) => `${name} is ${v}, expected ${expected}`);
  if (tag && !/^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(tag)) {
    problems.push(`tag ${tag} is not of the form vMAJOR.MINOR.PATCH`);
  }
  return problems;
}

function main() {
  const read = (...p) => readFileSync(join(root, ...p), "utf8");
  const versions = {
    "package.json": JSON.parse(read("package.json")).version,
    "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri", "tauri.conf.json")).version,
    "src-tauri/Cargo.toml": cargoVersion(read("src-tauri", "Cargo.toml")),
  };
  const problems = mismatches(versions, process.argv[2]);
  if (problems.length > 0) {
    console.error(`Version mismatch:\n- ${problems.join("\n- ")}`);
    process.exit(1);
  }
  console.log(`Version ${Object.values(versions)[0]} is consistent.`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}

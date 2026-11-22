// Fetches the pinned LGPL ffmpeg build for one target into src-tauri/binaries/, where Tauri
// picks it up as the `archlab-ffmpeg` sidecar. Usage: node scripts/ffmpeg.mjs [target-triple]
// Release builds only; see docs/adr/ADR-012-packaging-and-updates.md.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const SIDECAR = "archlab-ffmpeg";

export function loadManifest(path = join(here, "ffmpeg-manifest.json")) {
  return JSON.parse(readFileSync(path, "utf8"));
}

/** Throws unless every build is a pinned, LGPL-only build. */
export function validateManifest(manifest) {
  const problems = [];
  if (!/^autobuild-\d{4}-\d{2}-\d{2}-\d{2}-\d{2}$/.test(manifest.tag)) {
    problems.push("tag must be a dated autobuild tag, never the rolling 'latest'");
  }
  const builds = Object.entries(manifest.builds);
  if (builds.length === 0) {
    problems.push("no builds listed");
  }
  for (const [target, build] of builds) {
    if (!/^[0-9a-f]{64}$/.test(build.sha256)) {
      problems.push(`${target}: sha256 must be 64 hex characters`);
    }
    if (!/-lgpl-/.test(build.asset) || /shared/.test(build.asset)) {
      problems.push(`${target}: asset must be a static LGPL build, got ${build.asset}`);
    }
    if (/gpl/.test(build.asset.replace(/lgpl/g, ""))) {
      problems.push(`${target}: GPL builds are not allowed`);
    }
  }
  if (problems.length > 0) {
    throw new Error(`ffmpeg manifest is not acceptable:\n- ${problems.join("\n- ")}`);
  }
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = out.split("\n").find((l) => l.startsWith("host:"));
  if (!line) {
    throw new Error("could not read the host target from `rustc -vV`");
  }
  return line.slice("host:".length).trim();
}

function findFile(dir, relative) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      const inside = join(full, relative);
      if (existsSync(inside)) {
        return inside;
      }
    }
  }
  throw new Error(`${relative} not found in the archive`);
}

async function main() {
  const manifest = loadManifest();
  validateManifest(manifest);
  const target = process.argv[2] ?? hostTriple();
  const build = manifest.builds[target];
  if (!build) {
    throw new Error(
      `no pinned ffmpeg build for ${target}; builds exist for: ${Object.keys(manifest.builds).join(", ")}`,
    );
  }
  const url = `https://github.com/${manifest.repo}/releases/download/${manifest.tag}/${build.asset}`;
  console.log(`Downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`download failed: HTTP ${String(response.status)}`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  const actual = sha256(bytes);
  if (actual !== build.sha256) {
    throw new Error(
      `checksum mismatch for ${build.asset}: expected ${build.sha256}, got ${actual}`,
    );
  }

  const work = mkdtempSync(join(tmpdir(), "archlab-ffmpeg-"));
  try {
    const archive = join(work, build.asset);
    writeFileSync(archive, bytes);
    execFileSync("tar", ["-xf", archive, "-C", work], { stdio: "inherit" });
    const binary = findFile(work, build.binary);
    const license = findFile(work, "LICENSE.txt");

    const out = join(root, "src-tauri", "binaries");
    mkdirSync(out, { recursive: true });
    const suffix = target.includes("windows") ? ".exe" : "";
    const dest = join(out, `${SIDECAR}-${target}${suffix}`);
    copyFileSync(binary, dest);
    chmodSync(dest, 0o755);
    copyFileSync(license, join(out, "FFMPEG-LICENSE.txt"));

    if (target === hostTriple()) {
      const encoders = execFileSync(dest, ["-hide_banner", "-encoders"], { encoding: "utf8" });
      if (!encoders.includes("libvpx-vp9")) {
        throw new Error("this ffmpeg build has no libvpx-vp9 encoder");
      }
      console.log("Checked: the build has the libvpx-vp9 encoder.");
    }
    console.log(`Wrote ${dest}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  });
}

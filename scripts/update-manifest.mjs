// Builds the update manifest (latest.json) from the signed installers of a release, so it always
// lists every platform. tauri-action cannot do this: each build job writes its own latest.json
// at the same time, and the last writer wins with a single platform.
// Usage: node scripts/update-manifest.mjs <tag> <owner/repo> <dir with .sig files> <out file> [notes]
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** The signed file behind each updater platform key, for a version such as "0.2.1". */
export function platformFiles(version) {
  const appImage = `ArchLab_${version}_amd64.AppImage`;
  const nsis = `ArchLab_${version}_x64-setup.exe`;
  const mac = "ArchLab_aarch64.app.tar.gz";
  const macIntel = "ArchLab_x64.app.tar.gz";
  return {
    "darwin-aarch64": mac,
    "darwin-aarch64-app": mac,
    "darwin-x86_64": macIntel,
    "darwin-x86_64-app": macIntel,
    "linux-x86_64": appImage,
    "linux-x86_64-appimage": appImage,
    "linux-x86_64-deb": `ArchLab_${version}_amd64.deb`,
    "windows-x86_64": nsis,
    "windows-x86_64-nsis": nsis,
    "windows-x86_64-msi": `ArchLab_${version}_x64_en-US.msi`,
  };
}

/** `signatures` maps a file name to the contents of its `.sig` file. Throws if one is missing. */
export function buildManifest({ tag, repo, signatures, notes, pubDate }) {
  const version = tag.replace(/^v/, "");
  const files = platformFiles(version);
  const missing = [...new Set(Object.values(files))].filter((f) => !(f in signatures));
  if (missing.length > 0) {
    throw new Error(`no signature for: ${missing.join(", ")}`);
  }
  const platforms = Object.fromEntries(
    Object.entries(files).map(([key, file]) => [
      key,
      {
        signature: signatures[file].trim(),
        url: `https://github.com/${repo}/releases/download/${tag}/${file}`,
      },
    ]),
  );
  return { version, notes, pub_date: pubDate, platforms };
}

function main() {
  const [tag, repo, dir, out, notes = ""] = process.argv.slice(2);
  if (!tag || !repo || !dir || !out) {
    console.error("usage: update-manifest.mjs <tag> <owner/repo> <sig dir> <out file> [notes]");
    process.exit(2);
  }
  const signatures = Object.fromEntries(
    readdirSync(dir)
      .filter((f) => f.endsWith(".sig"))
      .map((f) => [f.slice(0, -4), readFileSync(join(dir, f), "utf8")]),
  );
  const manifest = buildManifest({
    tag,
    repo,
    signatures,
    notes,
    pubDate: new Date().toISOString(),
  });
  writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Wrote ${out}: ${Object.keys(manifest.platforms).length} platforms`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}

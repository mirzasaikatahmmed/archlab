# ADR-012: Packaging, signing, updates and the ffmpeg sidecar

- Status: Accepted
- Date: 2026-11-22

## Context

Phase 10 turns the app into installers people can download, keep up to date and trust, and
settles how ffmpeg (ADR-011) ships.

## Decision

1. **Release flow.** Pushing a tag `vMAJOR.MINOR.PATCH` runs `.github/workflows/release.yml`. It
   first checks that the tag, `package.json`, `tauri.conf.json` and `Cargo.toml` agree
   (`scripts/check-version.mjs`, also run in CI), then builds with `tauri-action` on four runners
   and uploads the installers to a **draft** GitHub Release; a final `manifest` job then builds
   `latest.json` from their signatures (`scripts/update-manifest.mjs`), because the four build
   jobs would otherwise overwrite each other's copy and leave one platform in it.
   A person publishes the draft. Targets: macOS Apple silicon and Intel (`.dmg`, `.app`; two
   separate builds, not a universal one, so no sidecar has to be merged with `lipo`), Windows x64
   (`.exe` NSIS, `.msi`), Linux x64 (`.deb`, `.AppImage`).
2. **Signing is optional and secret-driven.** Nothing is signed unless the secrets exist, and no
   secret is ever in the repository.
   - macOS: `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`,
     `APPLE_SIGNING_IDENTITY`, and for notarization `APPLE_ID`, `APPLE_PASSWORD` (an
     app-specific password) and `APPLE_TEAM_ID`.
   - Windows: `WINDOWS_CERTIFICATE` (base64 .pfx) and `WINDOWS_CERTIFICATE_PASSWORD`. This path is
     written from Tauri's documentation and has not been run.
   - Without them the build is unsigned: macOS asks the user to approve it in System Settings,
     Windows SmartScreen shows a warning.
3. **Auto-update** uses `tauri-plugin-updater` with GitHub Releases as the server
   (`.../releases/latest/download/latest.json`). Update packages are signed with a separate
   Tauri keypair: the public key is in `tauri.conf.json`, the private key is the secret
   `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). The updater verifies
   the signature before installing, so an unsigned or tampered package is refused regardless of
   OS code signing. `createUpdaterArtifacts` lives only in `tauri.release.conf.json`, so local
   builds do not need the private key.
   - The app checks at startup unless the person turns that off, and on request. It makes no other
     network calls, and keeps no telemetry. A failed startup check is silent; a failed manual
     check says why. It installs on request and asks the person to restart.
   - **A private repository cannot serve updates** to anonymous clients, so checks fail until the
     repository (or another host for `latest.json`) is public.
4. **Plugin versions.** The newest updater and dialog plugins require `tauri` 2.12; the project
   is on 2.11 with matching JS packages. `tauri-plugin-updater` is pinned to 2.12.0 (and its JS
   package to 2.12.0), the newest release that resolves with `tauri` 2.11.6; tauri itself is
   unchanged. Revisit when the app moves to tauri 2.12.
5. **ffmpeg sidecar (Windows and Linux).** The release workflow runs `scripts/ffmpeg.mjs`, which
   downloads a pinned **LGPL, static** build from BtbN/FFmpeg-Builds (a month-end autobuild tag
   and sha256 in `scripts/ffmpeg-manifest.json`; the rolling `latest` tag, GPL builds and shared
   builds are rejected by a test), checks the hash before unpacking, and on the host platform
   checks that `libvpx-vp9` is present. Tauri bundles it from `tauri.ffmpeg.conf.json` as
   `archlab-ffmpeg` (not `ffmpeg`, so a `.deb` never overwrites the distribution's
   `/usr/bin/ffmpeg`), together with the licence text. The app finds it next to its executable.
   I checked the pinned Linux build: LGPL v3 or later (`--enable-version3`, no `--enable-gpl`,
   no x264 or x265), `--enable-libvpx`, and the download matches the published checksum.
   The Windows build comes from the same builder and checksum list; its contents are checked in
   the release workflow.
6. **ffmpeg on macOS is not bundled.** I found no LGPL static macOS build I could verify. The
   app looks on `PATH` and in `/opt/homebrew/bin` and `/usr/local/bin` (a Finder-launched app does
   not inherit the shell's `PATH`), so `brew install ffmpeg` works. Bundling one later means adding
   two entries to the manifest.

## Consequences

- A static ffmpeg is about 140 MB on disk; installers grow by roughly a third to a half of that
  after compression. Windows and Linux installers are therefore much larger than macOS ones.
- LGPL compliance: ship the licence text (done, as a resource), say where the source is, and do not
  prevent replacing the program. Replacing works by pointing `ARCHLAB_FFMPEG` at another ffmpeg.
  The README names the version and source.
- The release workflow cannot be fully tested without pushing a tag: the first run is the test, and
  it produces only a draft.

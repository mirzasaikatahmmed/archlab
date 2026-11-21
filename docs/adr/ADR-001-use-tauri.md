# ADR-001: Use Tauri v2 with a Rust core

- Status: Accepted
- Date: 2026-11-21

## Context

ArchLab must be a single installable, fully local desktop app for Windows, macOS and Linux with no
separate backend and no runtime dependencies for end users. It runs a deterministic simulation
engine, tokio-based in-process components, bundled SQLite, and streams raw video frames to a
bundled ffmpeg sidecar. Installer size and memory use matter, and the UI needs a rich canvas and
SVG charts.

## Decision

Use **Tauri v2**: a Rust core (`src-tauri/`) plus a Vite + React + TypeScript UI rendered in the
system webview. The UI is presentation only; all business logic lives in Rust modules
(`domain/`, `sim/`, `runtime/`, `chaos/`, `storage/`, `export/`) behind thin, typed Tauri commands.

## Alternatives considered

- **Electron**: mature and consistent Chromium, but ships a full Chromium + Node (~100+ MB,
  higher RAM), and the simulation core would need native addons or a second language.
- **Native Rust GUI (egui/iced)**: single language, but weak for a rich topology canvas, SVG charts
  and canvas-based video frame capture.
- **Flutter / .NET MAUI**: heavier tooling, poorer fit for Rust simulation code and web-based
  SVG/canvas export.

## Consequences

- Positive: small installers, low memory, Rust for determinism/performance/safety, first-class
  sidecar support (ffmpeg), built-in bundler, updater and signing path (Phase 10).
- Negative: the UI runs in different system webviews (WebView2, WKWebView, WebKitGTK), so rendering
  and codec behavior must be tested on all three OSes (CI matrix covers builds). Linux needs
  WebKitGTK installed at build time.
- Follow-up: the WebM/ffmpeg decision will be recorded in a separate ADR (Phase 9).

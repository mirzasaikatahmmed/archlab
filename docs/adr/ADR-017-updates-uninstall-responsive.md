# ADR-017: Updates, uninstall and small screens

- Status: Accepted
- Date: 2026-11-22

## Context

Early use showed three gaps: the update notice said little when a check failed, there was no way to
remove the app from inside it, and the layout stacked the whole sidebar above the content on narrow
windows.

## Decision

1. **Updates.** The notice shows the installed version, the release notes of a newer version, a
   download progress bar and a "Restart now" button (`restart_app` calls Tauri's own restart, so no
   extra plugin). A failed manual check names the cause (the updater throws plain strings as well as
   errors); a failed startup check stays silent. Updates are served from GitHub Releases, so
   they work only from a **published** release of a **public** repository: a draft release has no
   public `latest.json`.
2. **Uninstall.** A quiet "Uninstall ArchLab..." button opens a confirmation that says exactly
   what will happen. It acts only on a copy an installer put in place, decided by pure, tested
   functions (`uninstall::choose`):
   - macOS: a bundle directly in `/Applications` or `~/Applications` is moved to the Trash.
   - Windows: the installer's own `uninstall.exe` next to the program is started; it asks about
     data itself.
   - Linux: an AppImage file is deleted, or the package that owns the program file (found with
     `dpkg -S`) is removed through `pkexec`, which asks for the administrator password.
   - Anything else (a development build, an unpacked download) is refused with an explanation, so the
     button cannot delete a source checkout or an unrelated folder.
     The window is not trusted: `uninstall_app` works the plan out again before acting. Paths reach the
     shell helper as arguments, never inside the script text. Projects and settings are kept unless the
     person ticks "Also delete my projects and settings", which names the folder and says it cannot be
     undone. The macOS and AppImage cleanup runs from a helper after the app quits; the Windows
     uninstaller and Linux package paths are written from the platforms' documentation and have not
     been run.
3. **Small screens.** Below 900 px the sidebar becomes a menu: a top bar with a menu button opens it
   over the content and it closes when something is chosen. The tab row scrolls sideways, mode
   switches wrap, paddings shrink, and touch devices get 40 px controls. The window's minimum size
   dropped from 960x600 to 360x480.
   **Large screens** use the width too: content may grow to 96 rem (it stopped at 64 rem and left a
   third of a 1920 px screen empty), the sidebar widens a little, the Stage's height follows the window
   height (22 to 46 rem), and on screens 1200 px and wider the live run shows the Stage on the left with
   steering, node load and export in a column on the right.

## Consequences

- Uninstall removes only what is named in the confirmation; it never touches anything outside the
  app, the AppImage or the package.
- Removing a package needs `apt`; other package managers are not supported and are refused.

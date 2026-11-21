export type ThemeChoice = "system" | "light" | "dark";

const KEY = "archlab.theme";

/** Reads the saved choice; storage can be unavailable, in which case the system theme applies. */
export function loadTheme(): ThemeChoice {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

/** Returns false when the choice could not be remembered. */
export function saveTheme(choice: ThemeChoice): boolean {
  try {
    localStorage.setItem(KEY, choice);
    return true;
  } catch {
    return false;
  }
}

/** `system` removes the attribute so the OS preference decides. */
export function applyTheme(choice: ThemeChoice): void {
  if (choice === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = choice;
  }
}

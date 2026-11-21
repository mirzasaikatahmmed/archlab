import { useEffect, useState } from "react";
import { applyTheme, loadTheme, saveTheme, type ThemeChoice } from "../lib/theme";
import { Icon, type IconName } from "./Icon";

const CHOICES: { value: ThemeChoice; label: string; icon: IconName }[] = [
  { value: "system", label: "Match system", icon: "monitor" },
  { value: "light", label: "Light", icon: "sun" },
  { value: "dark", label: "Dark", icon: "moon" },
];

export function ThemeToggle() {
  const [choice, setChoice] = useState<ThemeChoice>(loadTheme);

  useEffect(() => {
    applyTheme(choice);
    saveTheme(choice);
  }, [choice]);

  return (
    <div role="group" aria-label="Theme" className="theme-toggle">
      {CHOICES.map((c) => (
        <button
          key={c.value}
          type="button"
          className="icon-btn"
          aria-label={c.label}
          aria-pressed={choice === c.value}
          title={c.label}
          onClick={() => {
            setChoice(c.value);
          }}
        >
          <Icon name={c.icon} />
        </button>
      ))}
    </div>
  );
}
